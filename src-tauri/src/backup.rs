//! Backup de proyecto: export de la base de datos vía WP-CLI.
//!
//! `wp db export` escribe el dump dentro del container, en la raíz pública
//! (`/var/www/html`, bind-montada). Luego se mueve en el host a `app/sql/`.

use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::config::SiteConfig;
use crate::docker::DockerManager;

/// Exporta la DB del proyecto a `app/sql/db-{timestamp}.sql`.
/// Devuelve la ruta del archivo generado.
pub async fn export_db(docker: &DockerManager, site: &SiteConfig) -> Result<String> {
    let cname = site.container_name();
    if !docker.is_running(&cname).await {
        return Err(anyhow!("el proyecto '{}' no está encendido", site.name));
    }

    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let file = format!("db-{stamp}.sql");

    // Exportar dentro del container a la raíz pública (montada en el host).
    let out = docker
        .exec(
            &cname,
            vec![
                "php",
                "/usr/local/bin/wp",
                "--path=/var/www/html",
                "db",
                "export",
                &format!("/var/www/html/{file}"),
            ],
        )
        .await?;

    let in_public = site.public_dir().join(&file);
    if !in_public.exists() {
        return Err(anyhow!("WP-CLI no generó el dump: {out}"));
    }

    // Mover del público a app/sql/ (fuera de la raíz servida por nginx).
    let sql_dir = site.sql_dir();
    std::fs::create_dir_all(&sql_dir).ok();
    let dest = sql_dir.join(&file);
    std::fs::rename(&in_public, &dest)
        .or_else(|_| std::fs::copy(&in_public, &dest).map(|_| std::fs::remove_file(&in_public).ok()).map(|_| ()))?;

    Ok(dest.to_string_lossy().to_string())
}
