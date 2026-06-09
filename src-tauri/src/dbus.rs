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

    // -- Worktrees (los usa el wrapper `wordpress-panel-cli worktree`) --------

    /// Worktrees de un proyecto padre, como JSON `[{id,name,domain,branch,targetPath,sharedDb}]`.
    async fn list_worktrees(&self, parent_id: String) -> String {
        let sites = crate::worktree::list_worktrees(&parent_id).unwrap_or_default();
        let out: Vec<_> = sites
            .into_iter()
            .map(|s| {
                let w = s.worktree_of.expect("list_worktrees solo devuelve worktrees");
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "domain": s.domain,
                    "branch": w.branch,
                    "targetPath": w.target_path,
                    "sharedDb": w.shared_db,
                })
            })
            .collect();
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
    }

    /// Crea un worktree-project. `base_branch` vacío = rama actual del repo.
    /// Devuelve JSON `{ok,id,domain}` o `{ok:false,error}`.
    async fn create_worktree(
        &self,
        parent_id: String,
        target_path: String,
        branch: String,
        base_branch: String,
        shared_db: bool,
    ) -> String {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        let base = (!base_branch.trim().is_empty()).then_some(base_branch.as_str());
        match crate::worktree::create_worktree(
            &self.app, &docker, &parent_id, &target_path, &branch, base, shared_db,
        )
        .await
        {
            Ok(s) => serde_json::json!({ "ok": true, "id": s.id, "domain": s.domain }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Elimina un worktree-project. `delete_branch`: además borrar la rama.
    async fn remove_worktree(&self, id: String, delete_branch: bool) -> bool {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return false,
        };
        crate::worktree::remove_worktree(&self.app, &docker, &id, delete_branch)
            .await
            .is_ok()
    }
}

fn err_json(msg: &str) -> String {
    serde_json::json!({ "ok": false, "error": msg }).to_string()
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
