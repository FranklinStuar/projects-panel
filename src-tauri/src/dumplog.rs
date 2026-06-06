//! Registro (log) de volcados de DB: una línea por cada dump escrito en
//! `app/sql/`, para que el usuario pueda revisarlos y compararlos en el futuro.
//!
//! Formato JSONL (`config_dir/dump-log.jsonl`): una entrada `DumpLogEntry` por
//! línea, en orden de escritura. Lo alimentan el auto-dump (`autodump.rs`), el
//! export-al-detener (`stop_site`) y el export manual (`export_db`).
//!
//! Limpieza (`clean`): borra entradas por fecha (anteriores a) y/o por base de
//! datos. NOTA: solo poda el log, no borra los archivos `.sql` (de eso ya se
//! encarga `rotate_dumps`).

use std::io::Write;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::SiteConfig;

/// Una entrada del log de volcados. Espejo de `DumpLogEntry` en `types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpLogEntry {
    /// UTC, formato `YYYY-MM-DDTHH:MM:SSZ` (ordena lexicográficamente).
    pub timestamp: String,
    pub site_id: String,
    pub site_name: String,
    pub db_name: String,
    /// Ruta del archivo `.sql` generado.
    pub file: String,
    /// Tamaño del dump en bytes.
    pub bytes: u64,
    /// Origen del volcado: `auto` | `stop` | `manual`.
    pub source: String,
}

fn log_path() -> Result<std::path::PathBuf> {
    Ok(crate::config::config_dir()?.join("dump-log.jsonl"))
}

/// Añade una entrada al log para un dump recién escrito. Best-effort: si falla,
/// no debe romper el volcado (el `.sql` ya está en disco).
pub fn append(site: &SiteConfig, file: &str, source: &str) -> Result<()> {
    let bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let entry = DumpLogEntry {
        timestamp: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        site_id: site.id.clone(),
        site_name: site.name.clone(),
        db_name: site.services.db.db_name.clone(),
        file: file.to_string(),
        bytes,
        source: source.to_string(),
    };
    let line = serde_json::to_string(&entry)?;
    let path = log_path()?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Lee todas las entradas, más nuevas primero. Ignora líneas corruptas.
pub fn read_all() -> Result<Vec<DumpLogEntry>> {
    let path = log_path()?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut entries: Vec<DumpLogEntry> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    Ok(entries)
}

/// Borra entradas del log. Una entrada se elimina si cumple TODOS los filtros
/// dados: `before` (timestamp estrictamente anterior) y/o `db_name` (coincide).
/// Sin filtros = borra todo. Devuelve cuántas se eliminaron. No toca los `.sql`.
pub fn clean(before: Option<&str>, db_name: Option<&str>) -> Result<usize> {
    let all = read_all()?; // más nuevas primero; el orden no importa para reescribir
    let total = all.len();

    let kept: Vec<&DumpLogEntry> = all
        .iter()
        .filter(|e| {
            // ¿candidata a borrar? = cumple todos los filtros presentes.
            let by_date = before.map(|b| e.timestamp.as_str() < b).unwrap_or(true);
            let by_db = db_name.map(|d| e.db_name == d).unwrap_or(true);
            let remove = by_date && by_db;
            !remove
        })
        .collect();

    let removed = total - kept.len();
    if removed == 0 {
        return Ok(0);
    }

    // Reescribir en orden cronológico (más viejas primero), como se escribió.
    let path = log_path()?;
    let mut buf = String::new();
    for e in kept.iter().rev() {
        buf.push_str(&serde_json::to_string(e)?);
        buf.push('\n');
    }
    std::fs::write(&path, buf)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ts: &str, db: &str) -> DumpLogEntry {
        DumpLogEntry {
            timestamp: ts.into(),
            site_id: "s".into(),
            site_name: "S".into(),
            db_name: db.into(),
            file: "/tmp/x.sql".into(),
            bytes: 1,
            source: "auto".into(),
        }
    }

    fn keep_after_clean(
        all: &[DumpLogEntry],
        before: Option<&str>,
        db: Option<&str>,
    ) -> Vec<String> {
        all.iter()
            .filter(|e| {
                let by_date = before.map(|b| e.timestamp.as_str() < b).unwrap_or(true);
                let by_db = db.map(|d| e.db_name == d).unwrap_or(true);
                !(by_date && by_db)
            })
            .map(|e| format!("{}|{}", e.timestamp, e.db_name))
            .collect()
    }

    #[test]
    fn clean_por_fecha_borra_anteriores() {
        let all = vec![
            entry("2026-01-01T00:00:00Z", "a"),
            entry("2026-06-01T00:00:00Z", "a"),
        ];
        let kept = keep_after_clean(&all, Some("2026-03-01"), None);
        assert_eq!(kept, vec!["2026-06-01T00:00:00Z|a".to_string()]);
    }

    #[test]
    fn clean_por_db_borra_solo_esa() {
        let all = vec![
            entry("2026-01-01T00:00:00Z", "a"),
            entry("2026-01-01T00:00:00Z", "b"),
        ];
        let kept = keep_after_clean(&all, None, Some("a"));
        assert_eq!(kept, vec!["2026-01-01T00:00:00Z|b".to_string()]);
    }

    #[test]
    fn clean_combinado_es_interseccion() {
        let all = vec![
            entry("2026-01-01T00:00:00Z", "a"), // viejo + a → borra
            entry("2026-01-01T00:00:00Z", "b"), // viejo + b → conserva (db no)
            entry("2026-09-01T00:00:00Z", "a"), // nuevo + a → conserva (fecha no)
        ];
        let kept = keep_after_clean(&all, Some("2026-03-01"), Some("a"));
        assert_eq!(
            kept,
            vec![
                "2026-01-01T00:00:00Z|b".to_string(),
                "2026-09-01T00:00:00Z|a".to_string(),
            ]
        );
    }

    #[test]
    fn clean_sin_filtros_borra_todo() {
        let all = vec![entry("2026-01-01T00:00:00Z", "a")];
        let kept = keep_after_clean(&all, None, None);
        assert!(kept.is_empty());
    }
}
