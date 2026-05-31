//! Estado del sistema para la pantalla de configuración (Fase 4).
//!
//! Reúne en una sola lectura el estado de los prerequisitos del panel (Docker,
//! red `panel-net`, dnsmasq wildcard, CA de mkcert, wrappers WP-CLI, plasmoid)
//! junto con el endpoint y las rutas. La UI lo pinta como checklist y ofrece
//! botones para las acciones que NO requieren privilegios; las que sí (dnsmasq,
//! mkcert CA, plasmoid) se delegan a `scripts/first-run.sh`.

use serde::Serialize;

use crate::config::{self, Endpoint};
use crate::docker::DockerManager;

/// Id del plasmoid (debe coincidir con `KPlugin.Id` de su `metadata.json`).
const PLASMOID_ID: &str = "com.goldmediatech.wordpresspanel";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub docker_ok: bool,
    pub network_ok: bool,
    pub dnsmasq_ok: bool,
    pub mkcert_ok: bool,
    pub cli_wrapper_ok: bool,
    pub plasmoid_ok: bool,
    pub endpoint: Endpoint,
    pub projects_root: String,
    pub config_dir: String,
}

/// Estado actual del sistema (todas las comprobaciones son best-effort: un
/// chequeo que falla se reporta como `false`, nunca aborta).
pub async fn status() -> SystemStatus {
    let docker = DockerManager::connect();
    let docker_ok = docker.is_ok();
    let network_ok = match &docker {
        Ok(d) => d.network_exists().await,
        Err(_) => false,
    };

    SystemStatus {
        docker_ok,
        network_ok,
        dnsmasq_ok: crate::domain::wildcard_active(),
        mkcert_ok: mkcert_ca_installed(),
        cli_wrapper_ok: wrapper_installed(),
        plasmoid_ok: plasmoid_installed(),
        endpoint: config::endpoint_or_default(),
        projects_root: config::projects_root()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        config_dir: config::config_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// ¿La CA local de mkcert está instalada? (`rootCA.pem` en el CAROOT).
fn mkcert_ca_installed() -> bool {
    let out = std::process::Command::new("mkcert").arg("-CAROOT").output();
    match out {
        Ok(o) if o.status.success() => {
            let root = String::from_utf8_lossy(&o.stdout).trim().to_string();
            !root.is_empty() && std::path::Path::new(&root).join("rootCA.pem").exists()
        }
        _ => false,
    }
}

/// ¿Está instalado el wrapper `wp` en `~/.local/bin`?
fn wrapper_installed() -> bool {
    dirs::home_dir()
        .map(|h| h.join(".local").join("bin").join("wp").exists())
        .unwrap_or(false)
}

/// ¿Está instalado el plasmoid en `~/.local/share/plasma/plasmoids/{id}`?
fn plasmoid_installed() -> bool {
    dirs::data_dir()
        .map(|d| {
            d.join("plasma")
                .join("plasmoids")
                .join(PLASMOID_ID)
                .exists()
        })
        .unwrap_or(false)
}
