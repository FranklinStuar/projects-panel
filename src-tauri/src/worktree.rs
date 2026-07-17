//! Worktree-projects: probar una rama de un theme/plugin en aislamiento sin
//! duplicar todo el WordPress.
//!
//! Un worktree-project es un `SiteConfig` normal con `worktree_of` poblado. NO
//! copia el código del padre: el `public` del padre se comparte por **montaje
//! Docker** (ver `docker::create_php_container`) y solo se sobreponen
//!   - el repo objetivo (theme/plugin), que es un **`git worktree`** sobre una
//!     rama nueva, almacenado en `{path}/wt/{basename}`, y
//!   - un `wp-config.php` propio (`{path}/wp-config.php`) con el dominio y la DB
//!     del worktree.
//! nginx sirve los estáticos del padre y, para el objetivo, un `alias` al
//! worktree (ver `nginx::render_vhost`).
//!
//! La DB puede compartirse con el padre (constantes `WP_HOME`/`WP_SITEURL` en el
//! wp-config propio, sin tocar la DB) o copiarse a un esquema propio (dump +
//! import del padre). Al eliminar el worktree se hace `git worktree remove` —la
//! rama queda en el repo del padre para seguir trabajándola— y se borra la
//! carpeta: no queda rastro de que existió un proyecto de prueba.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::path::Path;
use tokio::process::Command;
use uuid::Uuid;

use crate::config::{
    self, path_basename, DbService, GithubConfig, NginxService, PhpService, Services, SiteConfig,
    WorktreeInfo,
};
use crate::docker::DockerManager;
use crate::progress::log;

