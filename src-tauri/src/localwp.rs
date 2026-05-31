//! Importación de proyectos desde LocalWP.
//!
//! Lee `~/.config/Local/sites.json` (formato Flywheel/Local), lista los sitios y
//! crea un proyecto del panel por cada uno: copia `app/public`, copia el dump
//! `app/sql/local.sql` como `imported.sql` y escribe un `config.json` marcado
//! `migrationPending=true`. La base de datos se materializa después con "Migrar
//! y encender" (`migrate.rs`), que importa ese dump y repunta `siteurl`.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;

use crate::config::{
    write_site_config, DbService, DbType, GithubConfig, NginxService, PhpService, Services,
    SiteConfig,
};
use crate::progress::log;

/// Versiones soportadas por el panel (ver `PLAN.md`). Si la de LocalWP no está,
/// se usa la más reciente y se avisa.
const PHP_SUPPORTED: &[&str] = &["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"];
const MYSQL_SUPPORTED: &[&str] = &["8.0", "8.4"];

// -- parseo tolerante de sites.json -----------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSite {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    services: RawServices,
    #[serde(default)]
    multi_site: String,
    #[serde(default)]
    xdebug_enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RawServices {
    #[serde(default)]
    php: RawVersioned,
    #[serde(default)]
    mysql: RawVersioned,
}

#[derive(Debug, Default, Deserialize)]
struct RawVersioned {
    #[serde(default)]
    version: String,
}

// -- modelos expuestos -------------------------------------------------------

/// Un sitio de LocalWP candidato a importar (espejo en `types.ts`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSite {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub path: String,
    pub php_version: String,
    pub db_version: String,
    pub multisite: bool,
    pub xdebug: bool,
    pub already_imported: bool,
}

/// Resultado de importar: la config creada + un aviso opcional (sin dump,
/// versión ajustada, etc.).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub site: SiteConfig,
    pub note: Option<String>,
}

/// `~/.config/Local/sites.json`.
fn sites_json() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("no se pudo determinar el directorio de configuración")?
        .join("Local")
        .join("sites.json"))
}

fn read_raw() -> Result<HashMap<String, RawSite>> {
    let path = sites_json()?;
    if !path.exists() {
        return Err(anyhow!(
            "no se encontró {:?} (¿LocalWP instalado en este sistema?)",
            path
        ));
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("leyendo {:?}", path))?;
    serde_json::from_str(&raw).context("parseando sites.json de LocalWP")
}

