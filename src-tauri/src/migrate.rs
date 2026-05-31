//! Migración de un proyecto al sistema actual (Fase 4).
//!
//! Una carpeta de `~/panel-wp/` es autosuficiente (ver filosofía en `PLAN.md`).
//! Al copiarla a otro sistema —o tras importar desde LocalWP— el proyecto
//! aparece como `migrationPending`. Migrar lo provisiona aquí: crea la base de
//! datos, regenera `wp-config.php` con las credenciales del panel, importa el
//! último dump de `app/sql/`, regenera el certificado SSL y enciende el sitio.

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::{write_site_config, SiteConfig};
use crate::docker::DockerManager;

/// Resultado de migrar: la config actualizada + un aviso opcional para la UI
/// (p. ej. "no había dump").
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    pub site: SiteConfig,
    pub note: Option<String>,
}

pub async fn migrate_site(docker: &DockerManager, site: &SiteConfig) -> Result<Migration> {
    if !site.public_dir().exists() {
        return Err(anyhow!(
            "falta {:?}: la carpeta del proyecto está incompleta",
            site.public_dir()
        ));
    }

    // 1. DB compartida on-demand + base de datos vacía del proyecto (idempotente).
    let db_container = docker.ensure_db(&site.services.db).await?;
    crate::wordpress::create_database(docker, &db_container, site).await?;

    // 2. Encender container php + vhost en panel-nginx + reload.
    docker.start_site(site).await?;

    // 3. Regenerar wp-config con las credenciales del panel: el origen pudo usar
    //    otro host/disco (otra instalación del panel, o LocalWP).
    crate::wordpress::wp_config_create(docker, site, &db_container).await?;

    // 4. Importar el último dump si existe; si no, el sitio arranca vacío.
    let note = match latest_dump(site) {
        Some(dump) => {
            import_dump(docker, site, &dump).await?;
            None
        }
        None => Some(
            "No había dump en app/sql/: el sitio arranca con la base de datos vacía.".to_string(),
        ),
    };

    // 5. SSL: regenerar el certificado para este sistema (la CA de mkcert es local).
    if site.services.nginx.ssl {
        crate::ssl::generate(site).await?;
        docker.reload_nginx().await.ok();
    }

    // 6. Marcar como migrado.
    let mut updated = site.clone();
    updated.migration_pending = false;
    updated.last_migrated_at = Some(Utc::now().to_rfc3339());
    write_site_config(&updated)?;

    Ok(Migration {
        site: updated,
        note,
    })
}

/// Dump `.sql` más reciente de `app/sql/` (por fecha de modificación).
fn latest_dump(site: &SiteConfig) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(site.sql_dir()).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            newest = Some((mtime, path));
        }
    }
    newest.map(|(_, p)| p)
}

/// Importa un dump SQL vía WP-CLI. `app/sql/` no está montado en el container,
/// así que se copia el dump a la raíz pública (montada en `/var/www/html`), se
/// importa y se borra.
async fn import_dump(docker: &DockerManager, site: &SiteConfig, dump: &Path) -> Result<()> {
    let staged_name = "panel-import.sql";
    let staged = site.public_dir().join(staged_name);
    std::fs::copy(dump, &staged)?;
    let res = crate::wpcli::run(
        docker,
        site,
        &[
            "db".to_string(),
            "import".to_string(),
            format!("/var/www/html/{staged_name}"),
        ],
    )
    .await;
    std::fs::remove_file(&staged).ok();
    res.map(|_| ())
}
