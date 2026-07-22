//! Servidor D-Bus para el plasmoid KDE.
//!
//! Expone `com.goldmediatech.WordpressPanel.Manager` en la sesión del usuario.
//! El plasmoid consulta proyectos activos y puede detenerlos / cerrar el panel.
//! Para evitar tipos D-Bus complejos, `GetRunningSites` devuelve JSON.

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{Emitter as _, Manager as _};
use tauri_plugin_opener::OpenerExt;
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

/// Avisa al frontend de que la lista/estado de proyectos cambió por una
/// mutación vía CLI/MCP (D-Bus), para que la UI recargue sola.
fn notify_sites_changed(app: &tauri::AppHandle) {
    let _ = app.emit("sites-changed", ());
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

    /// TODOS los proyectos con su estado: JSON `[{id,name,domain,group,running}]`.
    /// Para el `wordpress-panel-cli list` (elegir cuál encender/apagar).
    async fn list_sites(&self) -> String {
        let docker = DockerManager::connect().ok();
        let sites = config::load_all_sites().unwrap_or_default();
        let mut out = Vec::new();
        for s in sites {
            let running = match &docker {
                Some(d) => d.is_running(&s.container_name()).await,
                None => false,
            };
            out.push(serde_json::json!({
                "id": s.id,
                "name": s.name,
                "domain": s.domain,
                "group": s.group,
                "running": running,
            }));
        }
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
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
        // Parar el watcher de auto-dump antes del stop (igual que el comando Tauri).
        self.app.state::<crate::autodump::AutoDump>().stop(&id);
        let ok = docker.stop_site(&site, &all).await.is_ok();
        notify_sites_changed(&self.app);
        ok
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
        notify_sites_changed(&self.app);
        ok
    }

    /// Enciende un proyecto y arranca su watcher de auto-dump. true si no hubo error.
    async fn start_site(&self, id: String) -> bool {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return false,
        };
        if docker.start_site(&site).await.is_err() {
            return false;
        }
        self.app.state::<crate::autodump::AutoDump>().start(site);
        notify_sites_changed(&self.app);
        true
    }

    /// Abre el admin del proyecto con auto-login. JSON `{ok:true}` o error.
    async fn open_admin(&self, id: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        match crate::autologin::open_admin(&self.app, &docker, &site, None).await {
            Ok(()) => serde_json::json!({ "ok": true }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Abre el frontend del proyecto en el navegador. JSON `{ok,url}` o error.
    async fn open_site(&self, id: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        if !docker.is_running(&site.container_name()).await {
            return err_json("el proyecto no está encendido");
        }
        let url = config::endpoint_or_default().site_url(&site.domain, site.services.nginx.ssl);
        match self.app.opener().open_url(url.clone(), None::<&str>) {
            Ok(()) => serde_json::json!({ "ok": true, "url": url }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Contenedores del proyecto y su estado. JSON `[{name,role,running}, ...]`.
    async fn project_containers(&self, id: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        let mut entries = vec![
            (site.container_name(), "php"),
            (crate::docker::db_container_name(&site.services.db), "db"),
            (crate::docker::NGINX.to_string(), "nginx"),
            (crate::docker::MAILPIT.to_string(), "mailpit"),
        ];
        if site.minio {
            entries.push((crate::docker::MINIO.to_string(), "minio"));
        }
        let mut arr = Vec::with_capacity(entries.len());
        for (name, role) in entries {
            let running = docker.is_running(&name).await;
            arr.push(serde_json::json!({ "name": name, "role": role, "running": running }));
        }
        serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
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
            Ok(s) => {
                notify_sites_changed(&self.app);
                serde_json::json!({ "ok": true, "id": s.id, "domain": s.domain }).to_string()
            }
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Elimina un worktree-project. `delete_branch`: además borrar la rama.
    async fn remove_worktree(&self, id: String, delete_branch: bool) -> bool {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let ok = crate::worktree::remove_worktree(&self.app, &docker, &id, delete_branch)
            .await
            .is_ok();
        notify_sites_changed(&self.app);
        ok
    }

    // -- Snapshots, clones y git (los usa `wordpress-panel-cli`) --------------

    /// Crea un punto de guardado. JSON `{ok,snapshot}` o `{ok:false,error}`.
    async fn create_snapshot(&self, id: String, label: String) -> String {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        match crate::snapshot::create_snapshot(&self.app, &docker, &site, &label).await {
            Ok(meta) => serde_json::json!({ "ok": true, "snapshot": meta }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Lista los puntos de guardado de un proyecto como array JSON de SnapshotMeta.
    async fn list_snapshots(&self, id: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return "[]".to_string();
        };
        let snaps = crate::snapshot::list_snapshots(&site).unwrap_or_default();
        serde_json::to_string(&snaps).unwrap_or_else(|_| "[]".into())
    }

    /// Elimina un punto de guardado. Devuelve true si no hubo error.
    async fn delete_snapshot(&self, id: String, snapshot_id: String) -> bool {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        crate::snapshot::delete_snapshot(&site, &snapshot_id).is_ok()
    }

    /// Crea un clone temporal desde un snapshot. JSON `{ok,id,domain}` o error.
    async fn create_clone(&self, parent_id: String, snapshot_id: String) -> String {
        let docker = match DockerManager::connect() {
            Ok(d) => d,
            Err(e) => return err_json(&e.to_string()),
        };
        match crate::clone::create_clone(&self.app, &docker, &parent_id, &snapshot_id).await {
            Ok(s) => {
                notify_sites_changed(&self.app);
                serde_json::json!({ "ok": true, "id": s.id, "domain": s.domain }).to_string()
            }
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Detecta los repos git del proyecto como array JSON de DetectedRepo.
    async fn gh_scan(&self, id: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return "[]".to_string();
        };
        let repos = crate::github::scan(&site).await;
        serde_json::to_string(&repos).unwrap_or_else(|_| "[]".into())
    }

    /// `git pull --ff-only` de un repo. JSON `{ok,output}` o `{ok:false,error}`.
    async fn gh_pull(&self, id: String, path: String, branch: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        match crate::github::pull(&site, &path, &branch).await {
            Ok(out) => serde_json::json!({ "ok": true, "output": out }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Estado de una rama frente a su remoto (BranchStatus serializado) o error.
    async fn gh_branch_status(&self, id: String, path: String, branch: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        match crate::github::branch_status(&site, &path, &branch).await {
            Ok(st) => serde_json::to_string(&st).unwrap_or_else(|_| err_json("no serializable")),
            Err(e) => err_json(&format!("{e:#}")),
        }
    }

    /// Carpetas candidatas para el build de un repo, como array JSON de String.
    async fn gh_build_dirs(&self, id: String, path: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return "[]".to_string();
        };
        let dirs = crate::github::build_dir_candidates(&site, &path);
        serde_json::to_string(&dirs).unwrap_or_else(|_| "[]".into())
    }

    /// Guarda rama/comando/carpetas de build de un repo registrado. true si Ok.
    async fn gh_set_deploy(
        &self,
        id: String,
        path: String,
        branch: String,
        build_cmd: String,
        build_dirs_csv: String,
    ) -> bool {
        let Ok(Some(mut site)) = config::find_site(&id) else {
            return false;
        };
        let Some(repo) = site.github.repos.iter_mut().find(|r| r.path == path) else {
            return false;
        };
        if !branch.trim().is_empty() {
            repo.branch = branch.trim().to_string();
        }
        repo.build_cmd = (!build_cmd.trim().is_empty()).then(|| build_cmd.trim().to_string());
        repo.build_dirs = build_dirs_csv
            .split(',')
            .map(|d| d.trim().trim_matches('/').to_string())
            .filter(|d| !d.is_empty())
            .collect();
        config::write_site_config(&site).is_ok()
    }

    /// Deploy directo de un repo registrado. JSON `{ok:true}` o `{ok:false,error}`.
    async fn gh_deploy(&self, id: String, path: String) -> String {
        let all = config::load_all_sites().unwrap_or_default();
        let Some(site) = all.iter().find(|s| s.id == id).cloned() else {
            return err_json("proyecto no encontrado");
        };
        let Some(repo) = site.github.repos.iter().find(|r| r.path == path) else {
            return err_json("repo no registrado");
        };
        match crate::github::deploy(
            &self.app,
            &site,
            &path,
            &repo.branch,
            repo.build_cmd.as_deref(),
            &repo.build_dirs,
        )
        .await
        {
            Ok(()) => serde_json::json!({ "ok": true }).to_string(),
            Err(e) => err_json(&format!("{e:#}")),
        }
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
