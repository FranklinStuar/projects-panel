//! Tests de integración (Pieza 1b del plan de testing).
//!
//! Viven dentro del crate (no en `tests/`) para poder usar los módulos privados
//! (`config`, `docker`, `wordpress`, `migrate`, `localwp`). Todos van marcados
//! `#[ignore]`: NO corren en `cargo test` (rápido, sin Docker), solo bajo
//!
//! ```text
//! cargo test -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` es obligatorio: varios de estos tests redirigen variables
//! de entorno del proceso (`HOME`, `XDG_CONFIG_HOME`) o tocan infraestructura
//! Docker compartida, y correrlos en paralelo se pisaría.
//!
//! ## Prerequisitos de los tests con Docker
//! - Docker corriendo y accesible para el usuario.
//! - Red `panel-net` (la crea `ensure_network`, o `bash scripts/first-run.sh`).
//! - Salida a internet (descargan el core de WordPress y, la 1ª vez, construyen
//!   la imagen `panel-php`).
//!
//! El test de import desde LocalVP es **hermético** (no usa Docker): monta un
//! `HOME` temporal con un `sites.json` y una carpeta de sitio falsos.

use crate::config::{
    self, DbService, DbType, GithubConfig, NginxService, PhpService, Services, SiteConfig,
};
use crate::docker::DockerManager;
use crate::wordpress::{self, NewSiteRequest};

/// AppHandle simulado para los flujos que emiten progreso (`migrate`, `import`).
/// Devuelve el `App` (hay que mantenerlo vivo) — usar `app.handle()`.
fn mock() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_app()
}

