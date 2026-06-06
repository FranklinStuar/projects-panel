//! Auto-dump: protege contra pérdida de datos por apagón.
//!
//! El export-al-detener (`stop_site`) solo deja un dump fresco cuando el usuario
//! para el proyecto ordenadamente. Si la máquina se apaga de golpe con el sitio
//! activo, ese dump nunca se genera y se pierde el trabajo. Este módulo vigila la
//! DB de cada proyecto activo y, cuando detecta cambios, escribe un dump nuevo en
//! `app/sql/` — así siempre hay un volcado reciente que importar.
//!
//! "Trigger en cada cambio" a nivel SQL no sirve: un TRIGGER no puede correr
//! `mysqldump` en el host. En su lugar el watcher consulta un contador de
//! escrituras barato (gate) para no hacer nada mientras la DB está ociosa, y solo
//! cuando hubo escrituras vuelca la DB y compara su hash: si cambió respecto al
//! último volcado, persiste uno nuevo. Sin cambios, no escribe nada.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::config::{DbType, SiteConfig};
use crate::docker::{db_container_name, DockerManager};

/// Cada cuánto se sondea la DB de un proyecto activo.
const POLL: Duration = Duration::from_secs(20);

/// Registro de watchers de auto-dump activos, por id de proyecto. Estado Tauri
/// (`manage`), porque `DockerManager` se reconstruye en cada comando y no puede
/// guardar los handles.
#[derive(Default)]
pub struct AutoDump(Mutex<HashMap<String, JoinHandle<()>>>);

impl AutoDump {
    /// Arranca el watcher de un proyecto (idempotente: no duplica si ya corre).
    pub fn start(&self, site: SiteConfig) {
        let mut map = self.0.lock().unwrap();
        if map.contains_key(&site.id) {
            return;
        }
        let id = site.id.clone();
        let handle = tokio::spawn(watch(site));
        map.insert(id, handle);
    }

    /// Detiene el watcher de un proyecto (al pararlo). Best-effort.
    pub fn stop(&self, id: &str) {
        if let Some(h) = self.0.lock().unwrap().remove(id) {
            h.abort();
        }
    }
}

/// Bucle de vigilancia de un proyecto. Vive hasta que `AutoDump::stop` lo aborta.
async fn watch(site: SiteConfig) {
    let Ok(docker) = DockerManager::connect() else {
        return;
    };
    let db_container = db_container_name(&site.services.db);

    let mut last_writes: Option<u64> = None;
    // Línea base sembrada desde el último dump en disco (no desde el estado vivo):
    // si la DB cambió mientras el panel estaba cerrado o justo al arrancar, el
    // primer sondeo lo detecta y lo vuelca, en vez de tragárselo silenciosamente.
    let mut last_hash: Option<u64> = latest_dump_hash(&site);

    loop {
        tokio::time::sleep(POLL).await;

        if !docker.is_running(&db_container).await {
            continue;
        }

        // Gate barato: si no hubo escrituras desde el último sondeo, no volcar.
        if let Some(writes) = write_counter(&docker, &db_container, site.services.db.db_type).await
        {
            if last_writes == Some(writes) {
                continue;
            }
            last_writes = Some(writes);
        }

        // Hubo (posibles) escrituras: volcar a memoria y comparar el hash.
        let Ok(dump) = crate::backup::dump_bytes(&docker, &site).await else {
            continue;
        };
        let hash = hash_bytes(&dump);

        // Igual al último dump persistido → nada cambió, no escribir.
        if last_hash == Some(hash) {
            continue;
        }
        last_hash = Some(hash);
        if let Err(err) = persist(&site, &dump) {
            eprintln!("auto-dump de '{}' falló: {err}", site.name);
        }
    }
}

/// Hash del dump `db-*.sql` más reciente en `app/sql/` (la última foto que
/// persistimos), para sembrar la línea base. `None` si no hay ninguno.
fn latest_dump_hash(site: &SiteConfig) -> Option<u64> {
    let dir = site.sql_dir();
    let newest = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("db-") && n.ends_with(".sql")
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            Some((m, e.path()))
        })
        .max_by_key(|(m, _)| *m)?
        .1;
    let bytes = std::fs::read(&newest).ok()?;
    Some(hash_bytes(&bytes))
}

/// Escribe un dump nuevo en `app/sql/db-{timestamp}.sql`, lo registra en el log
/// y rota los viejos.
fn persist(site: &SiteConfig, dump: &[u8]) -> anyhow::Result<()> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let sql_dir = site.sql_dir();
    std::fs::create_dir_all(&sql_dir).ok();
    let dest = sql_dir.join(format!("db-{stamp}.sql"));
    std::fs::write(&dest, dump)?;
    crate::dumplog::append(site, &dest.to_string_lossy(), "auto").ok();
    crate::backup::rotate_dumps(site, 3).ok();
    Ok(())
}

/// Contador de filas escritas del engine (server-wide), como gate barato para
/// saltarse el `mysqldump` cuando la DB está ociosa. `None` = sin gate fiable
/// (se vuelca igualmente y decide el hash).
async fn write_counter(docker: &DockerManager, container: &str, db_type: DbType) -> Option<u64> {
    let out = match db_type {
        DbType::Mysql | DbType::Mariadb => docker
            .exec(
                container,
                vec![
                    "mysql",
                    "-uroot",
                    "-ppanel",
                    "-N",
                    "-B",
                    "-e",
                    "SHOW GLOBAL STATUS WHERE Variable_name IN \
                     ('Innodb_rows_inserted','Innodb_rows_updated','Innodb_rows_deleted')",
                ],
            )
            .await
            .ok()?,
        // Postgres apenas se usa con WP; sin gate, se confía en el hash.
        DbType::Postgres => return None,
    };

    // Líneas "Innodb_rows_inserted\t123"; sumar la segunda columna. Ignora ruido
    // (p. ej. el aviso de mysql sobre la contraseña en la línea de comandos).
    let mut sum: u64 = 0;
    let mut any = false;
    for line in out.lines() {
        let mut cols = line.split_whitespace();
        match (cols.next(), cols.next()) {
            (Some(name), Some(val)) if name.starts_with("Innodb_rows_") => {
                if let Ok(n) = val.parse::<u64>() {
                    sum = sum.saturating_add(n);
                    any = true;
                }
            }
            _ => {}
        }
    }
    any.then_some(sum)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
