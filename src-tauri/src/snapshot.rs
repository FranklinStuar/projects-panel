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
use tokio::process::Command;
use uuid::Uuid;

use crate::config::{DbType, SiteConfig};
use crate::docker::DockerManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub db_name: String,
    pub db_type: DbType,
}

fn snapshots_root(site: &SiteConfig) -> PathBuf {
    Path::new(&site.path).join("snapshots")
}

pub fn snapshot_dir(site: &SiteConfig, snapshot_id: &str) -> PathBuf {
    snapshots_root(site).join(snapshot_id)
}

/// Crea un punto de guardado del proyecto. El engine DB debe estar corriendo
/// (llama a `docker.ensure_db` antes si el sitio está parado).
pub async fn create_snapshot(
    docker: &DockerManager,
    site: &SiteConfig,
    label: &str,
) -> Result<SnapshotMeta> {
    // Arrancar el engine DB si está parado (solo necesitamos el engine, no el php).
    let _db_container = docker
        .ensure_db(&site.services.db)
        .await
        .context("arrancando motor de base de datos para snapshot")?;

    let id = Uuid::new_v4().to_string();
    let dir = snapshot_dir(site, &id);
    std::fs::create_dir_all(&dir).context("creando directorio del snapshot")?;

    // 1. Dump de la base de datos.
    let db_path = dir.join("db.sql");
    crate::backup::export_db_to(docker, site, &db_path)
        .await
        .context("exportando DB para el snapshot")?;

    // 2. Tar del código, excluyendo uploads/cache/wp-config/logs.
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

    let meta = SnapshotMeta {
        id: id.clone(),
        label: label.to_string(),
        created_at: Utc::now().to_rfc3339(),
        db_name: site.services.db.db_name.clone(),
        db_type: site.services.db.db_type,
    };
    std::fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)
        .context("escribiendo meta.json del snapshot")?;

    Ok(meta)
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