/// Lista los sitios de LocalWP, marcando los ya importados al panel.
pub fn list_sites() -> Result<Vec<LocalSite>> {
    let map = read_raw()?;
    let existing = crate::config::load_all_sites().unwrap_or_default();

    let mut out: Vec<LocalSite> = map
        .into_iter()
        .map(|(id, r)| {
            let slug = crate::wordpress::slugify(&r.name);
            let domain = format!("{slug}.test");
            let already = existing
                .iter()
                .any(|s| s.domain == domain || s.name == r.name);
            LocalSite {
                id,
                name: r.name,
                domain,
                path: expand_tilde(&r.path),
                php_version: major_minor(&r.services.php.version),
                db_version: major_minor(&r.services.mysql.version),
                multisite: !r.multi_site.is_empty(),
                xdebug: r.xdebug_enabled,
                already_imported: already,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Importa un sitio de LocalWP creando un proyecto del panel (sin DB todavía).
pub fn import_site(app: &AppHandle, local_id: &str) -> Result<ImportResult> {
    let map = read_raw()?;
    let r = map
        .get(local_id)
        .ok_or_else(|| anyhow!("sitio LocalWP no encontrado: {local_id}"))?;
    log(app, format!("▶ Importando «{}» desde LocalWP…", r.name));

    let src = PathBuf::from(expand_tilde(&r.path));
    let src_public = src.join("app").join("public");
    if !src_public.exists() {
        return Err(anyhow!("no existe {:?} en LocalWP", src_public));
    }

    let slug = crate::wordpress::slugify(&r.name);
    let dest = crate::config::projects_root()?.join(&slug);
    if dest.exists() {
        return Err(anyhow!("ya existe un proyecto en {:?}", dest));
    }

    let (php_version, php_adjusted) =
        pick_supported(&major_minor(&r.services.php.version), PHP_SUPPORTED);
    let (db_version, db_adjusted) =
        pick_supported(&major_minor(&r.services.mysql.version), MYSQL_SUPPORTED);

    let site = SiteConfig {
        id: Uuid::new_v4().to_string(),
        name: r.name.clone(),
        path: dest.to_string_lossy().to_string(),
        domain: format!("{slug}.test"),
        group: Some("LocalWP".to_string()),
        created_at: Utc::now().to_rfc3339(),
        services: Services {
            php: PhpService {
                version: php_version,
            },
            nginx: NginxService { ssl: true },
            db: DbService {
                db_type: DbType::Mysql,
                version: db_version,
                db_name: format!("{}_db", slug.replace('-', "_")),
            },
        },
        github: GithubConfig::default(),
        one_click_admin: true,
        xdebug_enabled: r.xdebug_enabled,
        headless: false,
        frontend_framework: None,
        minio: false,
        // Aparece pendiente: la DB se crea/importa al "Migrar y encender".
        migration_pending: true,
        last_migrated_at: None,
    };

    // Estructura + copia de archivos.
    crate::wordpress::create_dirs(&site)?;
    crate::wordpress::write_php_ini(&site)?;
    log(app, "• Copiando archivos (app/public, puede tardar)…");
    cp_contents(&src_public, &site.public_dir())?;

    // Dump: LocalWP guarda `app/sql/local.sql`.
    let mut note = String::new();
    let src_sql = src.join("app").join("sql").join("local.sql");
    if src_sql.exists() {
        let mb = std::fs::metadata(&src_sql).map(|m| m.len() / 1_048_576).unwrap_or(0);
        log(app, format!("• Copiando dump de la base de datos ({mb} MB)…"));
        std::fs::copy(&src_sql, site.sql_dir().join("imported.sql"))
            .context("copiando el dump de LocalWP")?;
    } else {
        note.push_str(
            "No se encontró app/sql/local.sql en LocalWP: exporta la DB desde LocalWP antes de migrar. ",
        );
    }
    if php_adjusted {
        note.push_str(&format!(
            "PHP {} no soportada → usando {}. ",
            major_minor(&r.services.php.version),
            site.services.php.version
        ));
    }
    if db_adjusted {
        note.push_str(&format!(
            "MySQL {} no soportada → usando {}. ",
            major_minor(&r.services.mysql.version),
            site.services.db.version
        ));
    }
    if !r.multi_site.is_empty() {
        note.push_str("Es multisite: revisa la configuración tras migrar. ");
    }

    write_site_config(&site)?;

    log(
        app,
        format!("✓ «{}» importado → usa «Migrar y encender» en Proyectos.", site.name),
    );
    Ok(ImportResult {
        site,
        note: (!note.is_empty()).then_some(note.trim().to_string()),
    })
}

// -- helpers -----------------------------------------------------------------

/// Copia el *contenido* de `src` dentro de `dest` con `cp -a` (preserva
/// atributos; más rápido que recorrer en Rust un árbol grande de WordPress).
fn cp_contents(src: &Path, dest: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", src.display()))
        .arg(dest)
        .status()
        .context("copiando app/public desde LocalWP (cp -a)")?;
    if !status.success() {
        return Err(anyhow!("cp -a falló copiando {:?}", src));
    }
    Ok(())
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

/// `8.4.10` → `8.4`.
fn major_minor(v: &str) -> String {
    let mut it = v.split('.');
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

/// Devuelve `(version, ajustada)`: la misma si está soportada, o la más reciente
/// soportada (con `true`) si no.
fn pick_supported(v: &str, supported: &[&str]) -> (String, bool) {
    if supported.contains(&v) {
        (v.to_string(), false)
    } else {
        (supported.last().unwrap().to_string(), true)
    }
}
