//! Puntos de guardado de un proyecto: tar del código (sin uploads/cache) + dump SQL.
//!
//! Almacén por proyecto: `~/panel-wp/{slug}/snapshots/{snapshot-id}/`
//!   - `code.tar.zst`  — código excluidos uploads, cache, wp-config.php y *.log
//!   - `db.sql`        — dump completo de la base de datos
//!   - `meta.json`     — metadatos del snapshot

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tokio::process::Command;
use uuid::Uuid;

use crate::config::{DbType, SiteConfig};
use crate::docker::DockerManager;
use crate::progress::log;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub db_name: String,
    pub db_type: DbType,
    /// Bytes del archivo code.tar.zst (0 si no se pudo medir).
    #[serde(default)]
    pub code_bytes: u64,
    /// Bytes del archivo db.sql (0 si no se pudo medir).
    #[serde(default)]
    pub db_bytes: u64,
}

fn snapshots_root(site: &SiteConfig) -> PathBuf {
    Path::new(&site.path).join("snapshots")
}

pub fn snapshot_dir(site: &SiteConfig, snapshot_id: &str) -> PathBuf {
    snapshots_root(site).join(snapshot_id)
}

/// Crea un punto de guardado del proyecto emitiendo progreso por `op-log`.
/// Arranca el engine DB si está parado (solo el engine, no el container php).
pub async fn create_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    site: &SiteConfig,
    label: &str,
) -> Result<SnapshotMeta> {
    match run(app, docker, site, label).await {
        Ok(meta) => Ok(meta),
        Err(err) => {
            log(app, format!("✗ Error creando el punto de guardado: {err:#}"));
            Err(err)
        }
    }
}

async fn run<R: Runtime>(
    app: &AppHandle<R>,
    docker: &DockerManager,
    site: &SiteConfig,
    label: &str,
) -> Result<SnapshotMeta> {
    log(app, format!("▶ Punto de guardado «{label}» — «{}»", site.name));

    // [1/3] Motor de base de datos.
    log(app, "[1/3] Arrancando motor de base de datos…");
    docker
        .ensure_db(&site.services.db)
        .await
        .context("arrancando motor de base de datos para snapshot")?;
    log(app, "      ✓ Motor listo.");

    let id = Uuid::new_v4().to_string();
    let dir = snapshot_dir(site, &id);
    std::fs::create_dir_all(&dir).context("creando directorio del snapshot")?;

    // [2/3] Dump de la base de datos.
    log(app, format!("[2/3] Exportando base de datos «{}»…", site.services.db.db_name));
    let db_path = dir.join("db.sql");
    crate::backup::export_db_to(docker, site, &db_path)
        .await
        .context("exportando DB para el snapshot")?;
    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    log(app, format!("      ✓ Base de datos exportada ({}).", fmt_bytes(db_bytes)));

    // [3/3] Tar del código, excluyendo uploads/cache/wp-config/logs.
    log(app, "[3/3] Comprimiendo código fuente (excluyendo uploads y caché)…");
    let code_path = dir.join("code.tar.zst");
    let public = site.public_dir();
    let status = Command::new("tar")
        .args([
            "--zstd",
            "-cf",
            code_path.to_str().unwrap(),
            "--exclude=./wp-content/uploads",
            "--exclude=./wp-content/cache",
            "--exclude=./wp-config.php",
            "--exclude=./*.log",
            "-C",
            public.to_str().context("ruta public inválida")?,
            ".",
        ])
        .status()
        .await
        .context("ejecutando tar para el snapshot de código")?;
    if !status.success() {
        std::fs::remove_dir_all(&dir).ok();
        return Err(anyhow!("tar falló al crear el snapshot de código"));
    }
    let code_bytes = std::fs::metadata(&code_path).map(|m| m.len()).unwrap_or(0);
    log(app, format!("      ✓ Código comprimido ({}).", fmt_bytes(code_bytes)));

    let total = code_bytes + db_bytes;
    log(app, format!("✓ Punto de guardado listo — total en disco: {}.", fmt_bytes(total)));

    let meta = SnapshotMeta {
        id: id.clone(),
        label: label.to_string(),
        created_at: Utc::now().to_rfc3339(),
        db_name: site.services.db.db_name.clone(),
        db_type: site.services.db.db_type,
        code_bytes,
        db_bytes,
    };
    std::fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)
        .context("escribiendo meta.json del snapshot")?;

    Ok(meta)
}

fn fmt_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", b as f64 / 1_073_741_824.0)
    } else if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else if b >= 1_024 {
        format!("{:.0} KB", b as f64 / 1_024.0)
    } else {
        format!("{b} B")
    }
}

/// Devuelve los snapshots del proyecto ordenados del más reciente al más antiguo.
pub fn list_snapshots(site: &SiteConfig) -> Result<Vec<SnapshotMeta>> {
    let root = snapshots_root(site);
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)?.flatten() {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let meta_path = entry.path().join("meta.json");
        if !meta_path.exists() {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&meta_path) {
            if let Ok(m) = serde_json::from_str::<SnapshotMeta>(&raw) {
                out.push(m);
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Borra el snapshot y sus archivos del disco.
pub fn delete_snapshot(site: &SiteConfig, snapshot_id: &str) -> Result<()> {
    let dir = snapshot_dir(site, snapshot_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("borrando snapshot {:?}", dir))?;
    }
    Ok(())
}
