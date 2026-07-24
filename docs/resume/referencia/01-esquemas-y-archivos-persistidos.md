# 01 · Esquemas y archivos persistidos

> Referencia verificada contra el commit `373841c` (rama `main`, 2026-07-23).
> Cada artefacto lista su **ruta absoluta** en el host, su **productor**,
> su **consumidor** y la referencia simbólica (`ruta::funcion`) que lo crea o
> lo lee. Los nombres se mantienen tal cual aparecen en el código para que
> cualquiera pueda grepearlos.

## 0. Tabla maestra de artefactos

| Artefacto                                                    | Productor (símbolo)                                                              | Consumidor (símbolo)                                                                 | Borrado por                                                  |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| `~/panel-wp/{slug}/config.json`                              | `config::write_site_config`                                                       | `config::load_all_sites`, `load_site`, `find_site`, `read_site_config`               | `lib::delete_site` (rename a sidecar o `remove_dir_all`)      |
| `~/panel-wp/{slug}/config.disconnected.json`                 | `lib::delete_site` (rename)                                                      | `config::disconnected_config_path`, `config::list_disconnected_sites`, `lib::import_disconnected` | `lib::import_disconnected` (lo borra tras restaurar)         |
| `~/panel-wp/{slug}/app/public/`                              | `wordpress::create_site` (descarga core), `localwp::import_site` (copia), `clone::create_clone` (extrae snapshot). | `nginx::render_vhost`, `docker::create_php_container`, `wordpress::inject_*_muplugin`, `github::scan`. | `lib::delete_site` (si `delete_folder=true`)                 |
| `~/panel-wp/{slug}/app/sql/`                                 | `wordpress::create_dirs` (mkdir)                                                 | `backup::dump_bytes/export_db`, `migrate::latest_dump`, `autodump::persist`, `backup::rotate_dumps`, `dumplog::append`. | `backup::rotate_dumps` (conserva `keep=3` últimos `db-*.sql`). |
| `~/panel-wp/{slug}/app/sql/db-{stamp}.sql`                   | `backup::export_db`, `autodump::persist`                                          | `dumplog::append`, `migrate::latest_dump`, `backup::rotate_dumps`                     | `backup::rotate_dumps`                                       |
| `~/panel-wp/{slug}/app/sql/imported.sql`                     | `localwp::import_site` (copia de `app/sql/local.sql`)                             | `migrate::latest_dump`                                                               | Usuario (no se rota).                                         |
| `~/panel-wp/{slug}/app/sql/from-parent-{stamp}.sql`          | `worktree::create_worktree` (cuando `!shared_db`)                                 | `migrate::import_dump` (consume)                                                     | Usuario (no se rota).                                         |
| `~/panel-wp/{slug}/conf/php/php.ini`                         | `wordpress::write_php_ini`                                                        | `docker::create_php_container` (monta como `zz-project.ini`)                          | `lib::delete_site` (carpeta).                                 |
| `~/panel-wp/{slug}/ssl/{cert,key}.pem`                       | `ssl::generate`                                                                  | `nginx::render_vhost` (es el `ssl_certificate` del vhost)                             | `lib::delete_site` (carpeta).                                 |
| `~/panel-wp/{slug}/logs/php/`                                | `wordpress::create_dirs` (mkdir)                                                 | output php-fpm (bind), excluidos del snapshot                                         | `lib::delete_site` (carpeta).                                 |
| `~/panel-wp/{slug}/data/`                                    | `wordpress::create_dirs` (mkdir)                                                 | Reservado                                                                             | `lib::delete_site` (carpeta).                                 |
| `~/panel-wp/{slug}/wt/{basename}/`                           | `worktree::create_worktree`                                                       | `docker::create_php_container` (ramas con `worktree_of`), `nginx::render_vhost` (`alias`). | `worktree::remove_worktree`                                   |
| `~/panel-wp/{slug}/wp-config.php`                            | `worktree::create_worktree` (placeholder `<?php\n`), `wordpress::wp_config_create` (lo reescribe como `www-data`). | `docker::create_php_container` (sobreescrito sobre el del padre)                       | `worktree::remove_worktree`                                   |
| `~/panel-wp/{slug}/snapshots/{sid}/code.tar.zst`             | `snapshot::create_snapshot` (`tar --zstd -cf`)                                   | `clone::create_clone` (`tar --zstd -xf`)                                              | `snapshot::delete_snapshot`                                   |
| `~/panel-wp/{slug}/snapshots/{sid}/db.sql`                   | `snapshot::create_snapshot` (`backup::export_db_to`)                              | `clone::create_clone` (`migrate::import_dump`)                                        | `snapshot::delete_snapshot`                                   |
| `~/panel-wp/{slug}/snapshots/{sid}/meta.json`                | `snapshot::create_snapshot`                                                      | `clone::create_clone`, `snapshot::list_snapshots`                                     | `snapshot::delete_snapshot`                                   |
| `~/panel-wp/{slug}/{safe}.code-workspace`                    | `github::ensure_workspace`                                                       | `github::open_vscode` (`code`/`codium`/`code-insiders`/`vscodium`)                    | `lib::delete_site` (carpeta).                                 |
| `~/.config/wordpress-panel/panel.json`                       | `config::save_panel_config`                                                       | `config::load_panel_config`, `load_endpoint`, `clear_endpoint`                        | `config::clear_endpoint` (deja `endpoint: null`).             |
| `~/.config/wordpress-panel/groups.json`                      | `groups::write_file`                                                             | `groups::list`, `create`, `rename`, `delete`, `reorder`                              | `groups::delete` (subarreglo por nombre).                      |
| `~/.config/wordpress-panel/dump-log.jsonl`                   | `dumplog::append` (open + append)                                                | `dumplog::read_all`, `dumplog::clean`                                                  | `dumplog::clean` (reescribe sin las entradas eliminadas).     |
| `~/.config/wordpress-panel/wp-versions.json`                 | `wordpress::fetch_versions`                                                       | `wordpress::fetch_versions` (cache 24h)                                              | `wordpress::fetch_versions` (sobrescribe cuando refresca).    |
| `~/.config/wordpress-panel/db-data/{container}/`             | `docker::db_data_dir` (mkdir) + `docker::ensure_db` (bind al datadir interno)    | `docker::ensure_db` (mount), `docker::migrate_db_to_volume` (cp)                      | `lib::delete_site` borra la DB schema (`drop_database`) pero no el dir. |
| `~/.config/wordpress-panel/minio-data/`                      | `docker::ensure_minio` (mkdir)                                                   | `docker::ensure_minio` (bind `/data`)                                                 | — (no se borra).                                              |
| `~/.config/wordpress-panel/wp-cli.phar`                      | `php::wp_cli_phar_path` (descarga `reqwest::get`)                                 | `docker::create_php_container` (bind a `/usr/local/bin/wp:ro`)                       | — (no se borra).                                              |
| `~/.config/wordpress-panel/nginx/conf.d/{id}.conf`           | `nginx::write_vhost`                                                              | `panel-nginx` (montado ro en `/etc/nginx/conf.d`)                                     | `nginx::remove_vhost`                                         |
| `~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf`| `nginx::ensure_tuning`                                                            | `panel-nginx` (alta de `server_names_hash_bucket_size 128;`)                          | — (siempre presente).                                          |
| `~/.config/wordpress-panel/dnsmasq-panel.conf`               | `domain::ensure_wildcard`                                                         | `domain::install_wildcard` (lo copia a `/etc/NetworkManager/dnsmasq.d/`)              | — (no se borra).                                              |
| `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf`         | `domain::install_wildcard` (vía `pkexec`) y `scripts/first-run.sh`               | dnsmasq (NetworkManager)                                                              | `scripts/first-run.sh` (puede reescribir).                    |
| `~/.local/share/plasma/plasmoids/com.goldmediatech.wordpresspanel/` | `scripts/package-plasmoid.sh`                                              | Plasma                                                                                | — (no se borra).                                              |
| `~/.local/bin/wp`                                            | `cli::install_cli_wrapper` (`install_one("wp-wrapper.sh", "wp")`)                 | shell del usuario                                                                    | `cli::install_cli_wrapper` (sobrescribe).                     |
| `~/.local/bin/wordpress-panel-cli`                           | `cli::install_cli_wrapper` (`install_one("wordpress-panel-cli.sh", "wordpress-panel-cli")`) | shell del usuario, `mcp/server.mjs` (fallback)                                       | `cli::install_cli_wrapper` (sobrescribe).                     |
| `~/.config/Local/sites.json` (solo lectura)                  | LocalWP (externo)                                                                | `localwp::read_raw` (lee)                                                             | — (no se borra).                                              |
| `~/Local Sites/{site}/app/public` (solo lectura)             | LocalWP (externo)                                                                | `localwp::import_site` (copia)                                                        | — (no se borra).                                              |
| `~/Local Sites/{site}/app/sql/local.sql` (solo lectura)      | LocalWP (externo)                                                                | `localwp::import_site` (copia a `imported.sql`)                                       | — (no se borra).                                              |

