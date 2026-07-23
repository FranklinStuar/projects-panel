//! Panel WP — backend Tauri.

mod autodump;
mod autologin;
mod backup;
mod cli;
mod clone;
mod config;
mod dbus;
mod docker;
mod domain;
mod dumplog;
mod github;
mod groups;
mod localwp;
mod logs;
mod migrate;
mod netcheck;
mod nginx;
mod php;
mod progress;
mod snapshot;
mod ssl;
mod system;
mod wordpress;
mod worktree;
mod wpcli;

#[cfg(test)]
mod integration_tests;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};
use tokio::task::JoinHandle;

use autodump::AutoDump;
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
async fn start_site(autodump: State<'_, AutoDump>, id: String) -> CmdResult<()> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    docker.start_site(&site).await.map_err(e)?;
    // Auto-dump: a partir de aquí cada cambio en la DB deja un dump fresco.
    autodump.start(site);
    Ok(())
}

#[tauri::command]
async fn stop_site(autodump: State<'_, AutoDump>, id: String) -> CmdResult<()> {
    let all = config::load_all_sites().map_err(e)?;
    let site = all
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    // Parar el watcher antes del stop: el propio stop ya exporta el dump final.
    autodump.stop(&id);
    let docker = DockerManager::connect().map_err(e)?;
    docker.stop_site(&site, &all).await.map_err(e)
}

