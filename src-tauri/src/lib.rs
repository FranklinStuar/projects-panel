//! Panel WP — backend Tauri.

mod config;
mod docker;
mod domain;
mod nginx;
mod php;
mod wordpress;
mod wpcli;

use config::{SiteConfig, SiteState};
use docker::DockerManager;
use wordpress::{NewSiteRequest, WpVersion};

type CmdResult<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_sites,
            start_site,
            stop_site,
            stop_all_sites,
            exec_wpcli,
            create_site,
            list_wp_versions
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar la aplicación Tauri");
}