## 1. `SiteConfig`: el registro de proyecto

Definido en `src-tauri/src/config.rs:164-198` (con `#[serde(rename_all = "camelCase")]`).
Espejo TS en `src/lib/types.ts:83-104`.

```rust
pub struct SiteConfig {
    pub id: String,                        // uuid v4
    pub name: String,                      // nombre humano
    pub path: String,                      // ~/panel-wp/{slug}
    pub domain: String,                    // p. ej. "mi-sitio.test"
    pub group: Option<String>,             // nombre de grupo (source-of-truth de pertenencia)
    pub created_at: String,                // RFC3339 UTC
    pub services: Services,                // php / nginx / db
    #[serde(default)] pub github: GithubConfig,
    pub one_click_admin: bool,
    pub xdebug_enabled: bool,
    pub headless: bool,
    pub frontend_framework: Option<String>,
    #[serde(default)] pub minio: bool,     // toggle de MinIO compartido
    #[serde(default)] pub migration_pending: bool,
    #[serde(default)] pub last_migrated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_of: Option<CloneInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_of: Option<WorktreeInfo>,
    #[serde(default)] pub snapshot_excludes: Vec<String>,
}
```

### 1.1 `Services` (`config.rs:76-81`)

```rust
pub struct Services {
    pub php: PhpService { version: String },
    pub nginx: NginxService { ssl: bool },
    pub db: DbService { ... },
}
```

