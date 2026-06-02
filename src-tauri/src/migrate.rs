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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};

use crate::config::{write_site_config, SiteConfig};
use crate::docker::DockerManager;
use crate::progress::{log, log_progress};

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

    // mu-plugins del panel (mailpit + auto-login): un import de LocalWP no los
    // trae y una copia de otro sistema puede traerlos desfasados. (Re)inyectarlos
    // garantiza que el auto-login al admin funcione igual que en un proyecto
    // creado en el panel.
    crate::wordpress::sync_mu_plugins(site)?;
    log(app, "  Plugins del panel (mailpit, auto-login) sincronizados.");

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
            import_dump(app, docker, site, &db_container, &dump).await?;
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
/// ~15&nbsp;s.
///
/// El import puede colgarse (mysql deja de leer stdin, o el exec no termina). Para
/// que no se quede bloqueado para siempre:
/// - lo aceleramos con pragmas de sesión ([`IMPORT_PREAMBLE`]): desactivar
///   `foreign_key_checks`/`unique_checks` y agrupar en una sola transacción
///   (`autocommit=0` + `COMMIT` final) evita un fsync y la revalidación de índices
///   por statement, que es la causa real de que un dump de decenas de MB tarde
///   minutos;
/// - emitimos un contador en vivo (MB enviados, MB ya en la DB y segundos
///   transcurridos) por `op-log`, para que la UI no parezca congelada;
/// - un watchdog cancela el `docker exec` si NI el stdin avanza NI crece la DB
///   durante [`IMPORT_IDLE_TIMEOUT`]. Ojo: medir solo bytes-por-stdin daba falsos
///   positivos —el pipe del OS es de ~64&nbsp;KB, así que tras el primer chunk
///   `write_all` se bloquea hasta que mysql consume stdin, y mysql lo consume tan
///   rápido como APLICA el SQL; durante un statement grande no fluye ni un byte
///   aunque el import avance—. Por eso el indicador de vida es el tamaño real de
///   la DB (sondeo a `information_schema`), no el stdin.
/// Al cancelar, recreamos la DB vacía ([`reset_database`]) para no dejar un dump
/// aplicado a medias (corrupto). Reintentar reanuda: los pasos previos son
/// idempotentes y la DB queda limpia, así que el import vuelve a empezar de cero.
const IMPORT_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const IMPORT_CHUNK: usize = 1 << 20; // 1 MiB por escritura → progreso fino
const IMPORT_TICK: Duration = Duration::from_secs(2);
/// Pragmas antepuestos al dump para acelerar la aplicación (ver doc de arriba).
const IMPORT_PREAMBLE: &[u8] =
    b"SET autocommit=0;\nSET unique_checks=0;\nSET foreign_key_checks=0;\n";
const IMPORT_EPILOGUE: &[u8] = b"\nCOMMIT;\n";

