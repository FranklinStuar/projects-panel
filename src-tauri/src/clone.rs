//! Creación de clones temporales desde un punto de guardado.
//!
//! Un clone es un `SiteConfig` normal con `clone_of` poblado. Comparte el engine
//! DB y nginx compartidos; solo añade 1 container php y 1 schema DB extra.
//! Los uploads viejos se sirven vía fallback nginx desde el principal (ro);
//! los nuevos se almacenan en la carpeta del clone (rw).

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::path::Path;
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

use crate::config::{
    CloneInfo, DbService, GithubConfig, NginxService, PhpService, Services, SiteConfig,
};
use crate::docker::DockerManager;
use crate::progress::log;

pub async fn create_clone<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    parent_id: &str,
    snapshot_id: &str,
) -> Result<SiteConfig> {
    match run(app, docker, parent_id, snapshot_id).await {
        Ok(site) => Ok(site),
        Err(err) => {
            log(app, format!("✗ Error creando el clone: {err:#}"));
            Err(err)
        }
    }
}

async fn run<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    parent_id: &str,
    snapshot_id: &str,
) -> Result<SiteConfig> {
    // -- Cargar padre y snapshot -----------------------------------------------
    let parent = crate::config::find_site(parent_id)?
        .ok_or_else(|| anyhow!("proyecto padre {parent_id} no encontrado"))?;

    let snap_dir = crate::snapshot::snapshot_dir(&parent, snapshot_id);
    let meta: crate::snapshot::SnapshotMeta =
        serde_json::from_str(&std::fs::read_to_string(snap_dir.join("meta.json"))
            .with_context(|| format!("snapshot {snapshot_id} no encontrado"))?)
        .context("parseando meta.json del snapshot")?;

    let parent_dirname = crate::nginx::project_dirname(&parent);
    let root = crate::config::projects_root()?;
    let existing = crate::config::load_all_sites()?;

    // -- Derivar slug / path / dominio libre -----------------------------------
    // El nombre y el slug del clone se basan en la etiqueta del punto de guardado.
    let label_slug = slugify(&meta.label);
    let base_slug = format!("{parent_dirname}-{label_slug}");
    let (clone_slug, clone_path, clone_domain) =
        find_free_slot(&root, &base_slug, &existing);
    let clone_id = Uuid::new_v4().to_string();
    let db_name = format!("{}_db", clone_slug.replace('-', "_"));

    log(
        app,
        format!(
            "▶ Creando clone de «{}» desde «{}».",
            parent.name, meta.label
        ),
    );
    log(app, format!("  Dominio: {clone_domain}"));

    let site = SiteConfig {
        id: clone_id.clone(),
        name: meta.label.clone(),
        path: clone_path.to_string_lossy().to_string(),
        domain: clone_domain.clone(),
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
        clone_of: Some(CloneInfo {
            parent_id: parent_id.to_string(),
            parent_dirname: parent_dirname.clone(),
            snapshot_id: snapshot_id.to_string(),
            created_at: Utc::now().to_rfc3339(),
        }),
    };

    // -- 1. Estructura de carpetas + php.ini + config.json --------------------
    log(app, "[1/8] Preparando carpeta del clone…");
    crate::wordpress::create_dirs(&site)?;
    crate::wordpress::write_php_ini(&site)?;
    crate::config::write_site_config(&site)?;
    log(app, "      ✓ Carpeta lista.");

    // -- 2. Extraer código del snapshot ---------------------------------------
    log(app, "[2/8] Extrayendo código del snapshot…");
    let code_tar = snap_dir.join("code.tar.zst");
    let public = site.public_dir();
    let status = tokio::process::Command::new("tar")
        .args([
            "--zstd",
            "-xf",
            code_tar.to_str().unwrap(),
            "-C",
            public.to_str().context("ruta public inválida")?,
        ])
        .status()
        .await
        .context("extrayendo código del snapshot")?;
    if !status.success() {
        return Err(anyhow!("tar falló al extraer el snapshot de código"));
    }
    // Directorio de uploads vacío (rw para archivos nuevos del clone).
    std::fs::create_dir_all(public.join("wp-content").join("uploads"))?;
    log(app, "      ✓ Código extraído.");

    // -- 3. Engine DB + schema del clone --------------------------------------
    log(app, "[3/8] Creando base de datos del clone…");
    let db_container = docker.ensure_db(&site.services.db).await?;
    crate::wordpress::create_database(docker, &db_container, &site).await?;
    log(app, "      ✓ Base de datos lista.");

    // -- 4. Importar dump del snapshot ----------------------------------------
    let db_path = snap_dir.join("db.sql");
    let db_mb = std::fs::metadata(&db_path)
        .map(|m| m.len() / 1_048_576)
        .unwrap_or(0);
    log(app, format!("[4/8] Importando base de datos ({db_mb} MB)…"));
    crate::migrate::import_dump(app, docker, &site, &db_container, &db_path).await?;
    log(app, "      ✓ Base de datos importada.");

    // -- 5. Mu-plugins (mailpit + auto-login) ---------------------------------
    log(app, "[5/8] Sincronizando plugins del panel…");
    crate::wordpress::sync_mu_plugins(&site)?;

    // -- 6. Certificado SSL ---------------------------------------------------
    if site.services.nginx.ssl {
        log(app, format!("[6/8] Generando certificado SSL para {clone_domain}…"));
        crate::ssl::generate(&site).await?;
        log(app, "      ✓ Certificado listo.");
    } else {
        log(app, "[6/8] SSL desactivado, se omite.");
    }

    // -- 7. Encender container PHP + vhost + nginx ----------------------------
    log(app, "[7/8] Arrancando el clone (container PHP + nginx)…");
    docker.start_site(&site).await?;
    log(app, "      ✓ Clone arriba.");

    // -- 8. wp-config + ajustar URLs al dominio del clone ---------------------
    log(app, "[8/8] Configurando WordPress del clone…");
    crate::wordpress::wp_config_create(docker, &site, &db_container).await?;
    match crate::migrate::fix_site_url(docker, &site).await {
        Ok(()) => log(app, "      ✓ URLs ajustadas al dominio del clone."),
        Err(err) => log(
            app,
            format!(
                "      ⚠ No se pudieron ajustar las URLs ({err:#}); revísalas en el admin."
            ),
        ),
    }

    log(app, format!("✓ Clone listo → {clone_domain}"));
    Ok(site)
}