`DbService` (líneas 67-74) tiene un `#[serde(rename = "type")]` sobre `db_type`:

```rust
pub struct DbService {
    #[serde(rename = "type")] pub db_type: DbType, // mysql | mariadb | postgres
    pub version: String,
    #[serde(rename = "dbName")] pub db_name: String,
}
```

El `frontmatter` (TS) mantiene `services.db.type` y `services.db.dbName` (verificado en `types.ts:13-17`).

### 1.2 `GithubConfig` (`config.rs:132-144`)

```rust
pub struct GithubConfig {
    #[serde(default)] pub repos: Vec<GithubRepo>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub theme: Option<GithubRepo>, // legacy
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub plugins: Vec<GithubRepo>, // legacy
}
```

`GithubRepo::normalize()` (líneas 146-162) pliega los campos legacy en `repos` en cada `read_site_config`. Test cubre la idempotencia (`config.rs:622-651`).

`GithubRepo` (`config.rs:114-130`):

```rust
pub struct GithubRepo {
    pub repo: String,
    pub branch: String,
    pub path: String,                          // relativa a public/
    #[serde(default, skip_serializing_if = "Option::is_none")] pub build_cmd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub build_dirs: Vec<String>,
}
```

### 1.3 `CloneInfo` y `WorktreeInfo` (`config.rs:83-112`)