#[tauri::command]
async fn stop_all_sites(autodump: State<'_, AutoDump>) -> CmdResult<()> {
    let all = config::load_all_sites().map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    for site in &all {
        autodump.stop(&site.id);
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

/// Punto de publicación del panel (IP loopback + puertos host). El frontend lo
/// usa para mostrar la URL real del sitio cuando hay puerto alterno.
#[tauri::command]
fn panel_endpoint() -> CmdResult<config::Endpoint> {
    Ok(config::endpoint_or_default())
}

// -- Fase 4: estado del sistema / primera configuración ----------------------

/// Estado de los prerequisitos del panel (Docker, red, dnsmasq, mkcert, etc.)
/// para la pantalla de configuración.
#[tauri::command]
async fn system_status() -> CmdResult<system::SystemStatus> {
    Ok(system::status().await)
}

/// Crea el bridge `panel-net` si falta (idempotente).
#[tauri::command]
async fn create_panel_network() -> CmdResult<()> {
    let docker = DockerManager::connect().map_err(e)?;
    docker.ensure_network().await.map_err(e)
}

/// Olvida el endpoint persistido para reasignar puerto en el próximo arranque.
#[tauri::command]
fn reset_endpoint() -> CmdResult<()> {
    config::clear_endpoint().map_err(e)
}

/// Migra un proyecto pendiente al sistema actual (crea DB, importa dump, SSL) y
/// lo enciende. Devuelve la config actualizada + un aviso opcional.
#[tauri::command]
async fn migrate_site(app: AppHandle, id: String) -> CmdResult<migrate::Migration> {
    let site = load_site(&id)?;
    let docker = DockerManager::connect().map_err(e)?;
    migrate::migrate_site(&app, &docker, &site).await.map_err(e)
}

/// Borra un proyecto: lo apaga (si corre), quita su container y vhost, y elimina
/// la base de datos del servidor compartido («borra todos los datos»).
///
/// `delete_folder` decide qué pasa con la carpeta de `~/panel-wp/`:
/// - `true`  → se borra entera (no queda nada en disco).
/// - `false` → se conserva, pero se quita su `config.json` para que el panel la
///   olvide (queda «desconectada»). Los archivos (app/public, conf, dumps en
///   app/sql) siguen en disco para poder reconfigurar el proyecto más tarde.
///
/// Pensado también para cancelar una importación con el proyecto equivocado.
#[tauri::command]
async fn delete_site(app: AppHandle, id: String, delete_folder: bool) -> CmdResult<()> {
    use crate::progress::log;
    let all = config::load_all_sites().map_err(e)?;
    let site = all
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    // Apaga + exporta dump fresco a app/sql + quita vhost + teardown de
    // compartidos (no-op si está pendiente).
    log(&app, "Apagando el proyecto y quitando su vhost…");
    docker.stop_site(&site, &all).await.ok();
    // Asegura que no quede el container php creado.
    docker.remove_container(&site.container_name()).await.ok();
    // Borra la base de datos del servidor compartido (datos del proyecto). Hay
    // que levantar el container de DB para ejecutar el DROP; luego se vuelve a
    // apagar si ningún otro proyecto activo lo usa.
    log(
        &app,
        format!(
            "Borrando la base de datos «{}» del servidor compartido…",
            site.services.db.db_name
        ),
    );
    if let Ok(db_container) = docker.ensure_db(&site.services.db).await {
        wordpress::drop_database(&docker, &db_container, &site).await.ok();
    }
    docker.teardown_unused_shared(&site, &all).await.ok();

    if delete_folder {
        // Borra la carpeta del proyecto entera.
        log(&app, "Borrando la carpeta del proyecto del disco…");
        std::fs::remove_dir_all(&site.path).map_err(e)?;
    } else {
        // Desconecta: en vez de borrar la config, la renombra a un sidecar
        // (`config.disconnected.json`). `load_all_sites()` solo escanea
        // `config.json`, así que el panel la olvida; pero la metadata se
        // conserva para re-importar el proyecto sin pérdida más tarde (otra PC,
        // tras formatear…). Ver `import_disconnected_site`.
        log(
            &app,
            "Desconectando el proyecto del panel (se conserva la carpeta y su configuración para reimportar)…",
        );
        let cfg = std::path::Path::new(&site.path).join("config.json");
        let sidecar = config::disconnected_config_path(&site.path);
        std::fs::rename(&cfg, &sidecar).map_err(e)?;
    }
    Ok(())
}

/// Lista las carpetas de `~/panel-wp/` que ya no están registradas en el panel
/// pero siguen en disco (proyectos desconectados, copiados de otra PC, etc.),
/// candidatas a re-importar.
#[tauri::command]
fn list_disconnected_sites() -> CmdResult<Vec<config::DisconnectedSite>> {
    config::list_disconnected_sites().map_err(e)
}

/// Re-importa una carpeta desconectada al panel: restaura su `config.json`
/// (desde el sidecar `config.disconnected.json`, o reconstruido best-effort si
/// no lo hay) y la deja como `migrationPending`. El usuario la enciende luego
/// con «Migrar y encender», que recrea la DB e importa el último dump.
#[tauri::command]
async fn import_disconnected_site(
    app: AppHandle,
    folder_name: String,
) -> CmdResult<localwp::ImportResult> {
    import_disconnected(&app, &folder_name).map_err(e)
}

/// Núcleo de `import_disconnected_site`, genérico sobre el runtime para poder
/// ejercitarlo en tests con `tauri::test::mock_app()`. Devuelve `anyhow::Result`.
fn import_disconnected<R: tauri::Runtime>(
    app: &AppHandle<R>,
    folder_name: &str,
) -> anyhow::Result<localwp::ImportResult> {
    use crate::progress::log;
    use anyhow::anyhow;

    // Resolver la ruta bajo ~/panel-wp/ y validar que es un proyecto en disco.
    let root = config::projects_root()?;
    let dir = root.join(folder_name);
    if !dir.is_dir() {
        return Err(anyhow!("no existe la carpeta {folder_name} en panel-wp"));
    }
    if dir.join("config.json").exists() {
        return Err(anyhow!("«{folder_name}» ya está en el panel"));
    }
    let path = dir.to_string_lossy().into_owned();
    if !dir.join("app").join("public").exists() {
        return Err(anyhow!(
            "la carpeta {folder_name} no contiene app/public (no es un proyecto del panel)"
        ));
    }

    log(app, format!("▶ Re-importando «{folder_name}»…"));

    let existing = config::load_all_sites()?;
    let sidecar = config::disconnected_config_path(&path);

    let mut site = if sidecar.exists() {
        log(app, "• Restaurando la configuración conservada…");
        let mut cfg = config::read_site_config(&sidecar)?;
        // La carpeta pudo moverse (otra PC, otra ruta): fijar la ruta actual.
        cfg.path = path.clone();
        cfg
    } else {
        log(app, "• Sin configuración conservada: reconstruyendo (best-effort)…");
        reconstruct_config(folder_name, &dir)
    };

    // Evitar colisión de id con un proyecto vivo (carpeta copiada/duplicada).
    if existing.iter().any(|s| s.id == site.id) {
        site.id = uuid::Uuid::new_v4().to_string();
    }
    // Queda pendiente: la DB se crea/importa con «Migrar y encender».
    site.migration_pending = true;
    site.last_migrated_at = None;

    config::write_site_config(&site)?;
    // Quitar el sidecar: ya hay config.json (fuente de verdad).
    if sidecar.exists() {
        std::fs::remove_file(&sidecar).ok();
    }

    log(
        app,
        format!("✓ «{}» re-importado → usa «Migrar y encender» en Proyectos.", site.name),
    );
    Ok(localwp::ImportResult { site, note: None })
}

/// Reconstruye un `SiteConfig` best-effort para una carpeta sin sidecar: nombre
/// = carpeta, dominio `{folder}.test`, `dbName` deducido de `wp-config.php` (o
/// del slug), versiones por defecto. Queda `migrationPending`.
fn reconstruct_config(folder_name: &str, dir: &std::path::Path) -> SiteConfig {
    use config::{DbService, DbType, GithubConfig, NginxService, PhpService, Services};

    let slug = wordpress::slugify(folder_name);
    let db_name = std::fs::read_to_string(dir.join("app").join("public").join("wp-config.php"))
        .ok()
        .and_then(|raw| config::parse_db_name(&raw))
        .unwrap_or_else(|| format!("{}_db", slug.replace('-', "_")));

    SiteConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: folder_name.to_string(),
        path: dir.to_string_lossy().into_owned(),
        domain: format!("{slug}.test"),
        group: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        services: Services {
            php: PhpService { version: "8.3".into(), ..Default::default() },
            nginx: NginxService { ssl: true },
            db: DbService {
                db_type: DbType::Mysql,
                version: "8.0".into(),
                db_name,
            },
        },
        github: GithubConfig::default(),
        one_click_admin: true,
        xdebug_enabled: false,
        headless: false,
        frontend_framework: None,
        minio: false,
        migration_pending: true,
        last_migrated_at: None,
        clone_of: None,
        worktree_of: None,
        snapshot_excludes: vec![],
    }
}

/// Lista los sitios de LocalWP candidatos a importar.
#[tauri::command]
fn list_localwp_sites() -> CmdResult<Vec<localwp::LocalSite>> {
    localwp::list_sites().map_err(e)
}

/// Importa un sitio de LocalWP como proyecto del panel (queda `migrationPending`).
#[tauri::command]
async fn import_localwp_site(app: AppHandle, id: String) -> CmdResult<localwp::ImportResult> {
    localwp::import_site(&app, &id).map_err(e)
}

/// Abre el admin en el navegador (auto-login si el proyecto lo tiene activado).
/// `user_id` = ID de usuario WordPress; None o 0 = primer administrador.
#[tauri::command]
async fn open_admin(app: AppHandle, id: String, user_id: Option<u64>) -> CmdResult<()> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    autologin::open_admin(&app, &docker, &site, user_id).await.map_err(e)
}