/// Crea un worktree-project del repo `target_path` del proyecto `parent_id` sobre
/// la rama `branch` (se crea desde `base_branch`, o desde la rama actual del repo
/// si `base_branch` es `None`). `shared_db`: compartir el esquema del padre o
/// copiarlo. Devuelve el `SiteConfig` del worktree, ya encendido.
pub async fn create_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    docker: &DockerManager,
    parent_id: &str,
    target_path: &str,
    branch: &str,
    base_branch: Option<&str>,
    shared_db: bool,
) -> Result<SiteConfig> {
    match run_create(app, docker, parent_id, target_path, branch, base_branch, shared_db).await {
        Ok(site) => Ok(site),
        Err(err) => {
            log(app, format!("✗ Error creando el worktree: {err:#}"));
            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_create<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    docker: &DockerManager,
    parent_id: &str,
    target_path: &str,
    branch: &str,
    base_branch: Option<&str>,
    shared_db: bool,
) -> Result<SiteConfig> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(anyhow!("debes indicar el nombre de la rama del worktree"));
    }
    // git rechaza ramas con espacios; suele ser un pegado accidental del comando
    // entero («git checkout -b feature/x») en vez de solo «feature/x». Detectamos
    // ese caso y sugerimos la rama ya extraída del pegado.
    if let Some(bad) = invalid_branch_reason(branch) {
        let hint = guess_branch(branch);
        return Err(anyhow!(
            "«{branch}» no es un nombre de rama válido ({bad}).\n\
             Escribe SOLO el nombre de la rama (ej. «feature/mi-cambio»), sin «git checkout -b» ni comillas.\
             {}",
            match hint {
                Some(g) => format!("\n¿Quisiste decir «{g}»?"),
                None => String::new(),
            }
        ));
    }
    let target_path = target_path.trim().trim_matches('/');

    // -- Padre + validación del repo objetivo ---------------------------------
    let parent = config::find_site(parent_id)?
        .ok_or_else(|| anyhow!("proyecto padre {parent_id} no encontrado"))?;
    let parent_dirname = crate::nginx::project_dirname(&parent);
    let repo_dir = parent.public_dir().join(target_path);
    if !repo_dir.join(".git").exists() {
        return Err(anyhow!(
            "{target_path} no es un repo git en «{}» (clónalo/regístralo antes de crear un worktree)",
            parent.name
        ));
    }
    let target_name = path_basename(target_path).to_string();

    // -- Slug / path / dominio libres -----------------------------------------
    let base_slug = format!("{parent_dirname}-{}", slugify(branch));
    let root = config::projects_root()?;
    let existing = config::load_all_sites()?;
    let (slug, path, domain) = find_free_slot(&root, &base_slug, &existing);
    let id = Uuid::new_v4().to_string();

    // DB: compartir el esquema del padre, o uno propio derivado del slug.
    let db_name = if shared_db {
        parent.services.db.db_name.clone()
    } else {
        format!("{}_db", slug.replace('-', "_"))
    };

    log(
        app,
        format!(
            "▶ Creando worktree «{branch}» de «{}» ({target_path}).",
            parent.name
        ),
    );
    log(
        app,
        format!(
            "  Dominio: {domain} · BD: {}",
            if shared_db { "compartida con el padre" } else { "copia propia" }
        ),
    );

    let site = SiteConfig {
        id: id.clone(),
        name: format!("{} · {branch}", parent.name),
        path: path.to_string_lossy().to_string(),
        domain: domain.clone(),
        group: parent.group.clone(),
        created_at: Utc::now().to_rfc3339(),
        services: Services {
            php: PhpService { version: parent.services.php.version.clone() },
            nginx: NginxService { ssl: parent.services.nginx.ssl },
            db: DbService {
                db_type: parent.services.db.db_type,
                version: parent.services.db.version.clone(),
                db_name: db_name.clone(),
            },
        },
        github: GithubConfig::default(),
        one_click_admin: parent.one_click_admin,
        xdebug_enabled: parent.xdebug_enabled,
        headless: false,
        frontend_framework: None,
        minio: false,
        migration_pending: false,
        last_migrated_at: None,
        clone_of: None,
        worktree_of: Some(WorktreeInfo {
            parent_id: parent_id.to_string(),
            parent_dirname: parent_dirname.clone(),
            target_path: target_path.to_string(),
            branch: branch.to_string(),
            shared_db,
            created_at: Utc::now().to_rfc3339(),
        }),
        snapshot_excludes: vec![],
    };

    // Pasos [1/7]..[7/7] en un bloque: si algo falla a medias, se limpia todo
    // (container/vhost/carpeta) para no dejar proyectos huérfanos que luego
    // rompan nginx (config.json ya escrito pero sin cert SSL, sin worktree, …).
    let build = async {
    // -- 1. Carpeta + php.ini + config.json -----------------------------------
    log(app, "[1/7] Preparando carpeta del worktree…");
    crate::wordpress::create_dirs(&site)?;
    crate::wordpress::write_php_ini(&site)?;
    std::fs::create_dir_all(site.worktree_root())?;
    // wp-config.php propio debe EXISTIR como archivo antes de montarlo (si no,
    // Docker crea un directorio en su lugar). Se rellena con `wp config create`.
    std::fs::write(site.worktree_wp_config(), b"<?php\n")?;
    config::write_site_config(&site)?;
    log(app, "      ✓ Carpeta lista.");

    // -- 2. git worktree del repo objetivo ------------------------------------
    let dest = site.worktree_root().join(&target_name);
    log(app, format!("[2/7] Creando git worktree → rama «{branch}»…"));
    add_worktree(&repo_dir, &dest, branch, base_branch).await?;
    log(app, "      ✓ Worktree creado (la rama vive en el repo del padre).");

    // -- 3. Base de datos (compartida o copia) --------------------------------
    let db_container = docker.ensure_db(&site.services.db).await?;
    if shared_db {
        log(app, "[3/7] Base de datos: compartida con el padre (sin copia).");
    } else {
        log(app, "[3/7] Base de datos: creando esquema propio y copiando del padre…");
        crate::wordpress::create_database(docker, &db_container, &site).await?;
        // Volcar el padre (necesita su engine DB arriba, que `ensure_db` ya garantiza
        // por ser el mismo container) e importar en el esquema del worktree.
        let dump_bytes = crate::backup::dump_bytes(docker, &parent)
            .await
            .context("volcando la base de datos del padre para la copia")?;
        let dump_path = site.sql_dir().join(format!(
            "from-parent-{}.sql",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::write(&dump_path, &dump_bytes)?;
        crate::migrate::import_dump(app, docker, &site, &db_container, &dump_path).await?;
        log(app, "      ✓ Copia de la base de datos lista.");
    }

    // -- 4. SSL ---------------------------------------------------------------
    if site.services.nginx.ssl {
        log(app, format!("[4/7] Generando certificado SSL para {domain}…"));
        crate::ssl::generate(&site).await?;
        log(app, "      ✓ Certificado listo.");
    } else {
        log(app, "[4/7] SSL desactivado, se omite.");
    }

    // -- 5. Encender (container php con montajes del padre + vhost) ------------
    log(app, "[5/7] Arrancando el worktree (container PHP + nginx)…");
    docker.start_site(&site).await?;
    log(app, "      ✓ Worktree arriba.");

    // -- 6. wp-config propio --------------------------------------------------
    log(app, "[6/7] Escribiendo wp-config.php del worktree…");
    crate::wordpress::wp_config_create(docker, &site, &db_container).await?;
    if shared_db {
        // No mutar la DB del padre: el dominio del worktree se fija por constantes
        // en el wp-config propio, que sobrescriben home/siteurl en tiempo de
        // ejecución (la DB sigue apuntando al dominio del padre).
        let url = config::endpoint_or_default().site_url(&site.domain, site.services.nginx.ssl);
        for cst in ["WP_HOME", "WP_SITEURL"] {
            crate::wpcli::run(
                docker,
                &site,
                &[
                    "config".to_string(),
                    "set".to_string(),
                    cst.to_string(),
                    url.clone(),
                    "--type=constant".to_string(),
                ],
            )
            .await?;
        }
        log(app, "      ✓ URLs fijadas por constantes (DB del padre intacta).");
    }

    // -- 7. Copia: ajustar URLs en la DB propia -------------------------------
    if !shared_db {
        log(app, format!("[7/7] Ajustando URLs de la copia a {domain}…"));
        match crate::migrate::fix_site_url(docker, &site).await {
            Ok(()) => log(app, "      ✓ URLs ajustadas."),
            Err(err) => log(
                app,
                format!("      ⚠ No se pudieron ajustar las URLs ({err:#}); revísalas en el admin."),
            ),
        }
    } else {
        log(app, "[7/7] Listo.");
    }

    log(app, format!("✓ Worktree listo → {domain}"));
    Ok::<(), anyhow::Error>(())
    };

    if let Err(err) = build.await {
        log(app, "  Limpiando el worktree a medio crear…".to_string());
        docker.remove_container(&site.container_name()).await.ok();
        crate::nginx::remove_vhost(&site).ok();
        std::fs::remove_dir_all(&site.path).ok();
        return Err(err);
    }
    Ok(site)
}

/// Elimina un worktree-project: lo apaga, hace `git worktree remove` (la rama
/// queda en el repo del padre), borra el esquema si era una copia y elimina la
/// carpeta. `delete_branch`: además borrar la rama del repo del padre.
pub async fn remove_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    docker: &DockerManager,
    id: &str,
    delete_branch: bool,
) -> Result<()> {
    use crate::progress::log;

    let all = config::load_all_sites()?;
    let site = all
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| anyhow!("worktree {id} no encontrado"))?;
    let wt = site
        .worktree_of
        .clone()
        .ok_or_else(|| anyhow!("el proyecto {id} no es un worktree"))?;
    let parent = config::find_site(&wt.parent_id)?
        .ok_or_else(|| anyhow!("proyecto padre {} no encontrado", wt.parent_id))?;
    let repo_dir = parent.public_dir().join(&wt.target_path);
    let dest = site.worktree_root().join(path_basename(&wt.target_path));

    log(app, format!("▶ Eliminando worktree «{}».", wt.branch));

    // Apagar + quitar vhost/container.
    log(app, "Apagando el worktree…");
    docker.stop_site(&site, &all).await.ok();
    docker.remove_container(&site.container_name()).await.ok();

    // git worktree remove: limpia la metadata en el repo del padre; la rama queda.
    log(app, "Quitando el git worktree (la rama se conserva)…");
    remove_git_worktree(&repo_dir, &dest).await.ok();
    if delete_branch {
        log(app, format!("Borrando la rama «{}»…", wt.branch));
        delete_git_branch(&repo_dir, &wt.branch).await.ok();
    }

    // Copia: borrar el esquema propio del servidor compartido. ¡NUNCA si es
    // compartida! (sería la DB del padre).
    if !wt.shared_db {
        log(app, format!("Borrando el esquema «{}»…", site.services.db.db_name));
        if let Ok(db_container) = docker.ensure_db(&site.services.db).await {
            crate::wordpress::drop_database(docker, &db_container, &site).await.ok();
        }
        docker.teardown_unused_shared(&site, &all).await.ok();
    }

    // Borrar la carpeta del worktree: no queda rastro del proyecto de prueba.
    log(app, "Borrando la carpeta del worktree…");
    std::fs::remove_dir_all(&site.path)
        .with_context(|| format!("borrando {}", site.path))?;

    log(app, "✓ Worktree eliminado. La rama sigue en el proyecto principal.");
    Ok(())
}

