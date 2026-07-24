# 03 · Mapa del repositorio

> Documento técnico verificado contra el commit `373841c` (rama `main`,
> 2026-07-23). Cubre la estructura física del proyecto, los límites de cada
> módulo y la dirección de las dependencias. Las convenciones se conservan
> literalmente desde `CLAUDE.md` y el código fuente.

## 1. Estructura física (top-level)

| Ruta                                          | Tipo          | Propósito                                                                                      |
| --------------------------------------------- | ------------- | ---------------------------------------------------------------------------------------------- |
| `CLAUDE.md`                                   | Doc           | Punto de entrada para agentes; convenciones y comandos rápidos.                                |
| `PLAN.md`                                     | Doc           | Plan de producto por fases, UI, flujos y decisiones.                                           |
| `DESIGN.md`                                   | Doc           | Notas de diseño de UI/UX.                                                                      |
| `README.md`                                   | Doc           | Resumen del proyecto.                                                                          |
| `docs/`                                       | Doc           | Arquitectura, extensión, changelog, issues, testing, planes.                                   |
| `src/`                                        | SvelteKit SPA | Frontend (sin SSR, `adapter-static`).                                                          |
| `src-tauri/`                                  | Cargo         | Backend Rust (Tauri 2).                                                                        |
| `docker/`                                     | Assets        | Dockerfiles, plantillas, mu-plugins, plugin Adminer.                                           |
| `scripts/`                                    | Scripts shell | `first-run.sh`, `wp-wrapper.sh`, `wordpress-panel-cli.sh`, `package-plasmoid.sh`.              |
| `mcp/`                                        | Node MCP      | Servidor MCP para agentes IA (`server.mjs`, `README.md`); solo protocolo JSON-RPC 2.0 por stdio. Sin deps.  |
| `plasma/`                                     | QML           | Plasmoid KDE (complemento al backend).                                                         |
| `e2e/`                                        | Playwright    | Tests end-to-end con `pnpm dev:mock` + `pnpm test:e2e`.                                        |
| `package.json` / `pnpm-lock.yaml`             | Node          | Frontend + toolchain (vite, svelte 5, tailwind, playwright).                                   |
| `Cargo.toml` / `Cargo.lock` (`src-tauri/`)    | Rust          | Dependencias del backend.                                                                      |
| `tauri.conf.json` (`src-tauri/`)              | Tauri         | Configuración de la app (ventana, build, identifier).                                          |
| `tsconfig.json` / `svelte.config.js` / `vite.config.js` | TS / Svelte | Configuración frontend.                                                                        |
| `tailwind.config.js` / `postcss.config.js`    | CSS           | Tailwind + autoprefixer.                                                                       |
| `playwright.config.ts`                        | TS            | Configuración de Playwright (e2e).                                                             |
| `.npmrc` / `pnpm-workspace.yaml`              | Node          | Configuración de pnpm.                                                                         |
| `skills-lock.json`                            | JSON          | Lockfile de skills/plugins.                                                                    |

## 2. Estructura del backend (`src-tauri/src/`)

Constatado en `src-tauri/src/lib.rs:3-27` (líneas `mod …`):

