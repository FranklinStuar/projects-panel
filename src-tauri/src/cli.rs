//! Instalación del wrapper WP-CLI para la terminal.
//!
//! Copia dos scripts a `~/.local/bin`:
//!   - `wp`                    → ejecuta WP-CLI en el container del proyecto del CWD.
//!   - `wordpress-panel-cli`   → resuelve a qué proyecto pertenece una ruta.
//! Así el usuario corre `wp ...` dentro de cualquier carpeta de proyecto.

use anyhow::{anyhow, Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn scripts_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts"))
}

fn local_bin() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("no se pudo determinar el home del usuario")?
        .join(".local")
        .join("bin");
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

fn install_one(src: &str, dest_name: &str) -> Result<PathBuf> {
    let src = scripts_dir().join(src);
    if !src.exists() {
        return Err(anyhow!("no se encontró el script {:?}", src));
    }
    let dest = local_bin()?.join(dest_name);
    std::fs::copy(&src, &dest).with_context(|| format!("copiando a {:?}", dest))?;
    let mut perms = std::fs::metadata(&dest)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&dest, perms)?;
    Ok(dest)
}

/// Instala los wrappers y devuelve un mensaje con el resultado.
pub fn install_cli_wrapper() -> Result<String> {
    let cli = install_one("wordpress-panel-cli.sh", "wordpress-panel-cli")?;
    let wp = install_one("wp-wrapper.sh", "wp")?;

    let bin = local_bin()?;
    let on_path = std::env::var("PATH")
        .map(|p| p.split(':').any(|d| PathBuf::from(d) == bin))
        .unwrap_or(false);

    let mut msg = format!(
        "Instalado:\n  {}\n  {}\nUsa `wp <args>` dentro de la carpeta de un proyecto.",
        wp.display(),
        cli.display()
    );
    if !on_path {
        msg.push_str(&format!(
            "\n\n⚠ {} no está en tu PATH. Añádelo a tu shell:\n  export PATH=\"$HOME/.local/bin:$PATH\"",
            bin.display()
        ));
    }
    Ok(msg)
}