/// Worktrees de un proyecto padre (sitios con `worktree_of.parent_id == parent_id`).
pub fn list_worktrees(parent_id: &str) -> Result<Vec<SiteConfig>> {
    Ok(config::load_all_sites()?
        .into_iter()
        .filter(|s| s.worktree_of.as_ref().map(|w| w.parent_id == parent_id).unwrap_or(false))
        .collect())
}

// ---------------------------------------------------------------------------
// git helpers
// ---------------------------------------------------------------------------

/// `git -C <repo> worktree add [-b <branch> <dest> <base>] | [<dest> <branch>]`.
/// Crea la rama nueva desde `base` (o la rama actual). Si la rama ya existe,
/// reintenta haciendo checkout de la existente en el worktree.
async fn add_worktree(
    repo: &Path,
    dest: &Path,
    branch: &str,
    base: Option<&str>,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dest_s = dest.to_string_lossy().to_string();

    // Poda registros obsoletos: si un intento anterior falló a mitad, el repo del
    // padre puede tener este dest «missing but already registered» y `add` se
    // niega. `prune` es idempotente y no toca worktrees vivos.
    git(repo, &["worktree", "prune"]).await.ok();

    // Intento 1: crear rama nueva.
    let mut args = vec!["worktree", "add", "-b", branch, &dest_s];
    if let Some(b) = base {
        if !b.trim().is_empty() {
            args.push(b);
        }
    }
    let out = git(repo, &args).await?;
    if out.ok {
        return Ok(());
    }

    // Intento 2: la rama ya existe → checkout de la existente.
    let out2 = git(repo, &["worktree", "add", &dest_s, branch]).await?;
    if out2.ok {
        return Ok(());
    }
    Err(anyhow!(
        "git worktree add falló: {} {}",
        out.stderr.trim(),
        out2.stderr.trim()
    ))
}

