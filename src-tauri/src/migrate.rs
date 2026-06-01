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
use tauri::{AppHandle, Runtime};

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

pub async fn migrate_site<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    site: &SiteConfig,
) -> Result<Migration> {
    // Envuelve el flujo real para que CUALQUIER error se vea en la consola de
    // progreso (`op-log`) con un ✗, además de propagarse a la UI. Sin esto el
    // usuario veía la consola abierta y vacía sin saber qué pasó.
    match run_migration(app, docker, site).await {
        Ok(mig) => Ok(mig),
        Err(err) => {
            log(app, format!("✗ La migración falló: {err:#}"));
            Err(err)
        }
    }
}

async fn run_migration<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    site: &SiteConfig,
) -> Result<Migration> {
    let scheme = if site.services.nginx.ssl { "https" } else { "http" };
    log(app, format!("▶ Migrando «{}» a este sistema.", site.name));
    log(app, format!("  Destino: {scheme}://{}", site.domain));
    if !site.public_dir().exists() {
        return Err(anyhow!(
            "falta {:?}: la carpeta del proyecto está incompleta",
            site.public_dir()
        ));
    }
    log(app, "  Carpeta del proyecto verificada.");

    // 1. DB compartida on-demand + base de datos vacía del proyecto (idempotente).
    log(
        app,
        format!(
            "[1/6] Base de datos: arrancando MySQL {} compartido…",
            site.services.db.version
        ),
    );
    let db_container = docker.ensure_db(&site.services.db).await?;
    log(
        app,
        format!(
            "      Creando el esquema «{}» (si no existe)…",
            site.services.db.db_name
        ),
    );
    crate::wordpress::create_database(docker, &db_container, site).await?;
    log(app, "      ✓ Base de datos lista.");

    // 2. SSL: generar el certificado ANTES de encender, porque el vhost de
    //    panel-nginx lo referencia y `nginx -s reload` falla si no existe (la CA
    //    de mkcert es local; se regenera en cada sistema).
    if site.services.nginx.ssl {
        log(
            app,
            format!("[2/6] SSL: generando certificado mkcert para {}…", site.domain),
        );
        crate::ssl::generate(site).await?;
        log(app, "      ✓ Certificado listo.");
    } else {
        log(app, "[2/6] SSL: desactivado para este proyecto, se omite.");
    }

    // 3. Encender container php + vhost en panel-nginx + reload.
    log(
        app,
        "[3/6] Encendiendo el proyecto (la 1ª vez construye la imagen PHP, puede tardar)…",
    );
    docker.start_site(site).await?;
    log(app, "      ✓ Container del proyecto y nginx arriba.");

    // 4. Regenerar wp-config con las credenciales del panel: el origen pudo usar
    //    otro host/disco (otra instalación del panel, o LocalWP).
    log(app, "[4/6] Regenerando wp-config.php con las credenciales del panel…");
    crate::wordpress::wp_config_create(docker, site, &db_container).await?;
    log(app, "      ✓ wp-config.php regenerado.");

    // 5. Importar el último dump si existe; si no, el sitio arranca vacío.
    let note = match latest_dump(site) {
        Some(dump) => {
            let mb = std::fs::metadata(&dump).map(|m| m.len() / 1_048_576).unwrap_or(0);
            let name = dump.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            log(
                app,
                format!("[5/6] Importando la base de datos ({name}, {mb} MB), espera…"),
            );
            import_dump(docker, site, &db_container, &dump).await?;
            log(app, "      ✓ Dump importado.");
            // El dump pudo venir con otro dominio (p. ej. LocalWP `.local`):
            // fijar home/siteurl al dominio del panel para que el admin funcione.
            log(
                app,
                format!("[6/6] Ajustando URLs del sitio a {scheme}://{}…", site.domain),
            );
            match fix_site_url(docker, site).await {
                Ok(()) => log(app, "      ✓ URLs (home/siteurl) actualizadas."),
                Err(e) => log(
                    app,
                    format!("      ⚠ No se pudieron ajustar las URLs ({e:#}); revísalas en el admin."),
                ),
            }
            None
        }
        None => {
            log(app, "[5/6] No hay dump en app/sql/: el sitio arranca con la DB vacía.");
            Some("No había dump en app/sql/: el sitio arranca con la base de datos vacía.".to_string())
        }
    };

    // 6. Marcar como migrado.
    let mut updated = site.clone();
    updated.migration_pending = false;
    updated.last_migrated_at = Some(Utc::now().to_rfc3339());
    write_site_config(&updated)?;

    log(app, format!("✓ «{}» migrado y encendido — {scheme}://{}", site.name, site.domain));
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
                // Saltar plugins/temas: actualizar una opción no los necesita y un
                // sitio migrado puede traer un plugin que se cuelga al cargar
                // (p. ej. llamada HTTP de licencia), bloqueando toda la migración.
                "--skip-plugins".to_string(),
                "--skip-themes".to_string(),
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
///
/// Vía el CLI `docker exec -i` (excepción justificada al "Docker solo por
/// bollard", como ya lo es `docker build` de la imagen php): el `exec_stdin` de
/// bollard se cuelga con dumps grandes —su stream de salida no emite `None` al
/// terminar un exec con stdin adjunto—, mientras que el CLI importa 7&nbsp;MB en
/// ~15&nbsp;s. `wait_with_output` drena stdout/stderr a la vez que escribimos el
/// dump por stdin, evitando el deadlock clásico de pipes.
async fn import_dump(
    _docker: &DockerManager,
    site: &SiteConfig,
    db_container: &str,
    dump: &Path,
) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let bytes = std::fs::read(dump)
        .with_context(|| format!("leyendo el dump {:?}", dump))?;
    let dbname = site.services.db.db_name.clone();

    let mut child = Command::new("docker")
        .args(["exec", "-i", db_container, "mysql", "-uroot", "-ppanel", &dbname])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("lanzando `docker exec` para importar el dump")?;

    // Escribir el dump por stdin en una tarea aparte mientras `wait_with_output`
    // drena stdout/stderr: si volcáramos todo sin leer, el pipe se llenaría y se
    // colgaría.
    let mut stdin = child.stdin.take().expect("stdin piped");
    let writer = tokio::spawn(async move {
        stdin.write_all(&bytes).await.ok();
        stdin.shutdown().await.ok(); // EOF → mysql termina
    });

    let output = child
        .wait_with_output()
        .await
        .context("esperando a `docker exec` (import)")?;
    writer.await.ok();

    if !output.status.success() {
        return Err(anyhow!(
            "import del dump falló en {db_container}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}