```rust
pub struct CloneInfo {
    pub parent_id: String,
    pub parent_dirname: String,                // basename del path del padre
    pub snapshot_id: String,
    pub created_at: String,
}

pub struct WorktreeInfo {
    pub parent_id: String,
    pub parent_dirname: String,
    pub target_path: String,                   // ej. wp-content/themes/mi-theme
    pub branch: String,
    pub shared_db: bool,                       // true = mismo esquema del padre
    pub created_at: String,
}
```

Los campos `cloneOf`/`worktreeOf` se omiten en JSON cuando son `None` (`skip_serializing_if`), verificado en `config.rs:653-675`.

### 1.4 `DbType` (`config.rs:12-55`)

```rust
pub enum DbType { Mysql, Mariadb, Postgres }   // #[serde(rename_all = "lowercase")]
```

Tiene métodos dedicados (`service_prefix`, `image`, `port`, `datadir`) que el
resto del backend consume (`docker.rs::db_container_name`, `db_env`,
`DbType::image`).

| `DbType`    | Prefijo container | Imagen                          | Puerto | Datadir container            |
| ----------- | ----------------- | ------------------------------- | ------ | ---------------------------- |
| `Mysql`     | `panel-mysql`     | `mysql:{version}` (no alpine)   | 3306   | `/var/lib/mysql`             |
| `Mariadb`   | `panel-mariadb`   | `mariadb:{version}` (no alpine) | 3306   | `/var/lib/mysql`             |
| `Postgres`  | `panel-postgres`  | `postgres:{version}-alpine`     | 5432   | `/var/lib/postgresql/data`   |

### 1.5 `Endpoint` y `PanelConfig` (`config.rs:248-340`)

```rust
pub struct Endpoint {
    pub loopback_ip: String,   // default "127.0.0.1"
    pub http_port: u16,        // default 80
    pub https_port: u16,       // default 443
}
pub struct PanelConfig { pub endpoint: Option<Endpoint> }
```

Persiste en `~/.config/wordpress-panel/panel.json`.

`endpoint_or_default()` (`config.rs:323-325`) es el getter de URL que usa la UI (vía `siteUrl()` en `types.ts:141-146`).

### 1.6 `SiteState` y `SiteStatus` (`config.rs:342-354`)

`SiteStatus` está serializado en camelCase (`#[serde(rename_all = "camelCase")]`) — los valores llegan al frontend como `"running"`, `"stopped"`, `"migrationPending"`. Espejo en `types.ts:200`.

## 2. `GroupsFile` (`groups.rs:17-22`)

```rust
pub struct GroupsFile { order: Vec<String> }   // #[serde(default)]
```

Archivo: `~/.config/wordpress-panel/groups.json`. Solo es la **lista**; la
pertenencia vive en `site.group`. `rename` también reescribe los `config.json`
de los proyectos (`groups.rs:67-94`).

## 3. `DumpLogEntry` (`dumplog.rs:21-35`)

```rust
pub struct DumpLogEntry {
    pub timestamp: String,    // "YYYY-MM-DDTHH:MM:SSZ" (UTC)
    pub site_id: String,
    pub site_name: String,
    pub db_name: String,
    pub file: String,         // ruta absoluta del .sql
    pub bytes: u64,
    pub source: String,       // "auto" | "stop" | "manual"
}
```

JSONL en `~/.config/wordpress-panel/dump-log.jsonl`. Best-effort: si falla la
adición, no rompe el dump (línea 42, comentario en `dumplog.rs:42-44`).

| `source`  | Quién lo registra                                                                 |
| --------- | ---------------------------------------------------------------------------------- |
| `manual`  | `lib::export_db` (comando IPC).                                                     |
| `stop`    | `docker::stop_site` (export-al-detener).                                            |
| `auto`    | `autodump::persist` (watcher).                                                      |