| Módulo                       | Símbolo / API pública                                              | Rol                                                            |
| ---------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------- |
| `autodump.rs`                | `AutoDump` (`Default`), `AutoDump::start(stop)`, `watch(site)`     | Watcher por proyecto que vigila `Innodb_rows_*` y dumpea si la DB cambió. |
| `autologin.rs`               | `open_admin(app, docker, site, user_id)`                          | Auto-login al admin con token efímero (mu-plugin `panel-autologin.php`). |
| `backup.rs`                  | `dump_bytes`, `export_db_to`, `export_db`, `rotate_dumps`         | `mysqldump` dentro del container DB, escritura a `app/sql/`, rotación. |
| `cli.rs`                     | `install_cli_wrapper`, `open_terminal_at`                         | Instala `wp` y `wordpress-panel-cli` en `~/.local/bin`; abre terminal. |
| `clone.rs`                   | `create_clone(app, docker, parent_id, snapshot_id)`               | Crea un SiteConfig `clone_of` desde un snapshot.               |
| `config.rs`                  | `DbType`, `PhpService`, `NginxService`, `DbService`, `Services`, `GithubRepo`, `GithubConfig`, `SiteConfig`, `CloneInfo`, `WorktreeInfo`, `Endpoint`, `PanelConfig`, `SiteState`, `SiteStatus`, `DisconnectedSite`; `load_all_sites`, `find_site`, `read_site_config`, `write_site_config`, `list_disconnected_sites`, `parse_db_name`, `projects_root`, `config_dir`, `panel_config_path`, `load/save_panel_config`, `load/save_endpoint`, `endpoint_or_default`, `clear_endpoint`, `disconnected_config_path`, `path_basename`. | Modelos + persistencia (fuente de verdad: `~/panel-wp/{slug}/config.json`). |
| `dbus.rs`                    | `Manager` (interface zbus), `serve(app)`                          | Sirve `com.goldmediatech.WordpressPanel.Manager` para el plasmoid. |
| `docker.rs`                  | `DockerManager`, `NETWORK`, `NGINX`, `MAILPIT`, `MINIO`, `ADMINER`, `MAILPIT_UI_PORT`, `MINIO_API_PORT`, `MINIO_CONSOLE_PORT`, `ADMINER_UI_PORT`, `PANEL_PREFIXES`; `db_container_name`, `db_data_dir`, `docker_assets_dir`, `host_uid_gid`. | Orquestación bollard (red, containers, ciclo de vida, exec). |
| `domain.rs`                  | `DEFAULT_IP`, `wildcard_rule`, `snippet_path`, `install_target`, `wildcard_active`, `resolves_to`, `ensure_wildcard`, `install_wildcard`. | Resolución DNS wildcard `*.test` vía dnsmasq. |
| `dumplog.rs`                 | `DumpLogEntry`, `append`, `read_all`, `clean`                     | Log JSONL de volcados de DB.                                    |
| `github.rs`                  | `GhStatus`, `BranchStatus`, `DetectedRepo`; `status`, `clone`, `pull`, `branch_status`, `deploy`, `build_dir_candidates`, `remove_dir`, `propose_path`, `scan`, `read_repo_meta`, `open_vscode`, `ensure_workspace`. | Integración con `gh` CLI y `git` CLI para themes/plugins. |
| `groups.rs`                  | `GroupsFile`, `list`, `create`, `rename`, `delete`, `reorder`     | Persistencia de `groups.json` (orden + grupos vacíos).         |
| `localwp.rs`                 | `LocalSite`, `ImportResult`; `list_sites`, `import_site`          | Importación de LocalWP (lee `~/.config/Local/sites.json`).     |
| `logs.rs`                    | `event_name`, `spawn_stream`                                      | Streaming de logs `wp-{id}` → evento `log:{id}`.               |
| `migrate.rs`                 | `Migration`, `migrate_site`, `import_dump`, `fix_site_url`        | Aprovisiona un proyecto en el sistema actual (DB, dump, SSL).  |
| `netcheck.rs`                | `PortStatus`, `port_status`, `pick_loopback_ip`, `pick_alt_port`, `holder_name`. | Lee `/proc/net/tcp{,6}` para detectar puertos libres y elegir endpoint. |
| `nginx.rs`                   | `conf_d_dir`, `ensure_tuning`, `project_dirname`, `render_vhost`, `write_vhost`, `remove_vhost`. | Renderiza y escribe los vhosts en `~/.config/wordpress-panel/nginx/conf.d/`. |
| `php.rs`                     | `IMAGE_REV`, `ensure_php_image`, `wp_cli_phar_path`               | Build de la imagen `panel-php:{ver}-r3` y descarga de `wp-cli.phar`. |
| `progress.rs`                | `EVENT`, `PROGRESS_PREFIX`, `log`, `log_progress`                 | Emite `op-log` para la consola de progreso del frontend.       |
| `snapshot.rs`                | `SnapshotMeta`, `ExcludableEntry`; `create_snapshot`, `list_snapshots`, `delete_snapshot`, `detect_excludable`, `snippet_path`, `snapshot_dir`. | Puntos de guardado (código tar.zst + db.sql). |
| `ssl.rs`                     | `has_cert`, `generate`                                            | Cert mkcert por dominio.                                       |
| `system.rs`                  | `SystemStatus`, `status`, `mkcert_ca_installed`, `wrapper_installed`, `plasmoid_installed`. | Estado del sistema para la pantalla de Configuración. |
| `wordpress.rs`               | `NewSiteRequest`, `WpVersion`; `fetch_versions`, `create_site`, `create_dirs`, `write_php_ini`, `download_core`, `create_database`, `reset_database`, `drop_database`, `wp_config_create`, `sync_mu_plugins`, `inject_mailpit_muplugin`, `inject_autologin_muplugin`, `slugify`. | Descarga + instalación de WP; crea estructura, mu-plugins, BD. |
| `worktree.rs`                | `create_worktree`, `remove_worktree`, `list_worktrees`            | Worktree-projects (rama en aislamiento, no duplica WP).        |
| `wpcli.rs`                   | `run`                                                              | Ejecuta `wp <args>` en el container del proyecto como `www-data`. |
| `integration_tests.rs`       | (test)                                                             | Tests marcados `#[ignore]` que mutan el entorno (Docker, sitios). |

