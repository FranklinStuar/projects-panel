//! Descarga e instalación de WordPress.
//!
//! El core se baja por tarball desde wordpress.org (control de versión + cache),
//! NO desde una imagen `wordpress:*`. La instalación corre vía WP-CLI dentro del
//! container php del proyecto.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;
use uuid::Uuid;

use crate::config::{
    write_site_config, DbService, DbType, GithubConfig, NginxService, PhpService, Services,
    SiteConfig,
};
use crate::docker::DockerManager;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSiteRequest {
    pub name: String,
    pub domain: Option<String>,
    pub wp_version: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    pub php_version: String,
    pub db_type: DbType,
    pub db_version: String,
    pub admin_user: String,
    pub admin_password: String,
    pub admin_email: String,
    pub title: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub one_click_admin: bool,
    #[serde(default)]
    pub xdebug: bool,
    #[serde(default)]
    pub headless: bool,
    pub frontend_framework: Option<String>,
    #[serde(default)]
    pub minio: bool,
    pub group: Option<String>,
}

fn default_locale() -> String {
    "en_US".to_string()
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct WpVersion {
    pub version: String,
    pub status: String, // "latest" | "outdated" | "insecure"
}

/// Lista de versiones de WordPress desde la API de wordpress.org, con cache 24h.
/// Ordenada de más nueva a más antigua.
pub async fn fetch_versions() -> Result<Vec<WpVersion>> {
    let cache = crate::config::config_dir()?.join("wp-versions.json");

    // cache fresca (<24h)
    if let Ok(meta) = std::fs::metadata(&cache) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().map(|d| d.as_secs() < 86_400).unwrap_or(false) {
                if let Ok(raw) = std::fs::read_to_string(&cache) {
                    if let Ok(v) = serde_json::from_str::<Vec<WpVersion>>(&raw) {
                        return Ok(v);
                    }
                }
            }
        }
    }

    let map: std::collections::HashMap<String, String> =
        reqwest::get("https://api.wordpress.org/core/stable-check/1.0/")
            .await
            .context("consultando versiones de WordPress")?
            .error_for_status()?
            .json()
            .await?;

    let mut versions: Vec<WpVersion> = map
        .into_iter()
        .map(|(version, status)| WpVersion { version, status })
        .collect();
    versions.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));

    std::fs::write(&cache, serde_json::to_string(&versions)?).ok();
    Ok(versions)
}

