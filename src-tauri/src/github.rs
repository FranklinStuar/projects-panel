//! Integración con el `gh` CLI del sistema (sin auth propia).
//!
//! Los repos se clonan en el HOST (no en el container): los archivos están
//! bind-montados, así que cualquier cambio se refleja al instante sin reiniciar.
//! Aprovecha la sesión y las SSH keys ya configuradas en la máquina.

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tokio::process::Command;

use crate::config::SiteConfig;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub user: Option<String>,
}

/// Estado de `gh`: instalado y autenticado.
pub async fn status() -> GhStatus {
    let version = Command::new("gh").arg("--version").output().await;
    let installed = matches!(&version, Ok(o) if o.status.success());
    if !installed {
        return GhStatus {
            installed: false,
            authenticated: false,
            user: None,
        };
    }

    let auth = Command::new("gh").args(["auth", "status"]).output().await;
    match auth {
        Ok(o) if o.status.success() => {
            let txt = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            GhStatus {
                installed: true,
                authenticated: true,
                user: parse_user(&txt),
            }
        }
        _ => GhStatus {
            installed: true,
            authenticated: false,
            user: None,
        },
    }
}

/// Extrae el usuario de la salida de `gh auth status`.
fn parse_user(txt: &str) -> Option<String> {
    // formato actual: "✓ Logged in to github.com account NAME (keyring)"
    if let Some(idx) = txt.find("account ") {
        let rest = &txt[idx + "account ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    // formato antiguo: "Logged in to github.com as NAME"
    if let Some(idx) = txt.find(" as ") {
        let rest = &txt[idx + 4..];
        let name: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Carpeta destino dentro de public/ a partir de la ruta relativa del proyecto.
fn dest_abs(site: &SiteConfig, rel_path: &str) -> std::path::PathBuf {
    site.public_dir().join(rel_path)
}

/// Clona un repo en la carpeta del proyecto. `rel_path` es relativo a public/
/// (ej. `wp-content/themes/mi-theme`).
pub async fn clone(site: &SiteConfig, repo: &str, branch: &str, rel_path: &str) -> Result<()> {
    let dest = dest_abs(site, rel_path);
    if dest.exists() {
        return Err(anyhow!("la carpeta ya existe: {rel_path}"));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut args = vec!["repo".to_string(), "clone".to_string(), repo.to_string()];
    args.push(dest.to_string_lossy().to_string());
    if !branch.is_empty() {
        // pasa flags a git tras `--`
        args.push("--".to_string());
        args.push("-b".to_string());
        args.push(branch.to_string());
    }

    let out = Command::new("gh")
        .args(&args)
        .output()
        .await
        .context("ejecutando gh repo clone")?;
    if !out.status.success() {
        return Err(anyhow!(
            "gh repo clone falló: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// `git pull` sobre una carpeta clonada del proyecto.
pub async fn pull(site: &SiteConfig, rel_path: &str, branch: &str) -> Result<String> {
    let dir = dest_abs(site, rel_path);
    if !dir.exists() {
        return Err(anyhow!("la carpeta no existe: {rel_path}"));
    }
    let mut args = vec![
        "-C".to_string(),
        dir.to_string_lossy().to_string(),
        "pull".to_string(),
    ];
    if !branch.is_empty() {
        args.push("origin".to_string());
        args.push(branch.to_string());
    }
    let out = Command::new("git")
        .args(&args)
        .output()
        .await
        .context("ejecutando git pull")?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(anyhow!("git pull falló: {combined}"));
    }
    Ok(combined)
}

/// Borra una carpeta clonada del proyecto (en el host).
pub fn remove_dir(site: &SiteConfig, rel_path: &str) -> Result<()> {
    let dir = dest_abs(site, rel_path);
    // seguridad: solo dentro de public/wp-content
    let wp_content = site.public_dir().join("wp-content");
    let canon = dir.canonicalize().unwrap_or(dir.clone());
    if !canon.starts_with(&wp_content) {
        return Err(anyhow!("ruta fuera de wp-content, no se borra: {rel_path}"));
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("borrando {:?}", dir))?;
    }
    Ok(())
}

/// Propone una ruta relativa según el nombre del repo y el tipo.
pub fn propose_path(kind: &str, repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let name = name.strip_suffix(".git").unwrap_or(name);
    let sub = match kind {
        "theme" => "themes",
        "plugin" => "plugins",
        "muplugin" => "mu-plugins",
        _ => "plugins",
    };
    format!("wp-content/{sub}/{name}")
}

/// Un repo git encontrado en disco bajo `wp-content/`, esté o no registrado en
/// el `config.json` del proyecto y tenga o no remoto en GitHub.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedRepo {
    /// Ruta relativa a public/ (ej. `wp-content/themes/mi-theme`).
    pub path: String,
    /// Nombre de carpeta (último segmento).
    pub name: String,
    /// URL del remoto `origin`, si existe.
    pub remote: Option<String>,
    /// Rama actual.
    pub branch: Option<String>,
    /// ¿Ya está en `github.repos` del config?
    pub registered: bool,
}

/// Devuelve `origin` y la rama actual de un repo git en disco.
async fn git_remote(dir: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "remote", "get-url", "origin"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

async fn git_branch(dir: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Recorre `wp-content/` buscando carpetas con `.git`. No desciende dentro de un
/// repo encontrado (ignora submódulos) ni en `node_modules`/`vendor`. Profundidad
/// limitada para no recorrer árboles enormes (uploads, etc.).
fn find_git_dirs(root: &std::path::Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    walk_git(root, max_depth, &mut found);
    found
}

fn walk_git(dir: &std::path::Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    if dir.join(".git").exists() {
        out.push(dir.to_path_buf());
        return; // no descender dentro del repo
    }
    if depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        match p.file_name().and_then(|n| n.to_str()) {
            Some("node_modules") | Some("vendor") | Some(".git") => continue,
            _ => {}
        }
        walk_git(&p, depth - 1, out);
    }
}

/// Escanea `wp-content/` del proyecto y devuelve los repos git encontrados,
/// marcando cuáles ya están registrados en el config.
pub async fn scan(site: &SiteConfig) -> Vec<DetectedRepo> {
    let wp_content = site.public_dir().join("wp-content");
    if !wp_content.exists() {
        return Vec::new();
    }
    let public = site.public_dir();
    let registered: std::collections::HashSet<&str> =
        site.github.repos.iter().map(|r| r.path.as_str()).collect();

    let mut repos = Vec::new();
    for dir in find_git_dirs(&wp_content, 4) {
        let rel = dir
            .strip_prefix(&public)
            .unwrap_or(&dir)
            .to_string_lossy()
            .to_string();
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel.clone());
        repos.push(DetectedRepo {
            registered: registered.contains(rel.as_str()),
            remote: git_remote(&dir).await,
            branch: git_branch(&dir).await,
            path: rel,
            name,
        });
    }
    repos.sort_by(|a, b| a.path.cmp(&b.path));
    repos
}

/// Lee `origin` y la rama de un repo ya en disco (para registrar un git huérfano).
pub async fn read_repo_meta(site: &SiteConfig, rel_path: &str) -> Result<(String, String)> {
    let dir = dest_abs(site, rel_path);
    if !dir.join(".git").exists() {
        return Err(anyhow!("{rel_path} no es un repo git"));
    }
    let remote = git_remote(&dir).await.unwrap_or_default();
    let branch = git_branch(&dir).await.unwrap_or_else(|| "main".to_string());
    Ok((remote, branch))
}

/// Lanza VSCode (o VSCodium) abriendo `target`. Proceso detached. Prueba varios
/// binarios en orden y usa el primero que exista.
pub fn open_vscode(target: &std::path::Path) -> Result<()> {
    use std::process::Command as StdCommand;
    let candidates = ["code", "codium", "code-insiders", "vscodium"];
    for bin in candidates {
        match StdCommand::new(bin).arg(target).spawn() {
            Ok(_) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(anyhow!("al lanzar {bin}: {err}")),
        }
    }
    Err(anyhow!(
        "no se encontró VSCode (code, codium, code-insiders). Instálalo o ábrelo manualmente en {}",
        target.display()
    ))
}

/// Genera el `<nombre>.code-workspace` del proyecto si no existe y devuelve su
/// ruta. Carpeta principal = `app/public`; se añade cada repo git detectado bajo
/// wp-content como carpeta adicional (multi-root). Si ya existe, no se toca: el
/// usuario es libre de editarlo a mano.
pub async fn ensure_workspace(site: &SiteConfig) -> Result<std::path::PathBuf> {
    let safe: String = site
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let ws = std::path::Path::new(&site.path).join(format!("{safe}.code-workspace"));
    if ws.exists() {
        return Ok(ws);
    }

    let mut folders = vec![serde_json::json!({
        "name": format!("{} (public)", site.name),
        "path": "app/public",
    })];
    for r in scan(site).await {
        folders.push(serde_json::json!({
            "name": r.name,
            "path": std::path::Path::new("app/public").join(&r.path).to_string_lossy(),
        }));
    }
    let doc = serde_json::json!({
        "folders": folders,
        "settings": {},
    });
    std::fs::write(&ws, serde_json::to_string_pretty(&doc)?)
        .with_context(|| format!("escribiendo {:?}", ws))?;
    Ok(ws)
}