> `lib.rs` registra el estado Tauri `LogStreams` (map de `JoinHandle` por id) y
> `AutoDump` (map de `JoinHandle` por id) y lanza el servidor D-Bus en
> `setup()` (`src-tauri/src/lib.rs:964-1010`).

## 3. Estructura del frontend (`src/`)

```
src/
├── app.css, app.d.ts, app.html
├── lib/
│   ├── api.ts            ← espejo de los #[tauri::command]
│   ├── types.ts          ← espejo de los modelos serde (camelCase)
│   ├── components/
│   │   ├── ProjectDetail.svelte     ← detalle embebido (master-detail)
│   │   ├── OpConsole.svelte         ← escucha `op-log` (re-rewrite en sitio)
│   │   ├── DeleteProjectModal.svelte
│   │   └── ImportProjectModal.svelte
│   └── dev/              ← mocks para `pnpm dev:mock`
└── routes/
    ├── +layout.svelte, +layout.ts, +page.svelte
    ├── site/
    │   ├── new/
    │   └── [id]/         ← wrapper de deep-link (delegado a `+page`)
    ├── domains/ (vía d-bus), services/, settings/, cli/
    └── import-localwp/, dumps/
```

## 4. Estructura de `docker/`

| Carpeta/archivo     | Contenido                                                             |
| ------------------- | --------------------------------------------------------------------- |
| `docker/php/`       | `Dockerfile` (`php:{ver}-fpm-alpine`, extensiones WP) + `entrypoint.sh` (remapeo `www-data` con `PUID`/`PGID`). |
| `docker/adminer/`   | `autologin.php` (plugin Adminer para auto-login con `server`/`pgsql`+`username`+`db`). |
| `docker/nginx/`     | `vhost.conf.tmpl` (plantilla usada por `nginx::render_vhost` para generar `conf.d/{id}.conf`). |
| `docker/mu-plugins/`| `panel-mailpit.php` (enruta correo a `panel-mailpit:1025` con `X-Project-ID`) y `panel-autologin.php` (token efímero). |
| `docker/php.ini.tmpl` | Configuración PHP base por proyecto (montada como `zz-project.ini`). |

## 5. Estructura de `scripts/`

| Script                       | Instalación                          | Función                                                                                       |
| ---------------------------- | ------------------------------------ | --------------------------------------------------------------------------------------------- |
| `first-run.sh`               | Ejecución única por usuario          | Prepara el sistema: red `panel-net`, dnsmasq (`/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf`), CA `mkcert`. |
| `wordpress-panel-cli.sh`     | `~/.local/bin/wordpress-panel-cli`   | CLI que habla con la app en ejecución por D-Bus. Detecta el proyecto por `~/panel-wp/*/config.json` (escaneo JSON con `sed`). |
| `wp-wrapper.sh`              | `~/.local/bin/wp`                    | Wrapper WP-CLI: `detect-project` + `docker exec -i --user www-data wp-{id} wp …`.            |
| `package-plasmoid.sh`        | Manual                               | Empaqueta el plasmoid KDE.                                                                    |

## 6. Mapa de dependencias (resumen)

