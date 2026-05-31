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
    let sub = if kind == "theme" { "themes" } else { "plugins" };
    format!("wp-content/{sub}/{name}")
}