`dumplog::clean()` (líneas 84-113) **no** borra `.sql`: solo poda el log.

## 4. `SnapshotMeta` (`snapshot.rs:20-38`)

```rust
pub struct SnapshotMeta {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub db_name: String,
    pub db_type: DbType,
    #[serde(default)] pub code_bytes: u64,    // 0 si no se pudo medir
    #[serde(default)] pub db_bytes: u64,
    #[serde(default)] pub excludes: Vec<String>,
}
```

Físicamente: `~/panel-wp/{slug}/snapshots/{sid}/{code.tar.zst, db.sql, meta.json}`.

`SnapshotMeta::excludes` es la instantánea del `site.snapshot_excludes` en el
momento de la creación. Snapshots anteriores a esta serie tienen `excludes: []`.

`KNOWN_BACKUP_DIRS` (`snapshot.rs:56-64`) es la lista por defecto que aparece en
la UI de "excluir del snapshot":

| Ruta                            | Plugin                     |
| ------------------------------- | -------------------------- |
| `wp-content/updraft`            | UpdraftPlus                |
| `wp-content/ai1wm-backups`      | All-in-One WP Migration    |
| `wp-content/wpvividbackups`     | WPvivid                    |
| `wp-content/backups-dup-lite`   | Duplicator                 |
| `wp-content/backups-dup-pro`    | Duplicator Pro             |
| `wp-content/backuply`           | Backuply                   |
| `wp-snapshots`                  | Duplicator                 |

## 5. `ExcludableEntry` (`snapshot.rs:41-52`)

```rust
pub struct ExcludableEntry {
    pub path: String,       // relativa a public
    pub bytes: u64,
    pub known: bool,
    pub label: Option<String>,
}
```

`detect_excludable` (líneas 240-286) expone (a) subcarpetas inmediatas de
`wp-content` que no sean `uploads`/`cache`, y (b) las rutas de
`KNOWN_BACKUP_DIRS` que existan en disco.

## 6. `SystemStatus` (`system.rs:18-29`)

```rust
pub struct SystemStatus {
    pub docker_ok: bool,
    pub network_ok: bool,
    pub dnsmasq_ok: bool,
    pub mkcert_ok: bool,
    pub cli_wrapper_ok: bool,
    pub plasmoid_ok: bool,
    pub endpoint: Endpoint,
    pub projects_root: String,
    pub config_dir: String,
}
```

Comprobaciones best-effort: cada check devuelve `false` en vez de abortar
(`system.rs:31-33`).

## 7. `Migration` (`migrate.rs:24-29`)

```rust
pub struct Migration {
    pub site: SiteConfig,
    pub note: Option<String>,
}
```

`note` se usa para avisar de cosas como "no había dump en `app/sql/`" (`migrate.rs:144-147`).

## 8. `ImportResult` (`localwp.rs:78-83` / `lib.rs:295-303`)

```rust
pub struct ImportResult {
    pub site: SiteConfig,
    pub note: Option<String>,
}
```

Idéntico a `Migration` salvo por el contexto. La UI muestra la `note` como banner.

## 9. `DisconnectedSite` (`config.rs:443-458`)

```rust
pub struct DisconnectedSite {
    pub folder_name: String,
    pub path: String,
    pub name: String,
    pub domain: String,
    pub php_version: String,
    pub db_version: String,
    pub db_type: String,    // "mysql" | "mariadb" | "postgres"
    pub has_dump: bool,
    pub kind: String,        // "preserved" | "reconstructed"
}
```

`list_disconnected_sites()` (`config.rs:464-517`) escanea `~/panel-wp/`:

- Si `dir/config.json` existe → se ignora (sigue conectado).
- Si existe `dir/config.disconnected.json` → `kind: "preserved"`, datos del sidecar.
- Si solo hay `dir/app/public/wp-config.php` → `kind: "reconstructed"`, defaults
  (`php: 8.3`, `db: 8.0 mysql`, `domain: {slug}.test`).