/// Lista los usuarios de WordPress del proyecto (ID, login, display_name, roles).
#[tauri::command]
async fn list_wp_users(id: String) -> CmdResult<Vec<serde_json::Value>> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    let args = vec![
        "user".to_string(),
        "list".to_string(),
        "--fields=ID,user_login,display_name,roles".to_string(),
        "--format=json".to_string(),
    ];
    let out = wpcli::run(&docker, &site, &args).await.map_err(e)?;
    serde_json::from_str::<Vec<serde_json::Value>>(&out)
        .map_err(|err| format!("no se pudo parsear lista de usuarios: {err}"))
}

/// (Re)inyecta el mu-plugin de auto-login (y el de mailpit) en un proyecto y
/// activa `oneClickAdmin`. Pensado para proyectos importados de LocalWP, que se
/// crearon sin estos mu-plugins y por eso no auto-logueaban al admin. Es
/// idempotente y no requiere que el proyecto esté encendido (los mu-plugins van
/// montados desde el disco).
#[tauri::command]
async fn repair_autologin(id: String) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    site.one_click_admin = true;
    config::write_site_config(&site).map_err(e)?;
    wordpress::sync_mu_plugins(&site).map_err(e)?;
    Ok(site)
}

/// Regenera el php.ini de todos los proyectos desde el template actual.
/// Útil para aplicar cambios de configuración (p. ej. OPcache) a proyectos existentes.
/// Los proyectos deben reiniciarse para que el cambio surta efecto.
#[tauri::command]
async fn repair_all_php_ini() -> CmdResult<String> {
    let sites = config::load_all_sites().map_err(e)?;
    let total = sites.len();
    let mut ok = 0usize;
    let mut errors = Vec::new();
    for site in &sites {
        match wordpress::write_php_ini(site) {
            Ok(_) => ok += 1,
            Err(err) => errors.push(format!("{}: {}", site.name, err)),
        }
    }
    if errors.is_empty() {
        Ok(format!("php.ini actualizado en {ok}/{total} proyectos. Reinicia los que estén encendidos."))
    } else {
        Ok(format!(
            "php.ini actualizado en {ok}/{total} proyectos.\nErrores:\n{}",
            errors.join("\n")
        ))
    }
}