/// Clave numérica para ordenar versiones tipo "6.7.2".
fn version_key(v: &str) -> (u32, u32, u32) {
    let mut it = v.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Crea el proyecto de principio a fin y lo deja encendido.
pub async fn create_site(docker: &DockerManager, req: NewSiteRequest) -> Result<SiteConfig> {
    let id = Uuid::new_v4().to_string();
    let root = crate::config::projects_root()?;
    let slug = slugify(&req.name);
    let path = root.join(&slug);
    if path.exists() {
        return Err(anyhow!("ya existe un proyecto en {:?}", path));
    }

    let domain = req
        .domain
        .clone()
        .unwrap_or_else(|| format!("{slug}.test"));
    let db_name = format!("{}_db", slug.replace('-', "_"));

    let site = SiteConfig {
        id: id.clone(),
        name: req.name.clone(),
        path: path.to_string_lossy().to_string(),
        domain: domain.clone(),
        group: req.group.clone(),
        created_at: Utc::now().to_rfc3339(),
        services: Services {
            php: PhpService {
                version: req.php_version.clone(),
            },
            nginx: NginxService { ssl: req.ssl },
            db: DbService {
                db_type: req.db_type,
                version: req.db_version.clone(),
                db_name: db_name.clone(),
            },
        },
        github: GithubConfig::default(),
        one_click_admin: req.one_click_admin,
        xdebug_enabled: req.xdebug,
        headless: req.headless,
        frontend_framework: req.frontend_framework.clone(),
        minio: req.minio,
        migration_pending: false,
        last_migrated_at: None,
    };

    // 1. estructura de carpetas
    create_dirs(&site)?;
    // 2. php.ini desde template
    write_php_ini(&site)?;
    // 3. config.json
    write_site_config(&site)?;

    // 4. DB compartida on-demand + crear base de datos del proyecto
    let db_container = docker.ensure_db(&site.services.db).await?;
    create_database(docker, &db_container, &site).await?;

    // 5. descargar core WordPress (tarball)
    download_core(&req.wp_version, &site.public_dir()).await?;

    // 6. mu-plugins: mailpit (siempre) + auto-login (si one-click)
    inject_mailpit_muplugin(&site)?;
    if site.one_click_admin {
        inject_autologin_muplugin(&site)?;
    }

    // 7. SSL: generar certificado antes de levantar nginx (el vhost lo referencia)
    if site.services.nginx.ssl {
        crate::ssl::generate(&site).await?;
    }

    // 8. encender container php + vhost + nginx
    docker.start_site(&site).await?;

    // 9. wp-config + core install vía WP-CLI
    wp_config_create(docker, &site, &db_container).await?;
    wp_core_install(docker, &site, &req).await?;

    Ok(site)
}

fn create_dirs(site: &SiteConfig) -> Result<()> {
    let base = Path::new(&site.path);
    for sub in [
        "app/public",
        "app/sql",
        "conf/php",
        "logs/php",
        "ssl",
        "data",
    ] {
        std::fs::create_dir_all(base.join(sub))
            .with_context(|| format!("creando {sub}"))?;
    }
    Ok(())
}

fn write_php_ini(site: &SiteConfig) -> Result<()> {
    let tmpl = crate::docker::docker_assets_dir().join("php.ini.tmpl");
    let content = std::fs::read_to_string(&tmpl)
        .unwrap_or_else(|_| DEFAULT_PHP_INI.to_string());
    std::fs::write(site.php_ini(), content)?;
    Ok(())
}

const DEFAULT_PHP_INI: &str = "memory_limit = 256M\nupload_max_filesize = 64M\npost_max_size = 64M\nmax_execution_time = 120\n";

/// Descarga `wordpress-{version}.tar.gz` y lo extrae en `public/` (strip wordpress/).
pub async fn download_core(version: &str, public: &Path) -> Result<()> {
    let url = format!("https://wordpress.org/wordpress-{version}.tar.gz");
    let bytes = reqwest::get(&url)
        .await
        .with_context(|| format!("descargando {url}"))?
        .error_for_status()
        .with_context(|| format!("versión WP no encontrada: {version}"))?
        .bytes()
        .await?;

    let tmp = std::env::temp_dir().join(format!("wp-{version}.tar.gz"));
    std::fs::write(&tmp, &bytes)?;

    let status = Command::new("tar")
        .args([
            "-xzf",
            tmp.to_str().unwrap(),
            "--strip-components=1",
            "-C",
            public.to_str().context("ruta public inválida")?,
        ])
        .status()
        .await
        .context("extrayendo tarball de WordPress")?;
    std::fs::remove_file(&tmp).ok();

    if !status.success() {
        return Err(anyhow!("fallo al extraer WordPress {version}"));
    }
    Ok(())
}

/// Crea la base de datos vacía dentro del container DB compartido.
async fn create_database(
    docker: &DockerManager,
    db_container: &str,
    site: &SiteConfig,
) -> Result<()> {
    let db = &site.services.db;
    match db.db_type {
        DbType::Mysql | DbType::Mariadb => {
            let sql = format!(
                "CREATE DATABASE IF NOT EXISTS `{}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;",
                db.db_name
            );
            docker
                .exec(
                    db_container,
                    vec!["mysql", "-uroot", "-ppanel", "-e", &sql],
                )
                .await?;
        }
        DbType::Postgres => {
            let sql = format!("CREATE DATABASE \"{}\";", db.db_name);
            docker
                .exec(
                    db_container,
                    vec!["psql", "-U", "panel", "-c", &sql],
                )
                .await?;
        }
    }
    Ok(())
}

async fn wp_config_create(
    docker: &DockerManager,
    site: &SiteConfig,
    db_container: &str,
) -> Result<()> {
    let db = &site.services.db;
    let args = vec![
        "config".to_string(),
        "create".to_string(),
        format!("--dbname={}", db.db_name),
        "--dbuser=root".to_string(),
        "--dbpass=panel".to_string(),
        format!("--dbhost={db_container}"),
        "--skip-check".to_string(),
        "--force".to_string(),
    ];
    crate::wpcli::run(docker, site, &args).await?;
    Ok(())
}

async fn wp_core_install(
    docker: &DockerManager,
    site: &SiteConfig,
    req: &NewSiteRequest,
) -> Result<()> {
    let scheme = if site.services.nginx.ssl { "https" } else { "http" };
    let url = format!("{scheme}://{}", site.domain);
    let args = vec![
        "core".to_string(),
        "install".to_string(),
        format!("--url={url}"),
        format!("--title={}", req.title),
        format!("--admin_user={}", req.admin_user),
        format!("--admin_password={}", req.admin_password),
        format!("--admin_email={}", req.admin_email),
        "--skip-email".to_string(),
    ];
    crate::wpcli::run(docker, site, &args).await?;

    // Idioma del sitio (si no es el de por defecto).
    if req.locale != "en_US" {
        let lang = vec![
            "language".to_string(),
            "core".to_string(),
            "install".to_string(),
            req.locale.clone(),
            "--activate".to_string(),
        ];
        crate::wpcli::run(docker, site, &lang).await.ok();
    }
    Ok(())
}

/// Inyecta el mu-plugin que enruta correos a Mailpit con header X-Project-ID.
fn inject_mailpit_muplugin(site: &SiteConfig) -> Result<()> {
    let dir = site.public_dir().join("wp-content").join("mu-plugins");
    std::fs::create_dir_all(&dir)?;

    let tmpl = crate::docker::docker_assets_dir()
        .join("mu-plugins")
        .join("panel-mailpit.php");
    let raw = std::fs::read_to_string(&tmpl)
        .unwrap_or_else(|_| DEFAULT_MAILPIT_MUPLUGIN.to_string());
    // El id real del proyecto sustituye al placeholder.
    let content = raw.replace("__PROJECT_ID__", &site.id);
    std::fs::write(dir.join("panel-mailpit.php"), content)?;
    Ok(())
}

/// Inyecta el mu-plugin de auto-login (token efímero de un solo uso).
fn inject_autologin_muplugin(site: &SiteConfig) -> Result<()> {
    let dir = site.public_dir().join("wp-content").join("mu-plugins");
    std::fs::create_dir_all(&dir)?;
    let tmpl = crate::docker::docker_assets_dir()
        .join("mu-plugins")
        .join("panel-autologin.php");
    let content = std::fs::read_to_string(&tmpl)
        .unwrap_or_else(|_| DEFAULT_AUTOLOGIN_MUPLUGIN.to_string());
    std::fs::write(dir.join("panel-autologin.php"), content)?;
    Ok(())
}

const DEFAULT_AUTOLOGIN_MUPLUGIN: &str = r#"<?php
defined( 'ABSPATH' ) || exit;
add_action( 'init', function () {
    if ( empty( $_GET['panel_autologin'] ) ) { return; }
    $token = preg_replace( '/[^a-z0-9]/i', '', (string) $_GET['panel_autologin'] );
    if ( $token === '' ) { return; }
    $key = 'panel_autologin_' . $token;
    if ( get_transient( $key ) === false ) { return; }
    delete_transient( $key );
    $admins = get_users( array( 'role' => 'administrator', 'number' => 1 ) );
    if ( empty( $admins ) ) { return; }
    wp_set_current_user( $admins[0]->ID );
    wp_set_auth_cookie( $admins[0]->ID, true );
    wp_safe_redirect( admin_url() );
    exit;
} );
"#;

const DEFAULT_MAILPIT_MUPLUGIN: &str = r#"<?php
/**
 * Panel WP — enruta el correo del proyecto a Mailpit compartido.
 */
add_action( 'phpmailer_init', function ( $mailer ) {
    $mailer->isSMTP();
    $mailer->Host     = 'panel-mailpit';
    $mailer->Port     = 1025;
    $mailer->SMTPAuth = false;
    $mailer->addCustomHeader( 'X-Project-ID', '__PROJECT_ID__' );
} );
"#;

fn slugify(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
