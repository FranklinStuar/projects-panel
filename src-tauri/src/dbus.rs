//! Servidor D-Bus para el plasmoid KDE.
//!
//! Expone `com.goldmediatech.WordpressPanel.Manager` en la sesión del usuario.
//! El plasmoid consulta proyectos activos y puede detenerlos / cerrar el panel.
//! Para evitar tipos D-Bus complejos, `GetRunningSites` devuelve JSON.

use anyhow::{Context, Result};
use serde::Serialize;
use zbus::interface;

use crate::config;
use crate::docker::DockerManager;

const SERVICE: &str = "com.goldmediatech.WordpressPanel";
const PATH: &str = "/com/goldmediatech/WordpressPanel";

#[derive(Serialize)]
struct RunningSite {
    id: String,
    name: String,
    domain: String,
}

struct Manager {
    app: tauri::AppHandle,
}

#[interface(name = "com.goldmediatech.WordpressPanel.Manager")]
impl Manager {
    /// JSON con los proyectos en ejecución: `[{id,name,domain}, ...]`.
    async fn get_running_sites(&self) -> String {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return "[]".to_string(),
        };
        let sites = config::load_all_sites().unwrap_or_default();
        let mut running = Vec::new();
        for s in sites {
            if docker.is_running(&s.container_name()).await {
                running.push(RunningSite {
                    id: s.id,
                    name: s.name,
                    domain: s.domain,
                });
            }
        }
        serde_json::to_string(&running).unwrap_or_else(|_| "[]".to_string())
    }

    /// Detiene un proyecto. Devuelve true si no hubo error.
    async fn stop_site(&self, id: String) -> bool {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        docker.stop_site(&site, &all).await.is_ok()
    }

    /// Detiene todos los proyectos activos.
    async fn stop_all(&self) -> bool {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let all = config::load_all_sites().unwrap_or_default();
        let mut ok = true;
        for s in &all {
            if docker.stop_site(s, &all).await.is_err() {
                ok = false;
            }
        }
        ok
    }

    /// Cierra el panel.
    async fn quit(&self) {
        self.app.exit(0);
    }
}

/// Arranca el servidor D-Bus y devuelve la conexión (mantenerla viva).
pub async fn serve(app: tauri::AppHandle) -> Result<zbus::Connection> {
    let manager = Manager { app };
    let conn = zbus::connection::Builder::session()
        .context("conectando a la sesión D-Bus")?
        .name(SERVICE)
        .context("registrando el nombre del servicio D-Bus")?
        .serve_at(PATH, manager)
        .context("publicando la interfaz D-Bus")?
        .build()
        .await
        .context("construyendo la conexión D-Bus")?;
    Ok(conn)
}
