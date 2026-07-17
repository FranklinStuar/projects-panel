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

    /// Directorio de datos DENTRO del container. Lo bindeamos a un dir del host
    /// (`config_dir/db-data/{container}`) para que los datos sobrevivan al
    /// recreado del container y a un apagón (almacenamiento durable).
    pub fn datadir(&self) -> &'static str {
        match self {
            DbType::Mysql | DbType::Mariadb => "/var/lib/mysql",
            DbType::Postgres => "/var/lib/postgresql/data",
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
#[serde(rename_all = "camelCase")]
pub struct CloneInfo {
    pub parent_id: String,
    /// Basename de `path` del padre — nginx.rs lo usa para la ruta de uploads.
    pub parent_dirname: String,
    pub snapshot_id: String,
    pub created_at: String,
}

/// Poblado si este sitio es un *worktree-project*: un proyecto ligero atado a un
/// repo (theme/plugin) del padre, montado para probar una rama en aislamiento.
/// El `public` del padre se comparte por montaje Docker; solo el repo objetivo
/// (un `git worktree` sobre `branch`) y el `wp-config.php` propio se sobreponen.
/// Ver `worktree.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub parent_id: String,
    /// Basename del `path` del padre — nginx/docker lo usan para las rutas montadas.
    pub parent_dirname: String,
    /// Ruta del repo objetivo relativa a `public/` (ej. `wp-content/themes/mi-theme`).
    pub target_path: String,
    /// Rama del worktree (se crea nueva o se reusa una existente).
    pub branch: String,
    /// `true` = comparte el esquema DB del padre (constantes `WP_HOME`/`WP_SITEURL`
    /// en el wp-config propio, sin mutar la DB); `false` = esquema propio copiado.
    pub shared_db: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepo {
    pub repo: String,
    pub branch: String,
    pub path: String,
    /// Comando de build a ejecutar en el host (login shell) tras un pull/deploy,
    /// en la carpeta del repo. `None`/vacío = no ejecutar nada. Se usa para el
    /// deploy directo desde el panel (staging sin servidor dedicado).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_cmd: Option<String>,
    /// Carpetas (relativas al repo) donde correr `build_cmd`. Vacío = raíz del
    /// repo. Permite proyectos cuyo build vive en `/src`, `/src-redesign`, o en
    /// varias a la vez (se ejecuta el comando en cada una).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubConfig {
    /// Lista genérica de repos del proyecto (en cualquier ruta bajo public/).
    /// Cada repo puede o no tener remoto en GitHub.
    #[serde(default)]
    pub repos: Vec<GithubRepo>,
    /// Legacy: antes había un único theme y una lista de plugins. Se conservan
    /// solo para leer config.json antiguos; `normalize()` los pliega en `repos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<GithubRepo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<GithubRepo>,
}

impl GithubConfig {
    /// Migra los campos legacy (`theme`, `plugins`) a la lista genérica `repos`
    /// y los vacía. Idempotente: si no hay legacy, no hace nada.
    pub fn normalize(&mut self) {
        let legacy: Vec<GithubRepo> = self
            .theme
            .take()
            .into_iter()
            .chain(std::mem::take(&mut self.plugins))
            .collect();
        for r in legacy {
            if !self.repos.iter().any(|e| e.path == r.path) {
                self.repos.push(r);
            }
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
    /// Poblado si este sitio es un clone temporal de otro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_of: Option<CloneInfo>,
    /// Poblado si este sitio es un worktree-project (ver `WorktreeInfo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_of: Option<WorktreeInfo>,
    /// Rutas (relativas a `public_dir`) a excluir del tar del punto de guardado,
    /// además de las exclusiones fijas (uploads, cache, wp-config, *.log).
    /// P. ej. `wp-content/updraft`, `wp-content/ai1wm-backups`.
    #[serde(default)]
    pub snapshot_excludes: Vec<String>,
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

    /// Carpeta del host que alberga el(los) `git worktree` de un worktree-project
    /// (`{path}/wt/`). Cada repo objetivo se chequea en `wt/{basename}`.
    pub fn worktree_root(&self) -> PathBuf {
        Path::new(&self.path).join("wt")
    }

    /// `wp-config.php` propio del worktree-project (se monta sobre el del padre).
    pub fn worktree_wp_config(&self) -> PathBuf {
        Path::new(&self.path).join("wp-config.php")
    }
}

/// Último segmento de una ruta relativa estilo `wp-content/themes/mi-theme` →
/// `mi-theme`. Lo usan docker/nginx para situar el `git worktree` del objetivo.
pub fn path_basename(rel: &str) -> &str {
    rel.trim_end_matches('/').rsplit('/').next().unwrap_or(rel)
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
    let mut cfg: SiteConfig = serde_json::from_str(&raw)
        .with_context(|| format!("parseando {:?}", path))?;
    cfg.github.normalize();
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

// ---------------------------------------------------------------------------
// Proyectos desconectados (carpeta conservada, fuera del panel)
// ---------------------------------------------------------------------------

/// Sidecar donde se guarda la config al desconectar un proyecto (borrar
/// conservando la carpeta). Mientras exista este archivo —y NO `config.json`—
/// la carpeta está «desconectada»: `load_all_sites()` la ignora pero se puede
/// re-importar sin pérdida restaurando la config.
pub(crate) const DISCONNECTED_CONFIG: &str = "config.disconnected.json";

/// Versiones por defecto para carpetas viejas sin ninguna config (best-effort).
const DEFAULT_PHP: &str = "8.3";
const DEFAULT_DB: &str = "8.0";

pub fn disconnected_config_path(path: &str) -> PathBuf {
    Path::new(path).join(DISCONNECTED_CONFIG)
}

/// Una carpeta de `~/panel-wp/` que ya no está registrada en el panel pero que
/// sigue en disco, candidata a re-importar (espejo en `types.ts`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectedSite {
    pub folder_name: String,
    pub path: String,
    pub name: String,
    pub domain: String,
    pub php_version: String,
    pub db_version: String,
    pub db_type: String,
    /// Hay al menos un `*.sql` en `app/sql/` (dump restaurable al migrar).
    pub has_dump: bool,
    /// `preserved` = tenía `config.disconnected.json`; `reconstructed` = carpeta
    /// vieja sin config, datos deducidos best-effort.
    pub kind: String,
}

/// Escanea `~/panel-wp/` y devuelve las carpetas que NO están en el panel
/// (sin `config.json`) pero que sí son proyectos del panel: con un sidecar
/// `config.disconnected.json` (preserved) o, en su defecto, con
/// `app/public/wp-config.php` (reconstructed).
pub fn list_disconnected_sites() -> Result<Vec<DisconnectedSite>> {
    let root = projects_root()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        if dir.join("config.json").exists() {
            continue; // sigue conectado al panel
        }
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        let path = dir.to_string_lossy().into_owned();
        let has_dump = dir_has_sql(&dir.join("app").join("sql"));

        let sidecar = dir.join(DISCONNECTED_CONFIG);
        if sidecar.exists() {
            if let Ok(cfg) = read_site_config(&sidecar) {
                out.push(DisconnectedSite {
                    folder_name,
                    path,
                    name: cfg.name,
                    domain: cfg.domain,
                    php_version: cfg.services.php.version,
                    db_version: cfg.services.db.version,
                    db_type: db_type_str(cfg.services.db.db_type),
                    has_dump,
                    kind: "preserved".into(),
                });
                continue;
            }
        }

        // Sin sidecar: solo cuenta si parece un WordPress (tiene wp-config.php).
        // El `dbName` se deduce al importar (lib.rs re-parsea wp-config.php).
        let wp_config = dir.join("app").join("public").join("wp-config.php");
        if wp_config.exists() {
            out.push(DisconnectedSite {
                name: folder_name.clone(),
                domain: format!("{folder_name}.test"),
                folder_name,
                path,
                php_version: DEFAULT_PHP.into(),
                db_version: DEFAULT_DB.into(),
                db_type: "mysql".into(),
                has_dump,
                kind: "reconstructed".into(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// ¿Hay algún `*.sql` en `dir`? (criterio de `migrate::latest_dump`.)
fn dir_has_sql(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.path().extension().and_then(|x| x.to_str()) == Some("sql")
            })
        })
        .unwrap_or(false)
}

fn db_type_str(t: DbType) -> String {
    match t {
        DbType::Mysql => "mysql",
        DbType::Mariadb => "mariadb",
        DbType::Postgres => "postgres",
    }
    .to_string()
}

/// Extrae `DB_NAME` de un `wp-config.php` (`define('DB_NAME', '…')`).
pub fn parse_db_name(wp_config: &str) -> Option<String> {
    for line in wp_config.lines() {
        let line = line.trim();
        if !line.contains("DB_NAME") || !line.contains("define") {
            continue;
        }
        // define( 'DB_NAME', 'valor' );  → segundo literal entre comillas.
        let mut parts = line.splitn(3, |c| c == '\'' || c == '"');
        let _before = parts.next()?; // define( DB_NAME → hasta la 1ª comilla
        let _db_name_token = parts.next()?; // DB_NAME
        let rest = parts.next()?; // ', 'valor' );
        let mut it = rest.splitn(3, |c| c == '\'' || c == '"');
        it.next()?; // ,
        let value = it.next()?; // valor
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> SiteConfig {
        SiteConfig {
            id: "abc123".into(),
            name: "Demo".into(),
            path: "/home/u/panel-wp/demo".into(),
            domain: "demo.test".into(),
            group: Some("LocalWP".into()),
            created_at: "2026-01-01T00:00:00Z".into(),
            services: Services {
                php: PhpService { version: "8.3".into() },
                nginx: NginxService { ssl: true },
                db: DbService {
                    db_type: DbType::Mysql,
                    version: "8.0".into(),
                    db_name: "demo".into(),
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

    #[test]
    fn container_name_y_sql_dir() {
        let s = site();
        assert_eq!(s.container_name(), "wp-abc123");
        assert_eq!(
            s.sql_dir(),
            std::path::Path::new("/home/u/panel-wp/demo/app/sql")
        );
    }

    #[test]
    fn site_url_cuatro_ramas() {
        let limpio = Endpoint::default(); // 80/443
        assert_eq!(limpio.site_url("demo.test", true), "https://demo.test");
        assert_eq!(limpio.site_url("demo.test", false), "http://demo.test");

        let alterno = Endpoint {
            loopback_ip: "127.0.0.1".into(),
            http_port: 8080,
            https_port: 8443,
        };
        assert_eq!(alterno.site_url("demo.test", true), "https://demo.test:8443");
        assert_eq!(alterno.site_url("demo.test", false), "http://demo.test:8080");
    }

    #[test]
    fn github_normalize_pliega_legacy_en_repos() {
        let mut gh = GithubConfig {
            repos: vec![],
            theme: Some(GithubRepo {
                repo: "me/theme".into(),
                branch: "main".into(),
                path: "wp-content/themes/t".into(),
                build_cmd: None,
                build_dirs: vec![],
            }),
            plugins: vec![GithubRepo {
                repo: "me/plug".into(),
                branch: "dev".into(),
                path: "wp-content/plugins/p".into(),
                build_cmd: None,
                build_dirs: vec![],
            }],
        };
        gh.normalize();
        assert_eq!(gh.repos.len(), 2);
        assert!(gh.theme.is_none());
        assert!(gh.plugins.is_empty());
        // idempotente y sin duplicar por path
        gh.normalize();
        assert_eq!(gh.repos.len(), 2);
        // legacy ya no se serializa
        let v = serde_json::to_value(&gh).unwrap();
        assert!(v.get("theme").is_none(), "theme no debe serializarse: {v}");
        assert!(v.get("plugins").is_none(), "plugins no debe serializarse: {v}");
    }

    #[test]
    fn clone_info_serializa_en_camelcase() {
        let info = CloneInfo {
            parent_id: "p1".into(),
            parent_dirname: "mysite".into(),
            snapshot_id: "s1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&info).unwrap();
        assert!(v.get("parentId").is_some(), "falta parentId: {v}");
        assert!(v.get("parentDirname").is_some(), "falta parentDirname: {v}");
        assert!(v.get("snapshotId").is_some(), "falta snapshotId: {v}");

        // clone_of=Some se serializa; clone_of=None se omite.
        let mut s = site();
        s.clone_of = Some(info);
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("cloneOf").is_some(), "falta cloneOf cuando Some: {v}");

        let s2 = site(); // clone_of=None
        let v2 = serde_json::to_value(&s2).unwrap();
        assert!(v2.get("cloneOf").is_none(), "cloneOf debe omitirse cuando None: {v2}");
    }

    #[test]
    fn endpoint_serializa_en_camelcase() {
        let v = serde_json::to_value(Endpoint::default()).unwrap();
        assert!(v.get("loopbackIp").is_some(), "falta loopbackIp: {v}");
        assert!(v.get("httpPort").is_some(), "falta httpPort: {v}");
        assert!(v.get("httpsPort").is_some(), "falta httpsPort: {v}");
    }

    #[test]
    fn siteconfig_roundtrip_camelcase() {
        let s = site();
        let v = serde_json::to_value(&s).unwrap();
        // Claves espejo de types.ts (camelCase + renames de DbService).
        for k in [
            "createdAt",
            "oneClickAdmin",
            "xdebugEnabled",
            "frontendFramework",
            "migrationPending",
            "lastMigratedAt",
        ] {
            assert!(v.get(k).is_some(), "falta clave {k} en {v}");
        }
        assert_eq!(v["services"]["db"]["type"], "mysql");
        assert!(v["services"]["db"].get("dbName").is_some());

        // Deserializa de vuelta sin pérdida.
        let back: SiteConfig = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, s.id);
        assert_eq!(back.domain, s.domain);
        assert!(back.migration_pending);
    }
}