## 10. `WpVersion` (`wordpress.rs:53-57`)

```rust
pub struct WpVersion {
    pub version: String,
    pub status: String,   // "latest" | "outdated" | "insecure"
}
```

Cacheado en `wp-versions.json` durante 24 h (`wordpress.rs:61-93`).

## 11. `NewSiteRequest` (`wordpress.rs:20-47`)

> Notas divergentes:
>
> - `frontend_framework` se mantiene en `SiteConfig` y se envía en
>   `NewSiteRequest`, **pero la UI no tiene campo para elegir framework** todavía
>   (queda como DEFERRED — ver `KNOWN_ISSUES.md` y la rama `headless`).
> - `headless` se persiste en `SiteConfig` (default `false`, `wordpress.rs:42`),
>   pero la imagen php-fpm no incluye aún un frontend headless (DEFERRED, ver
>   `CLAUDE.md` → "Diferido dentro de Fase 3: container de frontend headless").

## 12. `GhStatus` / `BranchStatus` / `DetectedRepo` (`github.rs`)

```rust
pub struct GhStatus { pub installed: bool, pub authenticated: bool, pub user: Option<String> }

pub struct BranchStatus {
    pub current: String,
    pub target: String,
    pub has_remote: bool,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: bool,
    pub can_pull: bool,
    pub message: String,
}

pub struct DetectedRepo {
    pub path: String,
    pub name: String,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub registered: bool,
}
```

`BranchStatus::can_pull` se calcula en `summarize` (`github.rs:232-249`) y es
pura/testeable (test en `github.rs:589-611`).

## 13. `RunningSite` (`dbus.rs:19-24`)

```rust
struct RunningSite {                // serializa a JSON (no #[serde(rename_all)])
    pub id: String,
    pub name: String,
    pub domain: String,
}
```

Solo se usa como payload interno del método D-Bus `GetRunningSites`.

## 14. `NewSiteRequest` ↔ `SiteConfig`: correspondencia de defaults

Verificada en `lib::create_site` + `wordpress::create_site` (`wordpress.rs:106-182`):

| Campo `SiteConfig`         | Origen en `NewSiteRequest`                  | Default                              |
| -------------------------- | ------------------------------------------- | ------------------------------------ |
| `id`                       | `Uuid::new_v4()`                            | generado                              |
| `name`                     | `req.name`                                  | requerido                             |
| `path`                     | `projects_root / slugify(req.name)`         | `~/panel-wp/{slug}`                  |
| `domain`                   | `req.domain` o `format!("{slug}.test")`     | `{slug}.test`                         |
| `group`                    | `req.group`                                 | `None`                                |
| `created_at`               | `Utc::now().to_rfc3339()`                   | ahora                                 |
| `services.php.version`     | `req.php_version`                           | requerido                             |
| `services.nginx.ssl`       | `req.ssl`                                   | `default = false`                     |
| `services.db.db_type`      | `req.db_type`                               | requerido                             |
| `services.db.version`      | `req.db_version`                            | requerido                             |
| `services.db.db_name`      | `format!("{slug}_db", …)`                   | `{slug}_db`                           |
| `one_click_admin`          | `req.one_click_admin`                       | `default = false`                     |
| `xdebug_enabled`           | `req.xdebug`                                | `default = false`                     |
| `headless`                 | `req.headless`                              | `default = false`                     |
| `frontend_framework`       | `req.frontend_framework`                    | `None`                                |
| `minio`                    | `req.minio`                                 | `default = false`                     |
| `migration_pending`        | `false` sempre en alta                       | `false`                               |
| `github`                   | `GithubConfig::default()`                    | `repos: []`                           |
| `clone_of` / `worktree_of` | `None`                                       | `None`                                |
| `snapshot_excludes`        | `vec![]`                                     | `[]`                                  |

