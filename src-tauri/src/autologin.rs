//! Auto-login al admin de WordPress (one-click).
//!
//! No usa ningún "magic link" de WP-CLI (no existe en core). En su lugar, el
//! proyecto lleva inyectado un mu-plugin (`panel-autologin.php`) que valida un
//! token efímero de un solo uso. El flujo:
//!   1. generamos un token aleatorio,
//!   2. lo guardamos como transient de WP (60s) vía WP-CLI,
//!   3. abrimos el navegador en `?panel_autologin={token}`,
//!   4. el mu-plugin valida, borra el transient (un solo uso) y loguea al admin.

use anyhow::{anyhow, Result};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::config::SiteConfig;
use crate::docker::DockerManager;

pub async fn open_admin(app: &AppHandle, docker: &DockerManager, site: &SiteConfig) -> Result<()> {
    if !docker.is_running(&site.container_name()).await {
        return Err(anyhow!("el proyecto '{}' no está encendido", site.name));
    }

    let scheme = if site.services.nginx.ssl { "https" } else { "http" };

    let url = if site.one_click_admin {
        let token = Uuid::new_v4().simple().to_string();
        let key = format!("panel_autologin_{token}");
        // transient de un solo uso, expira en 60s
        let args = vec![
            "transient".to_string(),
            "set".to_string(),
            key,
            "1".to_string(),
            "60".to_string(),
        ];
        crate::wpcli::run(docker, site, &args).await?;
        format!("{scheme}://{}/?panel_autologin={token}", site.domain)
    } else {
        format!("{scheme}://{}/wp-admin/", site.domain)
    };

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| anyhow!("no se pudo abrir el navegador: {err}"))?;
    Ok(())
}