async fn remove_git_worktree(repo: &Path, dest: &Path) -> Result<()> {
    let dest_s = dest.to_string_lossy().to_string();
    let out = git(repo, &["worktree", "remove", "--force", &dest_s]).await?;
    if out.ok {
        return Ok(());
    }
    // Si la carpeta ya no está, al menos podar la metadata.
    git(repo, &["worktree", "prune"]).await.ok();
    Ok(())
}

async fn delete_git_branch(repo: &Path, branch: &str) -> Result<()> {
    let out = git(repo, &["branch", "-D", branch]).await?;
    if !out.ok {
        return Err(anyhow!("git branch -D falló: {}", out.stderr.trim()));
    }
    Ok(())
}

struct GitOut {
    ok: bool,
    #[allow(dead_code)]
    stdout: String,
    stderr: String,
}

async fn git(repo: &Path, args: &[&str]) -> Result<GitOut> {
    let mut full = vec!["-C".to_string(), repo.to_string_lossy().to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let out = Command::new("git")
        .args(&full)
        .output()
        .await
        .with_context(|| format!("ejecutando git {}", args.join(" ")))?;
    Ok(GitOut {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

// ---------------------------------------------------------------------------
// slug / slot (DNS-safe, sin colisión de path/dominio)
// ---------------------------------------------------------------------------

/// Motivo por el que `branch` no es un nombre de rama git válido, o `None` si lo
/// es. Cubre los casos que rompen en la práctica (pegar el comando entero, dejar
/// espacios/comillas). No reimplementa todas las reglas de `git check-ref-format`.
fn invalid_branch_reason(branch: &str) -> Option<&'static str> {
    if branch.contains(char::is_whitespace) {
        Some("no puede contener espacios")
    } else if branch.starts_with('-') {
        Some("no puede empezar por «-»")
    } else if branch.contains("..") {
        Some("no puede contener «..»")
    } else if branch.contains(['~', '^', ':', '?', '*', '[', '\\', '"', '\'']) {
        Some("contiene caracteres no permitidos")
    } else {
        None
    }
}

/// Intenta recuperar la rama de un pegado tipo «git checkout -b feature/x»: quita
/// el comando y las comillas y devuelve el último token con pinta de rama.
fn guess_branch(pasted: &str) -> Option<String> {
    let cand = pasted
        .split_whitespace()
        .rev()
        .map(|t| t.trim_matches(['"', '\'']))
        .find(|t| !t.is_empty() && !t.starts_with('-'))?;
    (invalid_branch_reason(cand).is_none() && cand != pasted).then(|| cand.to_string())
}

/// Slug DNS-safe: minúsculas, alfanumérico y guiones. Vacío → "wt".
fn slugify(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut prev_dash = false;
    for ch in label.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() { "wt".to_string() } else { s }
}

/// (slug, path, domain) libres partiendo de `base` (evita colisión de carpeta y
/// de dominio con proyectos existentes).
fn find_free_slot(
    root: &Path,
    base: &str,
    existing: &[SiteConfig],
) -> (String, std::path::PathBuf, String) {
    let domains: std::collections::HashSet<&str> =
        existing.iter().map(|s| s.domain.as_str()).collect();
    for n in 0u32..=99 {
        let slug = if n == 0 { base.to_string() } else { format!("{base}-{n}") };
        let path = root.join(&slug);
        let domain = format!("{slug}.test");
        if !path.exists() && !domains.contains(domain.as_str()) {
            return (slug.clone(), path, domain);
        }
    }
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();
    let slug = format!("{base}-{short}");
    let domain = format!("{slug}.test");
    (slug.clone(), root.join(&slug), domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valida_rama_y_sugiere() {
        // El pegado del comando entero es inválido y se sugiere la rama real.
        let pasted = "git checkout -b feature/franklinp/sc-8300/uws-new-page";
        assert!(invalid_branch_reason(pasted).is_some());
        assert_eq!(guess_branch(pasted).as_deref(), Some("feature/franklinp/sc-8300/uws-new-page"));
        // Comillas alrededor → se limpian.
        assert_eq!(guess_branch("\"feature/x\"").as_deref(), Some("feature/x"));
        // Una rama válida no dispara error ni sugerencia (no hay nada que corregir).
        assert!(invalid_branch_reason("feature/x").is_none());
        assert_eq!(guess_branch("feature/x"), None);
    }

    #[test]
    fn slugify_ramas() {
        assert_eq!(slugify("feat/nueva-cabecera"), "feat-nueva-cabecera");
        assert_eq!(slugify("BUGFIX_123"), "bugfix-123");
        assert_eq!(slugify("///"), "wt");
    }

    #[test]
    fn find_free_slot_evita_colisiones() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("site-feat")).unwrap();
        let (slug, _, domain) = find_free_slot(tmp.path(), "site-feat", &[]);
        assert_eq!(slug, "site-feat-1");
        assert_eq!(domain, "site-feat-1.test");
    }

    #[test]
    fn path_basename_objetivo() {
        assert_eq!(path_basename("wp-content/themes/mi-theme"), "mi-theme");
        assert_eq!(path_basename("wp-content/plugins/x/"), "x");
    }
}