/// Ajusta el tope de subida (MB) del proyecto: reescribe su php.ini
/// (`upload_max_filesize` + `post_max_size`) y, si está encendido, recarga
/// php-fpm en caliente (SIGUSR2) para aplicarlo sin recrear el container.
/// `mb = 0` vuelve al default del template (64M). Devuelve la config actualizada.
#[tauri::command]
async fn set_php_upload_limit(id: String, mb: u32) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    site.services.php.upload_max_mb = if mb == 0 { None } else { Some(mb) };
    wordpress::write_php_ini(&site).map_err(e)?;
    config::write_site_config(&site).map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    if docker.is_running(&site.container_name()).await {
        docker
            .exec(&site.container_name(), vec!["kill", "-USR2", "1"])
            .await
            .ok();
    }
    Ok(site)
}

/// Repara el reverse-proxy nginx: poda vhosts huérfanos (de proyectos cuyo
/// container ya no corre) y recrea el container. Úsalo cuando ningún sitio
/// carga tras un apagón sucio (un upstream caído aborta el arranque de nginx).
#[tauri::command]
async fn repair_nginx() -> CmdResult<String> {
    let docker = DockerManager::connect().map_err(e)?;
    let pruned = docker.repair_nginx().await.map_err(e)?;
    Ok(format!(
        "nginx reiniciado. {} vhost(s) huérfano(s) podado(s).",
        pruned.len()
    ))
}

/// Abre la web pública del proyecto (home, sin auto-login) en el navegador.
#[tauri::command]
async fn open_site(app: AppHandle, id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    let docker = DockerManager::connect().map_err(e)?;
    if !docker.is_running(&site.container_name()).await {
        return Err(format!("el proyecto '{}' no está encendido", site.name));
    }
    let url = config::endpoint_or_default().site_url(&site.domain, site.services.nginx.ssl);
    app.opener().open_url(url, None::<&str>).map_err(e)
}

/// Abre la carpeta del proyecto en el explorador de archivos.
#[tauri::command]
async fn open_folder(app: AppHandle, id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    app.opener()
        .open_path(site.path.clone(), None::<&str>)
        .map_err(e)
}

/// Abre una terminal del sistema en la carpeta del proyecto, con el wrapper `wp`
/// listo (lo instala si hace falta). Dentro basta con ejecutar `wp <args>`.
#[tauri::command]
async fn open_terminal(id: String) -> CmdResult<()> {
    let site = config::find_site(&id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))?;
    // Idempotente: garantiza que `wp` exista antes de abrir la terminal.
    cli::install_cli_wrapper().map_err(e)?;
    cli::open_terminal_at(std::path::Path::new(&site.path)).map_err(e)
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
    // Asegura que un grupo asignado por drag&drop quede registrado en groups.json.
    if let Some(g) = &site.group {
        groups::create(g).map_err(e)?;
    }
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

// -- Grupos de proyectos (groups.json) ---------------------------------------

/// Lista de grupos persistidos, en orden.
#[tauri::command]
async fn list_groups() -> CmdResult<Vec<String>> {
    groups::list().map_err(e)
}

/// Crea un grupo vacío (idempotente).
#[tauri::command]
async fn create_group(name: String) -> CmdResult<()> {
    groups::create(&name).map_err(e)
}

