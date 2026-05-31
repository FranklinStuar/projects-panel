//! Certificados SSL locales con mkcert.
//!
//! `mkcert -install` (CA local) se hace una vez en la primera configuración
//! (`scripts/first-run.sh`). Aquí solo se generan los cert/key por dominio en la
//! carpeta `ssl/` del proyecto, que `panel-nginx` lee vía `/srv/projects/{dir}/ssl`.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::config::SiteConfig;

fn ssl_dir(site: &SiteConfig) -> PathBuf {
    Path::new(&site.path).join("ssl")
}

pub fn has_cert(site: &SiteConfig) -> bool {
    let dir = ssl_dir(site);
    dir.join("cert.pem").exists() && dir.join("key.pem").exists()
}

/// Genera `cert.pem`/`key.pem` para el dominio del proyecto con mkcert.
pub async fn generate(site: &SiteConfig) -> Result<()> {
    let dir = ssl_dir(site);
    std::fs::create_dir_all(&dir)?;
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");

    let status = Command::new("mkcert")
        .args([
            "-cert-file",
            cert.to_str().context("ruta cert inválida")?,
            "-key-file",
            key.to_str().context("ruta key inválida")?,
            &site.domain,
        ])
        .status()
        .await
        .context("ejecutando mkcert (¿instalado? ver first-run.sh)")?;

    if !status.success() {
        return Err(anyhow!("mkcert falló para el dominio {}", site.domain));
    }
    Ok(())
}