| Origen                | Destino                                                                      | Mecanismo                                                                 |
| --------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Frontend (SvelteKit)  | Backend (Tauri)                                                              | `invoke<T>(cmd, args)` (`src/lib/api.ts`).                                |
| Frontend (OpConsole)  | Backend (Tauri)                                                              | `listen('op-log', …)` (canal `EVENT` = `"op-log"` en `progress.rs`).      |
| Frontend (ProjectDetail) | Backend                                                                   | `listen(\`log:${id}\`, …)` (canal emitido por `logs::spawn_stream`).      |
| Frontend (`+page`)    | Backend (`sites-changed`)                                                    | `listen('sites-changed', …)` (emitido por `dbus.rs::notify_sites_changed`).|
| `wordpress-panel-cli` | Panel en ejecución                                                           | D-Bus (`gdbus`/`qdbus6`) → `com.goldmediatech.WordpressPanel.Manager`.     |
| Plasmoid KDE          | Panel en ejecución                                                           | D-Bus (igual que el CLI).                                                  |
| `mcp/server.mjs`      | `wordpress-panel-cli`                                                        | `spawn(CLI, argv, …)` y captura stdout/stderr; protocolo JSON-RPC 2.0.     |
| `wp-wrapper.sh`       | Docker CLI (`docker exec`)                                                   | Caso especial: salta bollard para evitar el bug de `exec` con stdin.       |
| `php::ensure_php_image` | Docker CLI (`docker build`)                                                | Construye `panel-php:{ver}-r3` desde `docker/php/Dockerfile`.              |
| `docker::migrate_db_to_volume` | Docker CLI (`docker cp`)                                            | Excepción documentada (extraer dir por stream tar sería complejo).         |
| `migrate::import_dump` | Docker CLI (`docker exec -i`)                                              | Excepción documentada: bollard `exec` con stdin se cuelga con dumps grandes. |
| `localwp::cp_contents` | CLI externo (`cp -a`)                                                       | Copia eficiente de `app/public`.                                          |
| `tld/tar` (varios `snapshot.rs`, `wordpress::download_core`) | CLI externo | `tar`/`curl`…                                                  |
| `github/*.rs`         | `gh`/`git` CLI (host)                                                        | Sin reimplementación; usa la sesión y SSH keys del host.                 |
| `ssl.rs`              | `mkcert` CLI (host)                                                          | Cert por dominio.                                                          |

## 7. Tres puntos de entrada del panel

1. **GUI Tauri** (`src-tauri/src/main.rs` →
   `wordpress_panel_lib::run()`): la app de escritorio principal.
2. **CLI shell** (`scripts/wordpress-panel-cli.sh`): comandos vía D-Bus
   (`ListSites`, `StartSite`, `StopSite`, `OpenAdmin`, `OpenSite`,
   `ProjectContainers`, `ListWorktrees`, `CreateWorktree`, `RemoveWorktree`,
   `CreateSnapshot`, `ListSnapshots`, `DeleteSnapshot`, `CreateClone`,
   `GhScan`, `GhPull`, `GhBranchStatus`, `GhBuildDirs`, `GhSetDeploy`,
   `GhDeploy`). Requiere panel en ejecución.
3. **MCP** (`mcp/server.mjs`): envuelve el CLI para agentes IA externos
   (JSON-RPC 2.0 por stdio). Mismo nivel que el CLI.

## 8. Estado de la fuente de verdad

| Hecho                                            | Persistencia                                                                                              |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| Configuración de cada proyecto                   | `~/panel-wp/{slug}/config.json` (`config::write_site_config`, `config::read_site_config`).                |
| Lista de grupos (orden + grupos vacíos)          | `~/.config/wordpress-panel/groups.json` (`groups.rs::write_file`).                                       |
| Log de volcados de DB                            | `~/.config/wordpress-panel/dump-log.jsonl` (`dumplog::log_path`).                                          |
| Endpoint publicado (puerto alterno)              | `~/.config/wordpress-panel/panel.json` (`config::panel_config_path`).                                     |
| Versiones de WP cacheadas                        | `~/.config/wordpress-panel/wp-versions.json` (`wordpress::fetch_versions`).                               |
| Cache de certificate CA (`mkcert`)               | `~/.config/wordpress-panel/rootCA.pem` (gestionado por `mkcert -CAROOT`).                                 |
| Snippet dnsmasq (texto)                          | `~/.config/wordpress-panel/dnsmasq-panel.conf` (`domain::snippet_path`).                                  |
| Datadirs DB compartidos                          | `~/.config/wordpress-panel/db-data/{container}/` (`docker::db_data_dir`).                                 |
| `wp-cli.phar`                                    | `~/.config/wordpress-panel/wp-cli.phar` (`php::wp_cli_phar_path`).                                         |
| minio-data                                       | `~/.config/wordpress-panel/minio-data/` (`docker::ensure_minio`).                                          |
| Datos MinIO                                      | `config_dir/minio-data` (bind a `/data`).                                                                  |
| vhosts `panel-nginx`                             | `~/.config/wordpress-panel/nginx/conf.d/{id}.conf` + `00-panel-tuning.conf` (`nginx::conf_d_dir`).       |
| Snippet dnsmasq activo                           | `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` (instalado por `first-run.sh` o `domain::install_wildcard`). |