/// Renombra un grupo y reasigna los proyectos que lo tenían.
#[tauri::command]
async fn rename_group(old: String, new: String) -> CmdResult<()> {
    groups::rename(&old, &new).map_err(e)
}

/// Borra un grupo; sus proyectos quedan sin grupo.
#[tauri::command]
async fn delete_group(name: String) -> CmdResult<()> {
    groups::delete(&name).map_err(e)
}

/// Sobrescribe el orden de los grupos.
#[tauri::command]
async fn reorder_groups(order: Vec<String>) -> CmdResult<()> {
    groups::reorder(order).map_err(e)
}

// -- Fase 3: servicios adicionales -------------------------------------------

/// Activa/desactiva MinIO (S3 local) para un proyecto. Si está encendido,
/// arranca/para el servicio compartido al instante.
#[tauri::command]
async fn set_site_minio(id: String, enabled: bool) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    site.minio = enabled;
    config::write_site_config(&site).map_err(e)?;
    let docker = DockerManager::connect().map_err(e)?;
    if enabled && docker.is_running(&site.container_name()).await {
        docker.ensure_minio().await.map_err(e)?;
    }
    Ok(site)
}

/// Exporta la base de datos del proyecto a `app/sql/`. Devuelve la ruta del dump.
#[tauri::command]
async fn export_db(id: String) -> CmdResult<String> {
    let site = load_site(&id)?;
    let docker = DockerManager::connect().map_err(e)?;
    let path = backup::export_db(&docker, &site).await.map_err(e)?;
    dumplog::append(&site, &path, "manual").ok();
    Ok(path)
}

/// Devuelve el log de volcados de DB (más nuevos primero) para revisión.
#[tauri::command]
async fn dump_log() -> CmdResult<Vec<dumplog::DumpLogEntry>> {
    dumplog::read_all().map_err(e)
}

/// Limpia el log de volcados por fecha (`before`, ISO `YYYY-MM-DD`) y/o por base
/// de datos (`dbName`). Sin filtros borra todo. Devuelve cuántas se eliminaron.
#[tauri::command]
async fn clean_dump_log(before: Option<String>, db_name: Option<String>) -> CmdResult<usize> {
    dumplog::clean(before.as_deref(), db_name.as_deref()).map_err(e)
}

/// Instala los wrappers WP-CLI (`wp`, `wordpress-panel-cli`) en `~/.local/bin`.
#[tauri::command]
async fn install_cli_wrapper() -> CmdResult<String> {
    cli::install_cli_wrapper().map_err(e)
}

/// Abre la UI de Mailpit (correo capturado) en el navegador.
#[tauri::command]
async fn open_mailpit(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let docker = DockerManager::connect().map_err(e)?;
    if !docker.is_running(docker::MAILPIT).await {
        return Err("Mailpit no está corriendo (enciende algún proyecto)".into());
    }
    let url = format!("http://127.0.0.1:{}/", docker::MAILPIT_UI_PORT);
    app.opener().open_url(url, None::<&str>).map_err(e)
}

/// Abre la consola web de MinIO en el navegador.
#[tauri::command]
async fn open_minio(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let docker = DockerManager::connect().map_err(e)?;
    if !docker.is_running(docker::MINIO).await {
        return Err("MinIO no está corriendo (actívalo en un proyecto activo)".into());
    }
    let url = format!("http://127.0.0.1:{}/", docker::MINIO_CONSOLE_PORT);
    app.opener().open_url(url, None::<&str>).map_err(e)
}

/// Abre Adminer en el navegador apuntando a la base de datos del proyecto.
/// Pasa servidor/usuario/db por la URL; el plugin `autologin.php` hace el
/// auto-login en cero clics con las credenciales del entorno.
#[tauri::command]
async fn open_adminer(app: AppHandle, id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let site = load_site(&id)?;
    let docker = DockerManager::connect().map_err(e)?;

    // La DB del proyecto debe estar corriendo (arranca al iniciar el proyecto).
    let db_container = docker::db_container_name(&site.services.db);
    if !docker.is_running(&db_container).await {
        return Err("La base de datos no está corriendo (inicia el proyecto primero)".into());
    }
    docker.ensure_adminer().await.map_err(e)?;

    let db = &site.services.db;
    // Clave del parámetro = driver: `pgsql` para Postgres, `server` para MySQL/MariaDB.
    let (driver_param, user) = match db.db_type {
        config::DbType::Postgres => ("pgsql", "panel"),
        config::DbType::Mysql | config::DbType::Mariadb => ("server", "root"),
    };
    let url = format!(
        "http://127.0.0.1:{port}/?{driver}={server}&username={user}&db={dbname}",
        port = docker::ADMINER_UI_PORT,
        driver = driver_param,
        server = db_container,
        user = user,
        dbname = db.db_name,
    );
    app.opener().open_url(url, None::<&str>).map_err(e)
}