/// Convierte una etiqueta libre en un slug DNS-safe: minúsculas, alfanumérico y
/// guiones, sin guiones repetidos ni en los extremos. Vacío → "clone".
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
    if s.is_empty() {
        "clone".to_string()
    } else {
        s
    }
}

/// Devuelve (slug, path, domain) libres para el clone (sin colisión de path ni
/// de dominio con proyectos existentes), partiendo de `base`.
fn find_free_slot(
    root: &Path,
    base: &str,
    existing: &[SiteConfig],
) -> (String, std::path::PathBuf, String) {
    let existing_domains: std::collections::HashSet<&str> =
        existing.iter().map(|s| s.domain.as_str()).collect();

    // Probar base, luego base-2, base-3, ...
    for n in 0u32..=99 {
        let slug = if n == 0 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let path = root.join(&slug);
        let domain = format!("{slug}.test");
        if !path.exists() && !existing_domains.contains(domain.as_str()) {
            return (slug.clone(), path, domain);
        }
    }

    // Fallback con UUID corto
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();
    let slug = format!("{base}-{short}");
    let domain = format!("{slug}.test");
    (slug.clone(), root.join(&slug), domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DbService, DbType, NginxService, PhpService, Services};

    fn mock_site(dirname: &str, domain: &str) -> SiteConfig {
        SiteConfig {
            id: Uuid::new_v4().to_string(),
            name: dirname.into(),
            path: format!("/home/u/panel-wp/{dirname}"),
            domain: domain.into(),
            group: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            services: Services {
                php: PhpService { version: "8.3".into() },
                nginx: NginxService { ssl: true },
                db: DbService {
                    db_type: DbType::Mysql,
                    version: "8.0".into(),
                    db_name: "test_db".into(),
                },
            },
            github: GithubConfig::default(),
            one_click_admin: true,
            xdebug_enabled: false,
            headless: false,
            frontend_framework: None,
            minio: false,
            migration_pending: false,
            last_migrated_at: None,
            clone_of: None,
        }
    }

    #[test]
    fn find_free_slot_base_libre() {
        let tmp = tempfile::tempdir().unwrap();
        let (slug, path, domain) = find_free_slot(tmp.path(), "mysite-v1", &[]);
        assert_eq!(slug, "mysite-v1");
        assert_eq!(path, tmp.path().join("mysite-v1"));
        assert_eq!(domain, "mysite-v1.test");
    }

    #[test]
    fn find_free_slot_evita_colision_path() {
        let tmp = tempfile::tempdir().unwrap();
        // Crear carpeta base para forzar colisión.
        std::fs::create_dir(tmp.path().join("mysite-v1")).unwrap();
        let (slug, _, domain) = find_free_slot(tmp.path(), "mysite-v1", &[]);
        assert_eq!(slug, "mysite-v1-1");
        assert_eq!(domain, "mysite-v1-1.test");
    }

    #[test]
    fn find_free_slot_evita_colision_dominio() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = vec![mock_site("other", "mysite-v1.test")];
        let (slug, _, domain) = find_free_slot(tmp.path(), "mysite-v1", &existing);
        // Path libre pero domain colisiona → siguiente.
        assert_eq!(slug, "mysite-v1-1");
        assert_eq!(domain, "mysite-v1-1.test");
    }

    #[test]
    fn slugify_etiquetas() {
        assert_eq!(slugify("Antes de actualizar"), "antes-de-actualizar");
        assert_eq!(slugify("  v2.0 / final  "), "v2-0-final");
        assert_eq!(slugify("¡¡¡"), "clone");
        assert_eq!(slugify(""), "clone");
        assert_eq!(slugify("Plugin WooCommerce!!"), "plugin-woocommerce");
    }

    #[test]
    fn db_name_derivacion() {
        // slug "mysite-clone" → db_name "mysite_clone_db"
        let slug = "mysite-clone";
        let db_name = format!("{}_db", slug.replace('-', "_"));
        assert_eq!(db_name, "mysite_clone_db");
    }
}
