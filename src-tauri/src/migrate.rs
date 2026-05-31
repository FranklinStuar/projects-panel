//! Migración de un proyecto al sistema actual (Fase 4).
//!
//! Una carpeta de `~/panel-wp/` es autosuficiente (ver filosofía en `PLAN.md`).
//! Al copiarla a otro sistema —o tras importar desde LocalWP— el proyecto
//! aparece como `migrationPending`. Migrar lo provisiona aquí: crea la base de
//! datos, regenera `wp-config.php` con las credenciales del panel, importa el
//! último dump de `app/sql/`, regenera el certificado SSL y enciende el sitio.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::config::{write_site_config, SiteConfig};
use crate::docker::DockerManager;
use crate::progress::log;

/// Resultado de migrar: la config actualizada + un aviso opcional para la UI
/// (p. ej. "no había dump").
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    pub site: SiteConfig,
    pub note: Option<String>,
}

pub async fn migrate_site(
    app: &AppHandle,
    docker: &DockerManager,
    site: &SiteConfig,
) -> Result<Migration> {
    log(app, format!("▶ Migrando «{}»…", site.name));
    if !site.public_dir().exists() {
        return Err(anyhow!(
            "falta {:?}: la carpeta del proyecto está incompleta",
            site.public_dir()
        ));
    }

    // 1. DB compartida on-demand + base de datos vacía del proyecto (idempotente).
    log(app, "• Arrancando base de datos y creando el esquema…");
    let db_container = docker.ensure_db(&site.services.db).await?;
    crate::wordpress::create_database(docker, &db_container, site).await?;

    // 2. SSL: generar el certificado ANTES de encender, porque el vhost de
    //    panel-nginx lo referencia y `nginx -s reload` falla si no existe (la CA
    //    de mkcert es local; se regenera en cada sistema).
    if site.services.nginx.ssl {
        log(app, "• Generando certificado SSL (mkcert)…");
        crate::ssl::generate(site).await?;
    }

    // 3. Encender container php + vhost en panel-nginx + reload.
    log(
        app,
        "• Encendiendo el proyecto (la primera vez puede construir la imagen PHP, tarda)…",
    );
    docker.start_site(site).await?;

    // 4. Regenerar wp-config con las credenciales del panel: el origen pudo usar
    //    otro host/disco (otra instalación del panel, o LocalWP).
    log(app, "• Regenerando wp-config.php…");
    crate::wordpress::wp_config_create(docker, site, &db_container).await?;

    // 5. Importar el último dump si existe; si no, el sitio arranca vacío.
    let note = match latest_dump(site) {
        Some(dump) => {
            let mb = std::fs::metadata(&dump).map(|m| m.len() / 1_048_576).unwrap_or(0);
            log(app, format!("• Importando base de datos ({mb} MB), espera…"));
            import_dump(docker, site, &db_container, &dump).await?;
            // El dump pudo venir con otro dominio (p. ej. LocalWP `.local`):
            // fijar home/siteurl al dominio del panel para que el admin funcione.
            log(app, "• Ajustando URLs del sitio…");
            fix_site_url(docker, site).await.ok();
            None
        }
        None => {
            log(app, "• No hay dump: el sitio arranca con la DB vacía.");
            Some("No había dump en app/sql/: el sitio arranca con la base de datos vacía.".to_string())
        }
    };

    // 6. Marcar como migrado.
    let mut updated = site.clone();
    updated.migration_pending = false;
    updated.last_migrated_at = Some(Utc::now().to_rfc3339());
    write_site_config(&updated)?;

    log(app, format!("✓ «{}» migrado y encendido.", site.name));
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

/// Fija `home`/`siteurl` al dominio del panel (el dump pudo traer otro dominio).
async fn fix_site_url(docker: &DockerManager, site: &SiteConfig) -> Result<()> {
    let url = crate::config::endpoint_or_default().site_url(&site.domain, site.services.nginx.ssl);
    for opt in ["home", "siteurl"] {
        crate::wpcli::run(
            docker,
            site,
            &[
                "option".to_string(),
                "update".to_string(),
                opt.to_string(),
                url.clone(),
            ],
        )
        .await?;
    }
    Ok(())
}

/// Importa un dump SQL alimentando el cliente `mysql` del container DB por
/// stdin. Se hace dentro del container DB (socket local, sin TLS) en vez de con
/// `wp db import` desde el container php, cuyo cliente falla la verificación del
/// certificado autofirmado de MySQL 8.
async fn import_dump(
    docker: &DockerManager,
    site: &SiteConfig,
    db_container: &str,
    dump: &Path,
) -> Result<()> {
    let bytes = std::fs::read(dump)
        .with_context(|| format!("leyendo el dump {:?}", dump))?;
    let dbname = &site.services.db.db_name;
    docker
        .exec_stdin(
            db_container,
            vec!["mysql", "-uroot", "-ppanel", dbname],
            &bytes,
        )
        .await?;
    Ok(())
}
