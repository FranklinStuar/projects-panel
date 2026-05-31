//! Panel WP — backend Tauri.

mod autologin;
mod config;
mod dbus;
mod docker;
mod domain;
mod github;
mod logs;
mod nginx;
mod php;
mod ssl;
mod wordpress;
mod wpcli;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, State};
use tokio::task::JoinHandle;

use config::{SiteConfig, SiteState};
use docker::DockerManager;
use wordpress::{NewSiteRequest, WpVersion};

type CmdResult<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

/// Tareas de streaming de logs activas, por id de proyecto.
#[derive(Default)]
struct LogStreams(Mutex<HashMap<String, JoinHandle<()>>>);

/// Lista todos los proyectos con su estado real (running / stopped / pending).
#[tauri::command]
async fn get_sites() -> CmdResult<Vec<SiteState>> {
    let sites = config::load_all_sites().map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    let mut out = Vec::with_capacity(sites.len());
    for cfg in sites {
        let status = docker.site_status(&cfg).await;
        out.push(SiteState { config: cfg, status });
    }
    Ok(out)
}

#[tauri::command]
async fn start_site(id: String) -> CmdResult<()> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    docker.start_site(&site).await.map_err(e)
}

#[tauri::command]
async fn stop_site(id: String) -> CmdResult<()> {
    let all = config::load_all_sites().map_err(e)?;
    let site = all
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    docker.stop_site(&site, &all).await.map_err(e)
}

#[tauri::command]
async fn stop_all_sites() -> CmdResult<()> {
    let all = config::load_all_sites().map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    for site in &all {
        docker.stop_site(site, &all).await.ok();
    }
    Ok(())
}

#[tauri::command]
async fn exec_wpcli(id: String, args: Vec<String>) -> CmdResult<String> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    wpcli::run(&docker, &site, &args).await.map_err(e)
}

#[tauri::command]
async fn create_site(req: NewSiteRequest) -> CmdResult<SiteConfig> {
    let docker = DockerManager::connect().map_err(e)?;
    wordpress::create_site(&docker, req).await.map_err(e)
}

#[tauri::command]
async fn list_wp_versions() -> CmdResult<Vec<WpVersion>> {
    wordpress::fetch_versions().await.map_err(e)
}

/// Abre el admin en el navegador (auto-login si el proyecto lo tiene activado).
#[tauri::command]
async fn open_admin(app: AppHandle, id: String) -> CmdResult<()> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    autologin::open_admin(&app, &docker, &site).await.map_err(e)
}

/// Empieza a emitir eventos `log:{id}` con los logs del container del proyecto.
#[tauri::command]
async fn stream_logs(app: AppHandle, state: State<'_, LogStreams>, id: String) -> CmdResult<()> {
    {
        let map = state.0.lock().unwrap();
        if map.contains_key(&id) {
            return Ok(()); // ya hay un stream activo
        }
    }
    let handle = logs::spawn_stream(app, id.clone()).map_err(e)?;
    state.0.lock().unwrap().insert(id, handle);
    Ok(())
}

#[tauri::command]
async fn stop_logs(state: State<'_, LogStreams>, id: String) -> CmdResult<()> {
    let handle = state.0.lock().unwrap().remove(&id);
    if let Some(h) = handle {
        h.abort();
    }
    Ok(())
}

#[tauri::command]
async fn list_plugins(id: String) -> CmdResult<String> {
    wpcli_json(&id, &["plugin", "list", "--format=json"]).await
}

#[tauri::command]
async fn list_themes(id: String) -> CmdResult<String> {
    wpcli_json(&id, &["theme", "list", "--format=json"]).await
}

/// Helper: ejecuta WP-CLI y devuelve la salida (esperada en JSON).
async fn wpcli_json(id: &str, args: &[&str]) -> CmdResult<String> {
    let site = config::find_site(id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    wpcli::run(&docker, &site, &owned).await.map_err(e)
}

// -- GitHub ------------------------------------------------------------------

#[tauri::command]
async fn gh_status() -> CmdResult<github::GhStatus> {
    Ok(github::status().await)
}

/// Regenera el certificado SSL del proyecto (mkcert) y recarga nginx.
#[tauri::command]
async fn regenerate_ssl(id: String) -> CmdResult<()> {
    let site = load_site(&id)?;
    ssl::generate(&site).await.map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    docker.reload_nginx().await.map_err(e)
}

/// Asigna (o quita, con cadena vacía) el grupo de un proyecto.
#[tauri::command]
async fn set_site_group(id: String, group: Option<String>) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    site.group = group.filter(|g| !g.trim().is_empty());
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

fn load_site(id: &str) -> CmdResult<SiteConfig> {
    config::find_site(id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))
}

/// Clona un repo (kind = "theme" | "plugin") y lo registra en config.json.
#[tauri::command]
async fn gh_clone(
    id: String,
    kind: String,
    repo: String,
    branch: String,
) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    let rel_path = github::propose_path(&kind, &repo);
    github::clone(&site, &repo, &branch, &rel_path)
        .await
        .map_err(e)?;

    let entry = config::GithubRepo {
        repo,
        branch,
        path: rel_path,
    };
    if kind == "theme" {
        site.github.theme = Some(entry);
    } else {
        site.github.plugins.push(entry);
    }
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

#[tauri::command]
async fn gh_pull(id: String, path: String, branch: String) -> CmdResult<String> {
    let site = load_site(&id)?;
    github::pull(&site, &path, &branch).await.map_err(e)
}

#[tauri::command]
async fn gh_pull_all(id: String) -> CmdResult<String> {
    let site = load_site(&id)?;
    let mut out = String::new();
    if let Some(t) = &site.github.theme {
        out.push_str(&format!("== theme {} ==\n", t.repo));
        out.push_str(&github::pull(&site, &t.path, &t.branch).await.unwrap_or_else(|err| e(err)));
    }
    for p in &site.github.plugins {
        out.push_str(&format!("\n== plugin {} ==\n", p.repo));
        out.push_str(&github::pull(&site, &p.path, &p.branch).await.unwrap_or_else(|err| e(err)));
    }
    Ok(out)
}

/// Quita un repo: borra la carpeta y lo desregistra de config.json.
#[tauri::command]
async fn gh_remove(id: String, kind: String, path: String) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    github::remove_dir(&site, &path).map_err(e)?;
    if kind == "theme" {
        site.github.theme = None;
    } else {
        site.github.plugins.retain(|p| p.path != path);
    }
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // NOTA: la posición de los botones de la barra de título (deben respetar la
    // config del usuario en KDE) queda pendiente — ver docs/KNOWN_ISSUES.md.
    // Se revisará al finalizar todas las fases.
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(LogStreams::default())
        .setup(|app| {
            // Servidor D-Bus para el plasmoid KDE. Si la sesión D-Bus no está
            // disponible, el panel sigue funcionando igual (solo sin widget).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match dbus::serve(handle).await {
                    Ok(conn) => {
                        let _keep = conn; // mantener viva la conexión
                        std::future::pending::<()>().await;
                    }
                    Err(err) => eprintln!("D-Bus no disponible: {err}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sites,
            start_site,
            stop_site,
            stop_all_sites,
            exec_wpcli,
            create_site,
            list_wp_versions,
            open_admin,
            stream_logs,
            stop_logs,
            list_plugins,
            list_themes,
            gh_status,
            gh_clone,
            gh_pull,
            gh_pull_all,
            gh_remove,
            regenerate_ssl,
            set_site_group
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar la aplicación Tauri");
}
