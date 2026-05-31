//! Backup de proyecto: export de la base de datos.
//!
//! Se ejecuta `mysqldump` DENTRO del container DB (socket local, sin TLS) y se
//! captura su stdout al host en `app/sql/`. No se usa `wp db export` desde el
//! container php porque su cliente mariadb falla la verificación del certificado
//! autofirmado de MySQL 8.

use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::config::SiteConfig;
use crate::docker::{db_container_name, DockerManager};

/// Exporta la DB del proyecto a `app/sql/db-{timestamp}.sql`.
/// Devuelve la ruta del archivo generado.
pub async fn export_db(docker: &DockerManager, site: &SiteConfig) -> Result<String> {
    let db_container = db_container_name(&site.services.db);
    if !docker.is_running(&db_container).await {
        return Err(anyhow!(
            "la base de datos de '{}' no está encendida",
            site.name
        ));
    }

    let dbname = &site.services.db.db_name;
    let dump = docker
        .exec_capture(
            &db_container,
            vec![
                "mysqldump",
                "-uroot",
                "-ppanel",
                "--single-transaction",
                "--no-tablespaces",
                dbname,
            ],
        )
        .await?;
    if dump.is_empty() {
        return Err(anyhow!("mysqldump no produjo salida para {dbname}"));
    }

    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let sql_dir = site.sql_dir();
    std::fs::create_dir_all(&sql_dir).ok();
    let dest = sql_dir.join(format!("db-{stamp}.sql"));
    std::fs::write(&dest, &dump)?;

    Ok(dest.to_string_lossy().to_string())
}

/// Mantiene solo los `keep` dumps `db-*.sql` más recientes en `app/sql/` (rota
/// los del export-al-detener). No toca otros `.sql` (p. ej. `imported.sql`).
pub fn rotate_dumps(site: &SiteConfig, keep: usize) -> Result<()> {
    let dir = site.sql_dir();
    let mut dumps: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !(name.starts_with("db-") && name.ends_with(".sql")) {
            continue;
        }
        if let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) {
            dumps.push((mtime, path));
        }
    }
    if dumps.len() <= keep {
        return Ok(());
    }
    dumps.sort_by(|a, b| b.0.cmp(&a.0)); // más nuevo primero
    for (_, path) in dumps.into_iter().skip(keep) {
        std::fs::remove_file(&path).ok();
    }
    Ok(())
}
