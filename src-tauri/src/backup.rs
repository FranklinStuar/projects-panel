//! Backup de proyecto: export de la base de datos.
//!
//! Se ejecuta `mysqldump` DENTRO del container DB (socket local, sin TLS) y se
//! captura su stdout al host en `app/sql/`. No se usa `wp db export` desde el
//! container php porque su cliente mariadb falla la verificación del certificado
//! autofirmado de MySQL 8.

use anyhow::{anyhow, Result};
use chrono::Utc;
use std::path::Path;

use crate::config::SiteConfig;
use crate::docker::{db_container_name, DockerManager};

/// Captura el dump de la DB del proyecto en memoria. Necesita el engine DB
/// corriendo. Lo usan `export_db_to` (lo escribe a disco) y el auto-dump
/// (compara su hash para decidir si hubo cambios). Ver `autodump.rs`.
pub async fn dump_bytes(docker: &DockerManager, site: &SiteConfig) -> Result<Vec<u8>> {
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
                // Sin la línea `-- Dump completed on <fecha>`: cambia en cada
                // volcado y rompería el dedup por hash del auto-dump (volcaría y
                // loguearía aunque la DB no haya cambiado). Ver autodump.rs.
                "--skip-dump-date",
                dbname,
            ],
        )
        .await?;
    if dump.is_empty() {
        return Err(anyhow!("mysqldump no produjo salida para {dbname}"));
    }
    Ok(dump)
}

/// Exporta la DB del proyecto a `dest`. Ruta arbitraria; el directorio padre
/// debe existir o crearse antes de llamar. Necesita el engine DB corriendo.
pub async fn export_db_to(docker: &DockerManager, site: &SiteConfig, dest: &Path) -> Result<()> {
    let dump = dump_bytes(docker, site).await?;
    std::fs::write(dest, &dump)?;
    Ok(())
}

/// Exporta la DB del proyecto a `app/sql/db-{timestamp}.sql`.
/// Devuelve la ruta del archivo generado.
pub async fn export_db(docker: &DockerManager, site: &SiteConfig) -> Result<String> {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let sql_dir = site.sql_dir();
    std::fs::create_dir_all(&sql_dir).ok();
    let dest = sql_dir.join(format!("db-{stamp}.sql"));
    export_db_to(docker, site, &dest).await?;
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

#[cfg(test)]
mod tests {
    use super::rotate_dumps;
    use crate::config::{
        DbService, DbType, NginxService, PhpService, Services, SiteConfig,
    };
    use crate::config::GithubConfig;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    fn site_en(path: &std::path::Path) -> SiteConfig {
        SiteConfig {
            id: "test".into(),
            name: "Test".into(),
            path: path.to_string_lossy().into_owned(),
            domain: "test.test".into(),
            group: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            services: Services {
                php: PhpService { version: "8.3".into() },
                nginx: NginxService { ssl: true },
                db: DbService {
                    db_type: DbType::Mysql,
                    version: "8.0".into(),
                    db_name: "test".into(),
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
            snapshot_excludes: vec![],
        }
    }

    /// Crea `name` con un mtime = base + `offset_secs` (mtime mayor = más nuevo).
    fn dump(dir: &std::path::Path, name: &str, base: SystemTime, offset_secs: u64) {
        let f = File::create(dir.join(name)).unwrap();
        f.set_modified(base + Duration::from_secs(offset_secs)).unwrap();
    }

    #[test]
    fn rotate_conserva_los_n_mas_recientes_e_ignora_ruido() {
        let tmp = tempfile::tempdir().unwrap();
        let site = site_en(tmp.path());
        let sql = site.sql_dir();
        std::fs::create_dir_all(&sql).unwrap();

        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        // 5 dumps, db-5 el más nuevo.
        for i in 1..=5 {
            dump(&sql, &format!("db-{i}.sql"), base, i * 10);
        }
        // Ruido que NO debe tocarse.
        dump(&sql, "imported.sql", base, 1);
        dump(&sql, "local.sql", base, 1);

        rotate_dumps(&site, 3).unwrap();

        let mut quedan: Vec<String> = std::fs::read_dir(&sql)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        quedan.sort();

        // Quedan los 3 db-* más recientes (3,4,5) + el ruido intacto.
        assert_eq!(
            quedan,
            vec![
                "db-3.sql".to_string(),
                "db-4.sql".to_string(),
                "db-5.sql".to_string(),
                "imported.sql".to_string(),
                "local.sql".to_string(),
            ]
        );
    }

    #[test]
    fn rotate_no_borra_si_hay_menos_o_igual_que_keep() {
        let tmp = tempfile::tempdir().unwrap();
        let site = site_en(tmp.path());
        let sql = site.sql_dir();
        std::fs::create_dir_all(&sql).unwrap();

        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        dump(&sql, "db-1.sql", base, 10);
        dump(&sql, "db-2.sql", base, 20);

        rotate_dumps(&site, 3).unwrap();

        let n = std::fs::read_dir(&sql).unwrap().count();
        assert_eq!(n, 2);
    }
}
