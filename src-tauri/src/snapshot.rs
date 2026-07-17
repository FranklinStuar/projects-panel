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
    /// Rutas extra (relativas a public) excluidas del tar en este snapshot,
    /// además de las fijas (uploads, cache, wp-config, *.log).
    #[serde(default)]
    pub excludes: Vec<String>,
}

/// Una carpeta candidata a excluir del punto de guardado, detectada en disco.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludableEntry {
    /// Ruta relativa a `public_dir` (la que se persiste en `snapshot_excludes`).
    pub path: String,
    /// Tamaño en disco de la carpeta (bytes).
    pub bytes: u64,
    /// true si es una carpeta de backup conocida (recomendado excluir).
    pub known: bool,
    /// Plugin/origen asociado si `known`, p. ej. "UpdraftPlus".
    pub label: Option<String>,
}

/// Carpetas de backup conocidas (ruta relativa a public → plugin de origen).
/// Pesan mucho y casi nunca interesa guardarlas en un punto de guardado.
const KNOWN_BACKUP_DIRS: &[(&str, &str)] = &[
    ("wp-content/updraft", "UpdraftPlus"),
    ("wp-content/ai1wm-backups", "All-in-One WP Migration"),
    ("wp-content/wpvividbackups", "WPvivid"),
    ("wp-content/backups-dup-lite", "Duplicator"),
    ("wp-content/backups-dup-pro", "Duplicator Pro"),
    ("wp-content/backuply", "Backuply"),
    ("wp-snapshots", "Duplicator"),
];

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

    // [3/3] Tar del código, excluyendo uploads/cache/wp-config/logs + extras del proyecto.
    let extras = &site.snapshot_excludes;
    if extras.is_empty() {
        log(app, "[3/3] Comprimiendo código fuente (excluyendo uploads y caché)…");
    } else {
        log(app, format!(
            "[3/3] Comprimiendo código fuente (excluyendo uploads, caché y {} ruta(s) del proyecto)…",
            extras.len()
        ));
    }
    let code_path = dir.join("code.tar.zst");
    let public = site.public_dir();
    let mut tar_args: Vec<String> = vec![
        "--zstd".into(),
        "-cf".into(),
        code_path.to_str().unwrap().to_string(),
        "--exclude=./wp-content/uploads".into(),
        "--exclude=./wp-content/cache".into(),
        "--exclude=./wp-config.php".into(),
        "--exclude=./*.log".into(),
    ];
    for rel in extras {
        // Normaliza a ruta relativa a public; tar las quiere como `./ruta`.
        let clean = rel.trim().trim_start_matches("./").trim_matches('/');
        if !clean.is_empty() {
            tar_args.push(format!("--exclude=./{clean}"));
        }
    }
    tar_args.push("-C".into());
    tar_args.push(public.to_str().context("ruta public inválida")?.to_string());
    tar_args.push(".".into());
    let out = Command::new("tar")
        .args(&tar_args)
        .output()
        .await
        .context("ejecutando tar para el snapshot de código")?;
    // tar: 0 = ok, 1 = avisos no fatales (típico «file changed as we read it» en
    // un WP activo: cache/logs mutan durante el tar; el archivo queda válido),
    // 2+ = error real. Solo abortamos en 2+; el aviso se registra y se sigue.
    let code = out.status.code().unwrap_or(-1);
    if code != 0 && code != 1 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        std::fs::remove_dir_all(&dir).ok();
        return Err(anyhow!(
            "tar falló (código {code}) al crear el snapshot de código:\n{}",
            stderr.trim()
        ));
    }
    if code == 1 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        log(app, format!(
            "      ⚠ tar avisó de archivos que cambiaron durante la copia (no fatal): {}",
            stderr.trim().lines().next().unwrap_or("")
        ));
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
        excludes: extras.clone(),
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

/// Detecta carpetas candidatas a excluir del punto de guardado.
///
/// Devuelve: (a) cada subcarpeta inmediata de `wp-content` —salvo las ya
/// excluidas siempre (`uploads`, `cache`)— para que el usuario pueda excluir
/// carpetas propias del proyecto, y (b) carpetas de backup conocidas que existan
/// (marcadas `known` y con la etiqueta del plugin). Ordenado por tamaño desc.
pub fn detect_excludable(site: &SiteConfig) -> Result<Vec<ExcludableEntry>> {
    let public = site.public_dir();
    let mut out: Vec<ExcludableEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // (a) Subcarpetas inmediatas de wp-content (excepto las forzadas).
    let wp_content = public.join("wp-content");
    if let Ok(rd) = std::fs::read_dir(&wp_content) {
        for entry in rd.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "uploads" || name == "cache" {
                continue; // ya excluidas siempre
            }
            let rel = format!("wp-content/{name}");
            let (known, label) = known_backup(&rel);
            out.push(ExcludableEntry {
                bytes: dir_size(&entry.path()),
                path: rel.clone(),
                known,
                label,
            });
            seen.insert(rel);
        }
    }

    // (b) Carpetas de backup conocidas fuera de wp-content (p. ej. wp-snapshots).
    for (rel, label) in KNOWN_BACKUP_DIRS {
        if seen.contains(*rel) {
            continue;
        }
        let abs = public.join(rel);
        if abs.is_dir() {
            out.push(ExcludableEntry {
                path: rel.to_string(),
                bytes: dir_size(&abs),
                known: true,
                label: Some(label.to_string()),
            });
        }
    }

    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Ok(out)
}

/// Si `rel` coincide con una carpeta de backup conocida, devuelve `(true, label)`.
fn known_backup(rel: &str) -> (bool, Option<String>) {
    for (k, label) in KNOWN_BACKUP_DIRS {
        if *k == rel {
            return (true, Some(label.to_string()));
        }
    }
    (false, None)
}

/// Tamaño recursivo de un directorio en bytes (best-effort; ignora errores).
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    total
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
