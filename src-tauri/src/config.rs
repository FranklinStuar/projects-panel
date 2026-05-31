//! Modelos de datos y persistencia.
//!
//! Fuente de verdad: el `config.json` dentro de cada carpeta de proyecto
//! (`~/panel-wp/{site-name}/config.json`). El panel escanea esa carpeta al
//! arrancar y reconstruye el registro — así una carpeta copiada a otro sistema
//! se detecta sola (ver filosofía de migración en PLAN.md).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbType {
    Mysql,
    Mariadb,
    Postgres,
}

impl DbType {
    /// Prefijo del nombre del container compartido (`panel-mysql-80`, ...).
    pub fn service_prefix(&self) -> &'static str {
        match self {
            DbType::Mysql => "panel-mysql",
            DbType::Mariadb => "panel-mariadb",
            DbType::Postgres => "panel-postgres",
        }
    }

    /// Imagen oficial. Nota: no existen variantes alpine de mysql/mariadb.
    pub fn image(&self, version: &str) -> String {
        match self {
            DbType::Mysql => format!("mysql:{version}"),
            DbType::Mariadb => format!("mariadb:{version}"),
            DbType::Postgres => format!("postgres:{version}-alpine"),
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            DbType::Mysql | DbType::Mariadb => 3306,
            DbType::Postgres => 5432,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpService {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxService {
    pub ssl: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbService {
    #[serde(rename = "type")]
    pub db_type: DbType,
    pub version: String,
    #[serde(rename = "dbName")]
    pub db_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Services {
    pub php: PhpService,
    pub nginx: NginxService,
    pub db: DbService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRepo {
    pub repo: String,
    pub branch: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubConfig {
    pub theme: Option<GithubRepo>,
    #[serde(default)]
    pub plugins: Vec<GithubRepo>,
}

impl Default for GithubConfig {
    fn default() -> Self {
        GithubConfig {
            theme: None,
            plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteConfig {
    pub id: String,
    pub name: String,
    pub path: String,
    pub domain: String,
    pub group: Option<String>,
    pub created_at: String,
    pub services: Services,
    #[serde(default)]
    pub github: GithubConfig,
    pub one_click_admin: bool,
    pub xdebug_enabled: bool,
    pub headless: bool,
    pub frontend_framework: Option<String>,
    /// MinIO (S3 local) compartido on-demand para este proyecto.
    #[serde(default)]
    pub minio: bool,
    #[serde(default)]
    pub migration_pending: bool,
    #[serde(default)]
    pub last_migrated_at: Option<String>,
}

impl SiteConfig {
    /// Nombre del container php-fpm del proyecto: `wp-{id}`.
    pub fn container_name(&self) -> String {
        format!("wp-{}", self.id)
    }

    pub fn public_dir(&self) -> PathBuf {
        Path::new(&self.path).join("app").join("public")
    }

    pub fn php_ini(&self) -> PathBuf {
        Path::new(&self.path).join("conf").join("php").join("php.ini")
    }

    pub fn sql_dir(&self) -> PathBuf {
        Path::new(&self.path).join("app").join("sql")
    }
}

// ---------------------------------------------------------------------------
// Punto de publicación del panel (global, no por proyecto)
// ---------------------------------------------------------------------------

/// Dónde publica `panel-nginx` en el host. Se elige UNA vez (autodetección de
/// puertos libres) y se persiste, porque WordPress guarda el `siteurl` con
/// puerto: cambiarlo después rompería los sitios ya instalados.
///
/// - Normal: `127.0.0.1:80/443` (URLs limpias `sitio.test`).
/// - Conflicto por IP concreta: otra IP loopback en 80/443 (sigue limpio).
/// - Conflicto wildcard (LocalWP en `0.0.0.0:80`): `127.0.0.1` con puerto
///   alterno (`sitio.test:8080`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub loopback_ip: String,
    pub http_port: u16,
    pub https_port: u16,
}

impl Default for Endpoint {
    fn default() -> Self {
        Endpoint {
            loopback_ip: "127.0.0.1".to_string(),
            http_port: 80,
            https_port: 443,
        }
    }
}

impl Endpoint {
    /// URL pública del sitio (con puerto solo si no es el estándar del esquema).
    pub fn site_url(&self, domain: &str, ssl: bool) -> String {
        if ssl {
            if self.https_port == 443 {
                format!("https://{domain}")
            } else {
                format!("https://{domain}:{}", self.https_port)
            }
        } else if self.http_port == 80 {
            format!("http://{domain}")
        } else {
            format!("http://{domain}:{}", self.http_port)
        }
    }

    /// ¿Es la configuración por defecto (127.0.0.1:80/443, URLs limpias)?
    #[allow(dead_code)] // usado por la UI de estado del panel (Fase 4)
    pub fn is_default(&self) -> bool {
        *self == Endpoint::default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelConfig {
    #[serde(default)]
    pub endpoint: Option<Endpoint>,
}

/// `~/.config/wordpress-panel/panel.json` (estado global del panel).
pub fn panel_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("panel.json"))
}

pub fn load_panel_config() -> Result<PanelConfig> {
    let path = panel_config_path()?;
    if !path.exists() {
        return Ok(PanelConfig::default());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("leyendo {:?}", path))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_panel_config(cfg: &PanelConfig) -> Result<()> {
    let path = panel_config_path()?;
    let raw = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, raw).with_context(|| format!("escribiendo {:?}", path))?;
    Ok(())
}

/// Endpoint ya elegido y persistido (si lo hay).
pub fn load_endpoint() -> Result<Option<Endpoint>> {
    Ok(load_panel_config()?.endpoint)
}

/// Endpoint efectivo para construir URLs; por defecto si aún no se eligió.
pub fn endpoint_or_default() -> Endpoint {
    load_endpoint().ok().flatten().unwrap_or_default()
}

pub fn save_endpoint(ep: &Endpoint) -> Result<()> {
    let mut cfg = load_panel_config()?;
    cfg.endpoint = Some(ep.clone());
    save_panel_config(&cfg)
}

/// Olvida el endpoint persistido: la próxima vez que arranque `panel-nginx` se
/// vuelve a autodetectar un puerto libre. Solo afecta a sitios creados después
/// (los ya instalados guardan el `siteurl` con el puerto anterior).
pub fn clear_endpoint() -> Result<()> {
    let mut cfg = load_panel_config()?;
    cfg.endpoint = None;
    save_panel_config(&cfg)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SiteStatus {
    Running,
    Stopped,
    MigrationPending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteState {
    pub config: SiteConfig,
    pub status: SiteStatus,
}

// ---------------------------------------------------------------------------
// Rutas
// ---------------------------------------------------------------------------

/// `~/.config/wordpress-panel/`
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("no se pudo determinar el directorio de configuración")?
        .join("wordpress-panel");
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

/// Raíz de proyectos: `~/panel-wp/`
pub fn projects_root() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("no se pudo determinar el home del usuario")?
        .join("panel-wp");
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Carga / guardado
// ---------------------------------------------------------------------------

/// Escanea `~/panel-wp/*/config.json` y devuelve todas las configuraciones.
pub fn load_all_sites() -> Result<Vec<SiteConfig>> {
    let root = projects_root()?;
    let mut sites = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let cfg = entry.path().join("config.json");
        if cfg.exists() {
            match read_site_config(&cfg) {
                Ok(s) => sites.push(s),
                Err(e) => eprintln!("config.json inválido en {:?}: {e}", cfg),
            }
        }
    }
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sites)
}

pub fn read_site_config(path: &Path) -> Result<SiteConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("leyendo {:?}", path))?;
    let cfg: SiteConfig = serde_json::from_str(&raw)
        .with_context(|| format!("parseando {:?}", path))?;
    Ok(cfg)
}

pub fn write_site_config(cfg: &SiteConfig) -> Result<()> {
    let path = Path::new(&cfg.path).join("config.json");
    let raw = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, raw).with_context(|| format!("escribiendo {:?}", path))?;
    Ok(())
}

pub fn find_site(id: &str) -> Result<Option<SiteConfig>> {
    Ok(load_all_sites()?.into_iter().find(|s| s.id == id))
}