## 9. Convenciones reiteradas (no romper)

- **Naming de containers**: `wp-{site-id}` (proyectos) y `panel-*` (compartidos).
- **Modelo en camelCase**: todos los structs serde llevan
  `#[serde(rename_all = "camelCase")]` (ver `config.rs`, `github.rs`, `migrate.rs` etc.).
- **`Result<T, String>`**: todos los `#[tauri::command]` usan el alias `CmdResult<T>`
  y la función `e<E: Display>` (línea `lib.rs:43-47`) para mapear errores.
- **Comandos añadidos en `lib.rs` → `api.ts`**: paritario. Si falta en `api.ts`,
  el botón de la UI no compila.
- **Docker vía bollard** en runtime; las **tres excepciones** documentadas son:
  `docker build` (imagen php), `docker cp` (migración datadir) y
  `docker exec -i` (import de dump). Justificaciones cruzadas en
  `docker.rs:947-950`, `docker.rs:240-243`, `migrate.rs:208-214`.
- **Eventos backend→frontend**: requieren capability. `src-tauri/capabilities/default.json`
  solo añade `core:default` + `core:event:default` (necesario para `OpConsole`).
- **Watchers de auto-dump y de logs**: viven en estado Tauri (`AutoDump`,
  `LogStreams`) porque `DockerManager` se reconstruye en cada comando.
- **Sin `sites.json` propio del panel**: la fuente de verdad es el escaneo de
  `~/panel-wp/*/config.json`. `sites.json` solo aparece en LocalWP
  (`localwp::sites_json()`).
- **No existen `agent.rs`, `ports.rs`, `shutdown.rs`**: cualquier mención en
  código es un plan de Fase 2/Fase 5 con `#[allow(dead_code)]` o un TODO
  histórico; no son módulos actuales.

## 10. Gaps / deuda técnica explícita

- Comentarios `// Fase 2` / `// Fase 5` en `docker.rs`, `lib.rs` y
  `EXTENDING.md` referencian módulos no creados todavía. Ejemplos:
  - `docker.rs:50` `#[allow(dead_code)] // usado por logs.rs / dbus.rs en Fase 2`,
  - `docker.rs:109` `// detección de huérfanos (shutdown.rs) en Fase 2`,
  - `docker.rs:875` `// limpieza de huérfanos / recrear container en Fase 2`,
  - `EXTENDING.md:89` `## Proveedor de IA (Fase 5, agent.rs)`.
- `feature_stub` (`lib.rs:674-684`) es un comando Tauri que devuelve siempre
  error para features pendientes (`cloudflare`, `deploy`, `package`).
- `KNOWN_ISSUES.md` documenta los problemas conocidos UI/Plasma/import
  LocalWP que arrastra la versión actual.

## Fuentes primarias

- `src-tauri/src/lib.rs` (registro de comandos `invoke_handler!` y `setup()`).
- `src-tauri/src/config.rs` (modelos serde y rutas de persistencia).
- `src-tauri/src/docker.rs` (orquestación bollard, puertos, imágenes).
- `src-tauri/src/dbus.rs` (interfaz zbus y catálogo completo de métodos).
- `src-tauri/src/groups.rs`, `src-tauri/src/dumplog.rs`, `src-tauri/src/snapshot.rs`.
- `src/lib/api.ts` y `src/lib/types.ts` (espejo frontend).
- `scripts/wordpress-panel-cli.sh` (CLI shell).
- `mcp/server.mjs` (servidor MCP).
- `src-tauri/capabilities/default.json` (cómo se permite `core:event`).
- `CLAUDE.md` (convenciones).