/// Stubs de Fase posterior: devuelven un mensaje informativo (UI preparada).
#[tauri::command]
async fn feature_stub(feature: String) -> CmdResult<String> {
    let label = match feature.as_str() {
        "cloudflare" => "Cloudflare Tunnel",
        "deploy" => "Deploy",
        "package" => "Empaquetado del sitio",
        other => other,
    };
    Err(format!(
        "{label}: aún no implementado. Planificado para una fase posterior."
    ))
}

// -- Fase 5: clones temporales + puntos de guardado --------------------------

/// Crea un punto de guardado del proyecto (tar del código + dump SQL).
/// Emite progreso por `op-log`.
#[tauri::command]
async fn create_snapshot(
    app: AppHandle,
    id: String,
    label: String,
) -> CmdResult<snapshot::SnapshotMeta> {
    let site = load_site(&id)?;
    let docker = DockerManager::connect().map_err(e)?;
    snapshot::create_snapshot(&app, &docker, &site, &label).await.map_err(e)
}

/// Lista los puntos de guardado del proyecto (más reciente primero).
#[tauri::command]
fn list_snapshots(id: String) -> CmdResult<Vec<snapshot::SnapshotMeta>> {
    let site = load_site(&id)?;
    snapshot::list_snapshots(&site).map_err(e)
}

/// Borra un punto de guardado del disco.
#[tauri::command]
fn delete_snapshot(id: String, snapshot_id: String) -> CmdResult<()> {
    let site = load_site(&id)?;
    snapshot::delete_snapshot(&site, &snapshot_id).map_err(e)
}

/// Detecta carpetas candidatas a excluir del punto de guardado (backups, etc.).
#[tauri::command]
fn detect_excludable(id: String) -> CmdResult<Vec<snapshot::ExcludableEntry>> {
    let site = load_site(&id)?;
    snapshot::detect_excludable(&site).map_err(e)
}