## 15. `slugify` (usado en `wordpress`, `clone`, `worktree`)

- `wordpress::slugify` (`wordpress.rs:466-476`): `is_alphanumeric` →
  `-`. Acepta Unicode (test en `wordpress.rs:498-504`).
- `clone::slugify` (`clone.rs:186-207`): ASCII-only, `trim_matches('-')`,
  `"clone"` si vacío.
- `worktree::slugify` (`worktree.rs:462-477`): ASCII-only, `"wt"` si vacío.

## 16. Persistencia y recarga

| Símbolo                              | Ruta                                                                                |
| ------------------------------------ | ----------------------------------------------------------------------------------- |
| `config::config_dir()`               | `dirs::config_dir()` + `/wordpress-panel` (`config.rs:361-367`).                     |
| `config::projects_root()`            | `dirs::home_dir()` + `/panel-wp` (`config.rs:369-376`).                              |
| `config::panel_config_path()`        | `config_dir()/panel.json` (`config.rs:297-299`).                                      |
| `groups::groups_path()`              | `config_dir()/groups.json` (`groups.rs:24-26`).                                       |
| `dumplog::log_path()`                | `config_dir()/dump-log.jsonl` (`dumplog.rs:37-39`).                                   |
| `nginx::conf_d_dir()`                | `config_dir()/nginx/conf.d` (`nginx.rs:12-16`).                                       |
| `domain::snippet_path()`             | `config_dir()/dnsmasq-panel.conf` (`domain.rs:24-26`).                                |
| `domain::install_target()`           | `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` (`domain.rs:30-32`).             |
| `wordpress::fetch_versions` (cache)  | `config_dir()/wp-versions.json` (`wordpress.rs:62-75`).                               |
| `php::wp_cli_phar_path()`            | `config_dir()/wp-cli.phar` (`php.rs:56-79`).                                          |
| `docker::db_data_dir()`              | `config_dir()/db-data/{container}` (`docker.rs:1015-1021`).                           |
| `docker::ensure_minio` (data)        | `config_dir()/minio-data/` (`docker.rs:366-367`).                                     |
| `cli::local_bin()`                   | `~/.local/bin/` (`cli.rs:16-23`).                                                     |
| `Snapshot::snapshots_root()` (per site) | `Path::new(&site.path).join("snapshots")` (`snapshot.rs:66-68`).                   |
| `config::disconnected_config_path()` | `Path::new(path)/config.disconnected.json` (`config.rs:437-439`).                    |
| `wp::cwd` (CLI)                      | `~/panel-wp` (env `PANEL_WP_ROOT` lo sobrescribe, `mcp/server.mjs:21`).               |

## 17. Mutaciones seguras

`write_site_config` (`config.rs:412-417`) reescribe el `config.json` sólo en el
sistema de archivos local. Para cambios en disco, los `keep` se controlan desde
`backup::rotate_dumps(site, 3)` (`backup.rs:71-92`). `dbus::notify_sites_changed`
(`dbus.rs:32-34`) emite el evento `sites-changed` que `+page.svelte:181` escucha
para forzar una recarga de la lista.

## 18. Divergencias detectadas

