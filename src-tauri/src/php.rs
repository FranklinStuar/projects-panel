//! Imagen php-fpm por versión y WP-CLI compartido.
//!
//! La imagen `panel-php:{ver}` se construye desde `docker/php/Dockerfile` con un
//! entrypoint que mapea uid/gid de www-data al del host (PUID/PGID) — sin esto
//! WordPress no puede escribir uploads/plugins en los bind-mounts.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use tokio::process::Command;

const WP_CLI_URL: &str =
    "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar";

/// Construye (si falta) y devuelve el tag de la imagen php para una versión.
pub async fn ensure_php_image(version: &str) -> Result<String> {
    let tag = format!("panel-php:{version}");

    // ¿ya existe localmente?
    let inspect = Command::new("docker")
        .args(["image", "inspect", &tag])
        .output()
        .await
        .context("ejecutando docker image inspect")?;
    if inspect.status.success() {
        return Ok(tag);
    }

    let context = crate::docker::docker_assets_dir().join("php");
    let status = Command::new("docker")
        .args([
            "build",
            "-t",
            &tag,
            "--build-arg",
            &format!("PHP_VERSION={version}"),
            context.to_str().context("ruta de contexto inválida")?,
        ])
        .status()
        .await
        .context("ejecutando docker build de la imagen php")?;

    if !status.success() {
        return Err(anyhow!("docker build falló para {tag}"));
    }
    Ok(tag)
}

/// Ruta al phar de WP-CLI en el host (se descarga una vez). Se monta en el
/// container como `/usr/local/bin/wp`.
pub async fn wp_cli_phar_path() -> Result<PathBuf> {
    let path = crate::config::config_dir()?.join("wp-cli.phar");
    if path.exists() {
        return Ok(path);
    }

    let bytes = reqwest::get(WP_CLI_URL)
        .await
        .context("descargando wp-cli.phar")?
        .error_for_status()?
        .bytes()
        .await?;
    std::fs::write(&path, &bytes).context("guardando wp-cli.phar")?;

    // ejecutable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms)?;
    }
    Ok(path)
}