/// Persiste la lista de rutas a excluir del punto de guardado de este proyecto.
#[tauri::command]
fn set_snapshot_excludes(id: String, excludes: Vec<String>) -> CmdResult<()> {
    let mut site = load_site(&id)?;
    let mut clean: Vec<String> = excludes
        .into_iter()
        .map(|s| s.trim().trim_start_matches("./").trim_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    clean.sort();
    clean.dedup();
    site.snapshot_excludes = clean;
    config::write_site_config(&site).map_err(e)
}

/// Crea un clone temporal desde un punto de guardado. Emite progreso por `op-log`.
#[tauri::command]
async fn create_clone(
    app: AppHandle,
    parent_id: String,
    snapshot_id: String,
) -> CmdResult<SiteConfig> {
    let docker = DockerManager::connect().map_err(e)?;
    clone::create_clone(&app, &docker, &parent_id, &snapshot_id)
        .await
        .map_err(e)
}

// -- Worktree-projects (probar una rama de un repo en aislamiento) ------------

/// Crea un worktree-project del repo `target_path` del padre `parent_id` sobre la
/// rama `branch` (desde `base_branch`, o la rama actual). `shared_db`: compartir
/// el esquema del padre o copiarlo. Emite progreso por `op-log`.
#[tauri::command]
async fn create_worktree_site(
    app: AppHandle,
    parent_id: String,
    target_path: String,
    branch: String,
    base_branch: Option<String>,
    shared_db: bool,
) -> CmdResult<SiteConfig> {
    let docker = DockerManager::connect().map_err(e)?;
    worktree::create_worktree(
        &app,
        &docker,
        &parent_id,
        &target_path,
        &branch,
        base_branch.as_deref(),
        shared_db,
    )
    .await
    .map_err(e)
}

/// Elimina un worktree-project: lo apaga, hace `git worktree remove` (la rama
/// queda en el repo del padre), borra el esquema si era copia y borra la carpeta.
/// `delete_branch`: además borrar la rama. Emite progreso por `op-log`.
#[tauri::command]
async fn remove_worktree_site(
    app: AppHandle,
    id: String,
    delete_branch: bool,
) -> CmdResult<()> {
    let docker = DockerManager::connect().map_err(e)?;
    worktree::remove_worktree(&app, &docker, &id, delete_branch)
        .await
        .map_err(e)
}

/// Lista los worktree-projects de un proyecto padre.
#[tauri::command]
fn list_worktrees(parent_id: String) -> CmdResult<Vec<SiteConfig>> {
    worktree::list_worktrees(&parent_id).map_err(e)
}

fn load_site(id: &str) -> CmdResult<SiteConfig> {
    config::find_site(id)
        .map_err(e)?
        .ok_or_else(|| format!("proyecto {id} no encontrado"))
}

/// Clona un repo y lo registra en config.json. `kind` ("theme"|"plugin"|
/// "muplugin") propone una ruta bajo wp-content; si se pasa `path` explícito
/// (relativo a public/) tiene prioridad, así el repo puede ir a cualquier sitio.
#[tauri::command]
async fn gh_clone(
    id: String,
    kind: String,
    repo: String,
    branch: String,
    path: Option<String>,
) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    let rel_path = path
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| github::propose_path(&kind, &repo));
    github::clone(&site, &repo, &branch, &rel_path)
        .await
        .map_err(e)?;

    site.github.repos.push(config::GithubRepo {
        repo,
        branch,
        path: rel_path,
        build_cmd: None,
        build_dirs: vec![],
    });
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
    for r in &site.github.repos {
        out.push_str(&format!("== {} ({}) ==\n", r.path, r.repo));
        out.push_str(&github::pull(&site, &r.path, &r.branch).await.unwrap_or_else(|err| e(err)));
        out.push('\n');
    }
    if out.is_empty() {
        out.push_str("No hay repos registrados.");
    }
    Ok(out)
}

/// Quita un repo: borra la carpeta y lo desregistra de config.json.
#[tauri::command]
async fn gh_remove(id: String, path: String) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    github::remove_dir(&site, &path).map_err(e)?;
    site.github.repos.retain(|r| r.path != path);
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

/// Escanea wp-content buscando repos git (registrados o huérfanos).
#[tauri::command]
async fn gh_scan(id: String) -> CmdResult<Vec<github::DetectedRepo>> {
    let site = load_site(&id)?;
    Ok(github::scan(&site).await)
}

/// Registra en config.json un repo git ya presente en disco (huérfano), leyendo
/// su remoto y rama actuales. No clona ni descarga nada.
#[tauri::command]
async fn gh_register(id: String, path: String) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    if site.github.repos.iter().any(|r| r.path == path) {
        return Ok(site); // ya registrado
    }
    let (repo, branch) = github::read_repo_meta(&site, &path).await.map_err(e)?;
    site.github.repos.push(config::GithubRepo { repo, branch, path, build_cmd: None, build_dirs: vec![] });
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

/// Estado de una rama frente a su remoto (fetch + ahead/behind + árbol sucio),
/// para decidir si se puede hacer deploy directo.
#[tauri::command]
async fn gh_branch_status(id: String, path: String, branch: String) -> CmdResult<github::BranchStatus> {
    let site = load_site(&id)?;
    github::branch_status(&site, &path, &branch).await.map_err(e)
}

/// Guarda la rama objetivo, el comando de build y las carpetas de build de un
/// repo registrado (config del deploy directo). El repo debe estar registrado.
#[tauri::command]
async fn gh_set_deploy(
    id: String,
    path: String,
    branch: String,
    build_cmd: Option<String>,
    build_dirs: Vec<String>,
) -> CmdResult<SiteConfig> {
    let mut site = load_site(&id)?;
    let repo = site
        .github
        .repos
        .iter_mut()
        .find(|r| r.path == path)
        .ok_or_else(|| "el repo no está registrado; regístralo primero".to_string())?;
    if !branch.trim().is_empty() {
        repo.branch = branch.trim().to_string();
    }
    repo.build_cmd = build_cmd.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    repo.build_dirs = build_dirs
        .into_iter()
        .map(|d| d.trim().trim_matches('/').to_string())
        .collect();
    config::write_site_config(&site).map_err(e)?;
    Ok(site)
}

