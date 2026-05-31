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