/// Nombre de proyecto único para no chocar con proyectos reales del usuario.
fn scratch_name() -> String {
    format!("zztest-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

/// Borra container + carpeta de un proyecto de prueba (best-effort).
async fn teardown(docker: &DockerManager, site: &SiteConfig) {
    docker.stop_site(site, &[]).await.ok();
    docker.remove_container(&site.container_name()).await.ok();
    std::fs::remove_dir_all(&site.path).ok();
}

/// Migración end-to-end de un sitio LocalWP **real** del usuario.
///
/// Valida el camino que se rompe de forma recurrente: que el dump de un sitio
/// LocalWP grande se importe de verdad a la DB del proyecto (el `docker exec -i
/// … mysql` por CLI en `migrate::import_dump`, porque el exec con stdin de
/// bollard se cuelga con dumps grandes).
///
/// Opt-in por entorno — se **salta** si no defines el ID del sitio LocalWP a
/// migrar (así es commiteable y no depende de la máquina de nadie):
///
/// ```text
/// PANEL_TEST_LOCALWP_ID=ulNchSyst \
///   cargo test -- --ignored --exact integration_tests::migra_localwp_real
/// ```
///
/// El ID es la clave del sitio en `~/.config/Local/sites.json`.
#[tokio::test]
#[ignore = "real: migra un sitio LocalWP; requiere PANEL_TEST_LOCALWP_ID"]
async fn migra_localwp_real() {
    let Ok(localwp_id) = std::env::var("PANEL_TEST_LOCALWP_ID") else {
        eprintln!("SKIP: define PANEL_TEST_LOCALWP_ID con la clave del sitio en sites.json");
        return;
    };

    let app = mock();
    // Importa el sitio real (lee ~/.config/Local/sites.json; copia archivos+dump).
    let imp = crate::localwp::import_site(app.handle(), &localwp_id)
        .unwrap_or_else(|e| panic!("import {localwp_id}: {e}"));
    let site = imp.site.clone();
    eprintln!("IMPORTADO: {} → {} (pending={})", site.name, site.domain, site.migration_pending);
    assert!(site.migration_pending);

    let docker = DockerManager::connect().expect("docker");
    docker.ensure_network().await.expect("panel-net");

    let mig = crate::migrate::migrate_site(app.handle(), &docker, &site)
        .await
        .expect("migrate");
    eprintln!("MIGRADO: pending={} note={:?}", mig.site.migration_pending, mig.note);
    assert!(!mig.site.migration_pending);
    assert!(docker.is_running(&site.container_name()).await, "wp-{} arriba", site.id);

    // El corazón del test: el dump entró. Cuenta tablas en la DB del proyecto y
    // exige al menos una (una migración silenciosamente vacía es el bug a cazar).
    let db = crate::docker::db_container_name(&site.services.db);
    let out = docker
        .exec(&db, vec!["mysql", "-uroot", "-ppanel", "-N", "-e",
            &format!("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='{}'", site.services.db.db_name)])
        .await
        .unwrap_or_default();
    let tablas: u64 = out.trim().parse().unwrap_or(0);
    eprintln!("TABLAS en {}: {}", site.services.db.db_name, tablas);

    // Limpieza antes del assert para no dejar basura si falla.
    teardown(&docker, &site).await;
    let _ = docker
        .exec(&db, vec!["mysql", "-uroot", "-ppanel", "-e",
            &format!("DROP DATABASE IF EXISTS `{}`", site.services.db.db_name)])
        .await;
    eprintln!("LIMPIO");

    assert!(tablas > 0, "el dump no importó ninguna tabla a {}", site.services.db.db_name);
}

fn req_para(nombre: &str) -> NewSiteRequest {
    NewSiteRequest {
        name: nombre.to_string(),
        domain: None,
        wp_version: "latest".to_string(),
        locale: "en_US".to_string(),
        php_version: "8.3".to_string(),
        db_type: DbType::Mysql,
        db_version: "8.0".to_string(),
        admin_user: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_email: "admin@test.test".to_string(),
        title: "Test".to_string(),
        ssl: false, // evita depender de la CA de mkcert en el test
        one_click_admin: false,
        xdebug: false,
        headless: false,
        frontend_framework: None,
        minio: false,
        group: Some("zztest".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Hermético (sin Docker): importar desde LocalWP
// ---------------------------------------------------------------------------

#[test]
#[ignore = "muta HOME/XDG; correr con --ignored --test-threads=1"]
fn import_localwp_hermetico() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let xdg = home.join(".config");
    std::fs::create_dir_all(&xdg).unwrap();

    // Sitio LocalWP falso: app/public + app/sql/local.sql.
    let lw = home.join("Local Sites").join("Mi Sitio");
    let public = lw.join("app").join("public");
    std::fs::create_dir_all(&public).unwrap();
    std::fs::write(public.join("index.php"), "<?php // fake wp\n").unwrap();
    let sql = lw.join("app").join("sql");
    std::fs::create_dir_all(&sql).unwrap();
    std::fs::write(sql.join("local.sql"), "-- dump\nSELECT 1;\n").unwrap();

    // sites.json (formato Flywheel/Local): mapa por id.
    let sites_json = serde_json::json!({
        "ABC123": {
            "name": "Mi Sitio",
            "path": lw.to_string_lossy(),
            "services": { "php": { "version": "8.4.10" }, "mysql": { "version": "8.0.35" } },
            "multiSite": "",
            "xdebugEnabled": false
        }
    });
    let local_dir = xdg.join("Local");
    std::fs::create_dir_all(&local_dir).unwrap();
    std::fs::write(
        local_dir.join("sites.json"),
        serde_json::to_string_pretty(&sites_json).unwrap(),
    )
    .unwrap();

    // Redirigir el entorno a los temporales (dirs:: respeta HOME/XDG_CONFIG_HOME).
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", &xdg);

    let app = mock();
    let result = crate::localwp::import_site(app.handle(), "ABC123").expect("import");

    let site = &result.site;
    assert!(site.migration_pending, "debe quedar migrationPending");
    assert_eq!(site.domain, "mi-sitio.test", "dominio derivado del slug .test");
    assert_eq!(site.services.php.version, "8.4"); // 8.4.10 → 8.4 (soportada)

    // El proyecto se creó bajo {HOME}/panel-wp/{slug}.
    let proj = home.join("panel-wp").join("mi-sitio");
    assert!(proj.join("config.json").exists(), "config.json creado");
    assert!(
        proj.join("app/public/index.php").exists(),
        "app/public copiado"
    );
    assert!(
        proj.join("app/sql/imported.sql").exists(),
        "dump copiado como imported.sql"
    );
}

// ---------------------------------------------------------------------------
// Hermético (sin Docker): listar e importar proyectos desconectados
// ---------------------------------------------------------------------------

#[test]
#[ignore = "muta HOME/XDG; correr con --ignored --test-threads=1"]
fn list_e_import_disconnected_hermetico() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let xdg = home.join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", &xdg);

    let root = home.join("panel-wp");

    // (a) Proyecto vivo: tiene config.json → debe quedar EXCLUIDO del listado.
    let vivo = root.join("vivo");
    std::fs::create_dir_all(vivo.join("app").join("public")).unwrap();
    std::fs::write(
        vivo.join("config.json"),
        serde_json::to_string_pretty(&site_config("vivo-id", "Vivo", &vivo)).unwrap(),
    )
    .unwrap();

    // (b) Desconectado con config conservada + dump → preserved, hasDump=true.
    let pres = root.join("preservado");
    std::fs::create_dir_all(pres.join("app").join("public")).unwrap();
    std::fs::create_dir_all(pres.join("app").join("sql")).unwrap();
    std::fs::write(pres.join("app").join("sql").join("db-1.sql"), "-- dump\n").unwrap();
    std::fs::write(
        pres.join(config::DISCONNECTED_CONFIG),
        serde_json::to_string_pretty(&site_config("pres-id", "Cliente Preservado", &pres)).unwrap(),
    )
    .unwrap();

    // (c) Carpeta vieja sin config, con wp-config.php → reconstructed.
    let recon = root.join("reconstruido");
    let pub_c = recon.join("app").join("public");
    std::fs::create_dir_all(&pub_c).unwrap();
    std::fs::write(
        pub_c.join("wp-config.php"),
        "<?php\ndefine( 'DB_NAME', 'mi_db_legacy' );\n",
    )
    .unwrap();

    // -- listar --
    let list = config::list_disconnected_sites().expect("list");
    assert_eq!(list.len(), 2, "vivo excluido; preservado + reconstruido listados");
    let pres_item = list.iter().find(|d| d.folder_name == "preservado").expect("preservado");
    assert_eq!(pres_item.kind, "preserved");
    assert!(pres_item.has_dump, "preservado tiene dump");
    assert_eq!(pres_item.name, "Cliente Preservado");
    let recon_item = list.iter().find(|d| d.folder_name == "reconstruido").expect("reconstruido");
    assert_eq!(recon_item.kind, "reconstructed");
    assert!(!recon_item.has_dump);

    // -- importar el preservado --
    let app = mock();
    let res = crate::import_disconnected(app.handle(), "preservado").expect("import");
    assert!(res.site.migration_pending, "queda pendiente de migración");
    assert_eq!(res.site.id, "pres-id", "id conservado (no colisiona)");
    assert!(pres.join("config.json").exists(), "config.json restaurado");
    assert!(!pres.join(config::DISCONNECTED_CONFIG).exists(), "sidecar eliminado");

    // -- importar el reconstruido: dbName deducido de wp-config.php --
    let res2 = crate::import_disconnected(app.handle(), "reconstruido").expect("import recon");
    assert!(res2.site.migration_pending);
    assert_eq!(res2.site.services.db.db_name, "mi_db_legacy");
    assert!(recon.join("config.json").exists());
}

/// SiteConfig mínimo para fixtures de los tests herméticos.
fn site_config(id: &str, name: &str, path: &std::path::Path) -> SiteConfig {
    SiteConfig {
        id: id.into(),
        name: name.into(),
        path: path.to_string_lossy().into_owned(),
        domain: format!("{}.test", id),
        group: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        services: Services {
            php: PhpService { version: "8.3".into() },
            nginx: NginxService { ssl: true },
            db: DbService {
                db_type: DbType::Mysql,
                version: "8.0".into(),
                db_name: format!("{}_db", id.replace('-', "_")),
            },
        },
        github: GithubConfig::default(),
        one_click_admin: true,
        xdebug_enabled: false,
        headless: false,
        frontend_framework: None,
        minio: false,
        migration_pending: false,
        last_migrated_at: None,
    }
}

// ---------------------------------------------------------------------------
// Con Docker
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requiere Docker; correr con --ignored --test-threads=1"]
async fn db_lifecycle_idempotente() {
    let docker = DockerManager::connect().expect("conectar a Docker");
    docker.ensure_network().await.expect("panel-net");

    // SiteConfig mínimo solo para crear su base de datos (sin tocar disco/WP).
    let slug = scratch_name();
    let cfg = SiteConfig {
        id: slug.clone(),
        name: slug.clone(),
        path: config::projects_root().unwrap().join(&slug).to_string_lossy().into_owned(),
        domain: format!("{slug}.test"),
        group: Some("zztest".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
        services: Services {
            php: PhpService { version: "8.3".into() },
            nginx: NginxService { ssl: false },
            db: DbService {
                db_type: DbType::Mysql,
                version: "8.0".into(),
                db_name: format!("{}_db", slug.replace('-', "_")),
            },
        },
        github: GithubConfig::default(),
        one_click_admin: false,
        xdebug_enabled: false,
        headless: false,
        frontend_framework: None,
        minio: false,
        migration_pending: false,
        last_migrated_at: None,
    };

    let db_container = docker.ensure_db(&cfg.services.db).await.expect("ensure_db");
    assert!(docker.is_running(&db_container).await, "DB compartida arriba");

    // create_database dos veces = idempotente (no debe fallar).
    wordpress::create_database(&docker, &db_container, &cfg)
        .await
        .expect("create_database 1");
    wordpress::create_database(&docker, &db_container, &cfg)
        .await
        .expect("create_database 2 (idempotente)");

    // No tocamos la DB compartida (infra del panel); nada que limpiar.
}

#[tokio::test]
#[ignore = "e2e pesado: descarga WordPress y construye imagen; --ignored --test-threads=1"]
async fn crear_exportar_migrar_e2e() {
    let docker = DockerManager::connect().expect("conectar a Docker");
    docker.ensure_network().await.expect("panel-net");

    let site = wordpress::create_site(&docker, req_para(&scratch_name()))
        .await
        .expect("create_site");

    // export_db deja un dump en app/sql/db-*.sql.
    crate::backup::export_db(&docker, &site).await.expect("export_db");
    let n = std::fs::read_dir(site.sql_dir())
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("db-") && n.ends_with(".sql")
        })
        .count();
    assert!(n >= 1, "debe existir al menos un dump db-*.sql");

    crate::backup::rotate_dumps(&site, 3).expect("rotate_dumps");

    // migrate_site reprovisiona sobre el mismo sitio (idempotente): reimporta el
    // último dump y deja migration_pending=false.
    let app = mock();
    let mut pendiente = site.clone();
    pendiente.migration_pending = true;
    let mig = crate::migrate::migrate_site(app.handle(), &docker, &pendiente)
        .await
        .expect("migrate_site");
    assert!(!mig.site.migration_pending, "tras migrar: no pendiente");

    teardown(&docker, &site).await;
}