/// Carpetas candidatas para el build dentro de un repo (raíz + subcarpetas con
/// package.json), para el selector de la UI.
#[tauri::command]
async fn gh_build_dirs(id: String, path: String) -> CmdResult<Vec<String>> {
    let site = load_site(&id)?;
    Ok(github::build_dir_candidates(&site, &path))
}

/// Deploy directo de un repo registrado: checkout + `git pull --ff-only` + build
/// (si hay comando configurado) en cada carpeta de build. Emite progreso al op-log.
#[tauri::command]
async fn gh_deploy(app: AppHandle, id: String, path: String) -> CmdResult<()> {
    let site = load_site(&id)?;
    let repo = site
        .github
        .repos
        .iter()
        .find(|r| r.path == path)
        .ok_or_else(|| "el repo no está registrado".to_string())?;
    github::deploy(&app, &site, &path, &repo.branch, repo.build_cmd.as_deref(), &repo.build_dirs)
        .await
        .map_err(e)
}

/// Abre el proyecto en VSCode. Genera (si no existe) un `.code-workspace` con
/// app/public como carpeta principal y cada repo git detectado como adicional,
/// y abre ese workspace.
#[tauri::command]
async fn open_vscode(id: String) -> CmdResult<()> {
    let site = load_site(&id)?;
    let ws = github::ensure_workspace(&site).await.map_err(e)?;
    github::open_vscode(&ws).map_err(e)
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
        .manage(AutoDump::default())
        .setup(|app| {
            // Instala los wrappers WP-CLI (`wp`, `wordpress-panel-cli`) una vez al
            // arrancar. Son globales del usuario (en ~/.local/bin) y detectan el
            // proyecto por el CWD, así que no hay nada por-proyecto que instalar.
            // Idempotente y best-effort: si falla (p. ej. ~/.local/bin no escribible),
            // el botón manual sigue disponible.
            if let Err(err) = cli::install_cli_wrapper() {
                eprintln!("no se pudieron instalar los wrappers WP-CLI: {err}");
            }

            // Auto-dump para proyectos que ya estaban activos al abrir el panel
            // (containers que sobrevivieron a la sesión anterior). Los que se
            // arranquen luego enganchan su watcher en `start_site`.
            let autodump_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let Ok(docker) = DockerManager::connect() else {
                    return;
                };
                let Ok(sites) = config::load_all_sites() else {
                    return;
                };
                let state = autodump_handle.state::<AutoDump>();
                for site in sites {
                    if docker.is_running(&site.container_name()).await {
                        state.start(site);
                    }
                }
            });

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
            panel_endpoint,
            system_status,
            create_panel_network,
            reset_endpoint,
            migrate_site,
            delete_site,
            list_localwp_sites,
            import_localwp_site,
            list_disconnected_sites,
            import_disconnected_site,
            open_admin,
            list_wp_users,
            repair_autologin,
            repair_all_php_ini,
            repair_nginx,
            set_php_upload_limit,
            open_site,
            open_folder,
            open_terminal,
            stream_logs,
            stop_logs,
            list_plugins,
            list_themes,
            gh_status,
            gh_clone,
            gh_pull,
            gh_pull_all,
            gh_remove,
            gh_scan,
            gh_register,
            gh_branch_status,
            gh_set_deploy,
            gh_build_dirs,
            gh_deploy,
            open_vscode,
            regenerate_ssl,
            set_site_group,
            list_groups,
            create_group,
            rename_group,
            delete_group,
            reorder_groups,
            set_site_minio,
            export_db,
            dump_log,
            clean_dump_log,
            install_cli_wrapper,
            open_mailpit,
            open_minio,
            open_adminer,
            feature_stub,
            create_snapshot,
            list_snapshots,
            delete_snapshot,
            detect_excludable,
            set_snapshot_excludes,
            create_clone,
            create_worktree_site,
            remove_worktree_site,
            list_worktrees
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar la aplicación Tauri");
}