| Punto                                                                 | Detalle                                                                                                  |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `GithubConfig::theme` / `::plugins` (legacy)                          | Existen solo para leer `config.json` antiguos; `normalize()` los fusiona en `repos` y nunca se reescriben. |
| `dbName` (`DbService`), `type` (`DbService`)                          | Renombres explícitos en `#[serde(rename)]` (`config.rs:69-73`); espejo en `types.ts:13-17`.              |
| `DisconnectedSite.db_type`                                             | `String` (no `DbType`) en el wire — `localwp.rs:454` usa `"mysql"`.                                      |
| `db_type` en `WpVersion`                                                | No se persiste; solo la lista cacheada (`wp-versions.json`).                                              |
| `SnapshotMeta::excludes`                                               | `0` en snapshots antiguos; `default` evita que rompa el parseo (`snapshot.rs:28-37`).                      |
| `dump-log.jsonl`                                                       | JSONL, una línea por volcado; `source ∈ {auto, stop, manual}` (`dumplog.rs:34`).                          |
| `php_support = IMAGE_REV`                                              | `r3` (`php.rs:18`); subirla fuerza reconstruir imagen y recrear containers.                              |
| `wp-cli.phar` binding                                                  | `wp-cli.phar` del host → `/usr/local/bin/wp:ro` (lectura) en el container (`docker::create_php_container`). |
| `localhost:8025` Mailpit UI                                             | Solo loopback (`docker.rs:32`, `web PortBinding` a `127.0.0.1`).                                          |
| `localhost:9100/9101` MinIO API/console                                | Solo loopback (`docker.rs:33-34`).                                                                        |
| `localhost:8088` Adminer UI                                             | Solo loopback (`docker.rs:35`).                                                                           |
| `wp-php` container (proyectos)                                         | No publica puertos al host (`docker.rs:754-759`); nginx lo alcanza por `panel-net`.                       |
| `panel-nginx`                                                          | Publica en `{loopback_ip}:{http_port}` / `{https_port}` (`docker.rs:514-528`). Puertos alt (`8080`/`8443+`) para coexistir con LocalWP. |
| `panels-mailpit`/`panel-minio`                                         | Imagen sin restricción de versión (`mailpit:latest`, `minio:latest`).                                     |

## 19. Estado de deuda / Diferido

- `SiteConfig::headless` y `frontend_framework` son campos sostenidos pero
  **no hay UI ni container de frontend** todavía (ver `CLAUDE.md` y
  `KNOWN_ISSUES.md`).
- `Domain::DEFAULT_IP` (`127.0.0.1`) y `pick_loopback_ip` (`netcheck.rs:115-126`)
  existen pero `autoselect_endpoint` (`docker.rs:583-597`) ya siempre toma
  puertos altos para coexistir con LocalWP; la selección de IP alterna se
  conserva como historico / fallback.
- `docker::running_panel_containers` (`docker.rs:110-128`) y `remove_container`
  (`docker.rs:876-889`) están marcados `#[allow(dead_code)]` por la nota
  "detección de huérfanos / cleanup en Fase 2" — no se invocan hoy.
- `localwp::import_site` (Fase 4) depende de `cp -a` y `mysqldump` para un
  import "limpio"; en `KNOWN_ISSUES.md` se documenta que la DB requiere dump
  en disco (`local.sql`).
- `feature_stub` (`lib::create::feature_stub`) cubre `cloudflare`, `deploy`,
  `package` como botones preparados pero no implementados.

## Fuentes primarias

- `src-tauri/src/config.rs` (modelos, `parse_db_name`, `path_basename`, `load/save_panel_config`).
- `src-tauri/src/groups.rs` (GroupsFile).
- `src-tauri/src/dumplog.rs` (DumpLogEntry, `append`, `clean`).
- `src-tauri/src/snapshot.rs` (SnapshotMeta, ExcludableEntry, `KNOWN_BACKUP_DIRS`).
- `src-tauri/src/system.rs` (SystemStatus).
- `src-tauri/src/migrate.rs` (Migration).
- `src-tauri/src/localwp.rs` (LocalSite, ImportResult).
- `src-tauri/src/wordpress.rs` (NewSiteRequest, WpVersion, slugify).
- `src-tauri/src/github.rs` (GhStatus, BranchStatus, DetectedRepo).
- `src-tauri/src/php.rs` (IMAGE_REV).
- `src-tauri/src/docker.rs` (constantes y rutas).
- `src-tauri/src/ssl.rs`, `src-tauri/src/dbus.rs`, `src-tauri/src/cli.rs`.
- `src/lib/types.ts` (espejo TS).
- `docs/CHANGELOG.md`, `docs/KNOWN_ISSUES.md`.