async fn import_dump<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    site: &SiteConfig,
    db_container: &str,
    dump: &Path,
) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::process::Command;

    let bytes = Arc::new(
        std::fs::read(dump).with_context(|| format!("leyendo el dump {:?}", dump))?,
    );
    let total = bytes.len() as u64;
    let total_mb = total / 1_048_576;
    let dbname = site.services.db.db_name.clone();

    let mut child = Command::new("docker")
        .args(["exec", "-i", db_container, "mysql", "-uroot", "-ppanel", &dbname])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("lanzando `docker exec` para importar el dump")?;

    let mut stdin = child.stdin.take().expect("stdin piped");
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let start = Instant::now();
    // Marca de la última actividad de escritura, en ms desde `start`. La actualiza
    // el writer tras cada chunk; el watchdog la compara contra el reloj.
    let last_activity = Arc::new(AtomicU64::new(0));
    let written = Arc::new(AtomicU64::new(0));

    // Writer: vuelca pragmas + dump por chunks, registrando avance. Si mysql deja
    // de leer, `write_all` se bloquea; el watchdog se apoya en el crecimiento de la
    // DB (no solo en esto) para no cancelar un import sano pero lento.
    let writer = {
        let bytes = Arc::clone(&bytes);
        let last_activity = Arc::clone(&last_activity);
        let written = Arc::clone(&written);
        tokio::spawn(async move {
            if stdin.write_all(IMPORT_PREAMBLE).await.is_err() {
                return;
            }
            for chunk in bytes.chunks(IMPORT_CHUNK) {
                if stdin.write_all(chunk).await.is_err() {
                    return; // el exec murió (p. ej. lo mató el watchdog)
                }
                written.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                last_activity.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
            }
            stdin.write_all(IMPORT_EPILOGUE).await.ok(); // COMMIT de la transacción
            stdin.shutdown().await.ok(); // EOF → mysql termina
            // Tras EOF, reiniciar el reloj de inactividad: dale margen a mysql para
            // aplicar/commitear lo último sin que el watchdog lo mate.
            last_activity.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        })
    };

    // Drenar stdout/stderr para que el pipe no se llene y bloquee el proceso.
    let out_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.ok();
        buf
    });
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).await.ok();
        buf
    });

    // Watchdog + contador en vivo. Solo resuelve si se supera la inactividad; si
    // no, corre indefinidamente hasta que `child.wait()` gane el select. El
    // indicador de vida es el tamaño real de la DB (sondeo a `information_schema`):
    // crece mientras mysql aplica, aunque el stdin esté bloqueado.
    let db_size_sql = format!(
        "SELECT COALESCE(SUM(data_length+index_length),0) \
         FROM information_schema.tables WHERE table_schema='{dbname}'"
    );
    let watchdog = {
        let last_activity = Arc::clone(&last_activity);
        let written = Arc::clone(&written);
        async move {
            let mut ticker = tokio::time::interval(IMPORT_TICK);
            ticker.tick().await; // dispara de inmediato; descartar
            let mut max_db_bytes = 0u64;
            loop {
                ticker.tick().await;
                let now_ms = start.elapsed().as_millis() as u64;

                // Tamaño de la DB ahora; si creció, cuenta como actividad.
                let db_bytes = query_db_size(docker, db_container, &db_size_sql).await;
                if db_bytes > max_db_bytes {
                    max_db_bytes = db_bytes;
                    last_activity.store(now_ms, Ordering::Relaxed);
                }

                let sent = written.load(Ordering::Relaxed);
                let sent_mb = sent / 1_048_576;
                let secs = now_ms / 1000;
                // Línea "viva" que el frontend reescribe en sitio (no apila): barra
                // por bytes enviados + reloj. `max_db_bytes` solo alimenta el
                // watchdog, no se muestra (mantener la línea corta).
                log_progress(
                    app,
                    format!(
                        "      {sent_mb}/{total_mb} MB {} {}:{:02}",
                        progress_bar(sent, total, 24),
                        secs / 60,
                        secs % 60
                    ),
                );
                let idle_ms = now_ms.saturating_sub(last_activity.load(Ordering::Relaxed));
                if idle_ms >= IMPORT_IDLE_TIMEOUT.as_millis() as u64 {
                    return; // colgado: ni stdin avanza ni crece la DB
                }
            }
        }
    };

    let timed_out = tokio::select! {
        status = child.wait() => {
            let status = status.context("esperando a `docker exec` (import)")?;
            if !status.success() {
                writer.await.ok();
                let err = err_task.await.unwrap_or_default();
                out_task.abort();
                return Err(anyhow!(
                    "import del dump falló en {db_container}: {}",
                    String::from_utf8_lossy(&err).trim()
                ));
            }
            false
        }
        _ = watchdog => true,
    };

    if timed_out {
        let mins = IMPORT_IDLE_TIMEOUT.as_secs() / 60;
        log(
            app,
            format!("      ✗ Import sin avance por {mins} min: cancelando y restaurando la DB…"),
        );
        child.start_kill().ok();
        child.wait().await.ok();
        writer.abort();
        out_task.abort();
        err_task.abort();
        // Revertir: dejar la DB vacía para que un reintento importe desde cero.
        crate::wordpress::reset_database(docker, db_container, site)
            .await
            .context("restaurando la DB tras cancelar el import")?;
        log(app, "      ✓ DB restaurada (vacía).");
        return Err(anyhow!(
            "import cancelado: sin actividad por {mins} min. La DB se restauró \
             vacía; reintenta la migración para importar de nuevo."
        ));
    }

    writer.await.ok();
    out_task.abort();
    err_task.abort();
    Ok(())
}

/// Barra de progreso textual: `filled` de `width` caracteres según `done/total`.
/// Llena con `━`, resto `─` (p. ej. `━━━━━──────`).
fn progress_bar(done: u64, total: u64, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        (done.min(total) as u128 * width as u128 / total as u128) as usize
    };
    let mut s = String::with_capacity(width * 3);
    s.extend(std::iter::repeat('━').take(filled));
    s.extend(std::iter::repeat('─').take(width - filled));
    s
}

/// Tamaño actual de la DB (bytes) según `information_schema`. Indicador de vida
/// del import: si crece, mysql sigue aplicando. Best-effort: ante cualquier fallo
/// devuelve 0 (no se trata como progreso, pero tampoco rompe). El cliente `mysql`
/// avisa por stderr al pasar la contraseña en CLI y `exec` mezcla stdout+stderr,
/// así que buscamos la línea que sea un entero, no el primer renglón.
async fn query_db_size(docker: &DockerManager, db_container: &str, sql: &str) -> u64 {
    let out = match docker
        .exec(
            db_container,
            vec!["mysql", "-uroot", "-ppanel", "-N", "-B", "-e", sql],
        )
        .await
    {
        Ok(s) => s,
        Err(_) => return 0,
    };
    out.lines()
        .filter_map(|l| l.trim().parse::<u64>().ok())
        .next_back()
        .unwrap_or(0)
}
