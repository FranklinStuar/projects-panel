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
use crate::wordpress;

pub async fn open_admin(
    app: &AppHandle,
    docker: &DockerManager,
    site: &SiteConfig,
    user_id: Option<u64>,
) -> Result<()> {
    if !docker.is_running(&site.container_name()).await {
        return Err(anyhow!("el proyecto '{}' no está encendido", site.name));
    }

    let base = crate::config::endpoint_or_default().site_url(&site.domain, site.services.nginx.ssl);

    let url = if site.one_click_admin {
        // Garantiza que el mu-plugin en disco sea siempre la versión actual.
        // Necesario para proyectos creados antes de que se añadiera soporte de user_id.
        wordpress::inject_autologin_muplugin(site).ok();

        let token = Uuid::new_v4().simple().to_string();
        let key = format!("panel_autologin_{token}");
        // valor = user_id para login como usuario específico; "0" = primer admin
        let value = user_id.unwrap_or(0).to_string();
        let args = vec![
            "transient".to_string(),
            "set".to_string(),
            key,
            value,
            "60".to_string(),
        ];
        crate::wpcli::run(docker, site, &args).await?;
        format!("{base}/?panel_autologin={token}")
    } else {
        format!("{base}/wp-admin/")
    };

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| anyhow!("no se pudo abrir el navegador: {err}"))?;
    Ok(())
}
