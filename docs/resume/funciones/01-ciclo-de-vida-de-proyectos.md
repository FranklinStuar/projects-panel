# 01 — Ciclo de vida de proyectos

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para crear, arrancar, parar, borrar y
> re-importar proyectos de Panel WP.

## Resultado para el usuario

El usuario crea un proyecto (alta de WordPress + DB + nginx + SSL + auto-login),
lo enciende, lo apaga, lo borra (con opción de conservar la carpeta desconectada)
y lo re-importa si lo trae de otra PC. Las herramientas de gestión (terminal,
wp-admin, frontend, logs) siempre pasan por el ciclo sin estado mágico: la fuente
de verdad es `~/panel-wp/<slug>/config.json` y la lista vuelve a escanearse
cuando hace falta.

## Precondiciones

- Docker daemon accesible (`docker_ok` en `system_status`).
- Red `panel-net` creada (`create_panel_network` si no).
- dnsmasq wildcard `*.test` activo (`domain::wildcard_active`).
- CA de mkcert instalada por `scripts/first-run.sh` (Fase 4).
- Wrappers `wp` / `wordpress-panel-cli` en `~/.local/bin` (instalación idempotente
  al arranque del panel y bajo demanda con `install_cli_wrapper`).
- Endpoint resuelto: `127.0.0.1:80/443` o, en su defecto, puertos altos
  (autoelegidos por `docker::autoselect_endpoint`).

## Flujo feliz (camino nominal)

1. **Crear** (`POST create_site` → `lib.rs::create_site` →
   `wordpress::create_site`) genera un `id` UUID, calcula `slug` con
   `wordpress::slugify`, valida que la carpeta no exista
   (`SiteConfig.path = projects_root/{slug}`), crea subcarpetas
   (`app/public`, `app/sql`, `conf/php`, `logs/php`, `ssl`, `data`,
   `wordpress::create_dirs`), escribe `php.ini` desde `docker/php.ini.tmpl`
   (`wordpress::write_php_ini`) y persiste `config.json`.
2. **DB list** (`docker::ensure_db`): asegura la imagen oficial, prepara bind
   `config_dir/db-data/{container}` sobre `DbType::datadir()`, crea el container
   `panel-{prefix}-{version}` (no `mysql:80` → `panel-mysql-80`) y espera a que
   el motor TCP acepte (`docker::wait_db_ready`, gate `-h127.0.0.1`).
3. **DB schema**: `wordpress::create_database` materializa la base
   `"{slug}_db"`.
4. **WP core**: `wordpress::download_core` baja
   `https://wordpress.org/wordpress-{version}.tar.gz` y la extrae con `tar
   --strip-components=1` en `app/public/`.
5. **mu-plugins**: `wordpress::sync_mu_plugins` inyecta `panel-mailpit.php`
   (siempre) y `panel-autologin.php` (si `one_click_admin`). Lee el placeholder
   `__PROJECT_ID__` y lo sustituye por el id del proyecto.
6. **SSL**: si `services.nginx.ssl`, `ssl::generate` ejecuta
   `mkcert -cert-file -key-file <ssl/cert.pem> <ssl/key.pem> <domain>`.
7. **Encender**: `docker::start_site` arranca el container php-fpm
   (`panel-php:{ver}-{rev}`), escribe el vhost (`nginx::write_vhost`) en
   `config_dir/nginx/conf.d/{id}.conf`, arranca `panel-nginx` y hace
   `nginx -s reload` (recrea el container si quedó zombie).
8. **wp-config + install**: `wordpress::wp_config_create` + `wp_core_install`
   corren WP-CLI dentro del container php con `www-data` para fijar
   home/siteurl y crear el admin.
9. **Mover al listado**: `config::write_site_config` ya escribió el `config.json`
   en el paso 1; `load_all_sites` lo recoge al recargar la lista.

**Encender** (`start_site` en `lib.rs`): conecta `DockerManager`,
`docker::start_site` garantiza `panel-net`, `panel-{db}`, mailpit
(`ensure_mailpit`, on-demand), MinIO si `site.minio`, imagen php
(`php::ensure_php_image`), recrea el container del proyecto si quedó con tag
viejo (`IMAGE_REV`), lo arranca, escribe su vhost y recarga nginx.

**Detener** (`stop_site` en `lib.rs`): aborta el watcher de auto-dump
(`autodump::AutoDump::stop`), `docker::stop_site` exporta un dump fresco
(`backup::export_db`, log source `stop`), rota los `db-*.sql` a 3
(`backup::rotate_dumps`), para el container php, quita el vhost y hace reload
de nginx. Después, `teardown_unused_shared` apaga el DB del proyecto si
ningún otro activo la usa, MinIO si nadie lo pide, y nginx/mailpit/adminer
si no queda NINGÚN proyecto activo.

**Parar todos** (`stop_all_sites`): itera `load_all_sites()`,
`autodump::stop(id)` + `docker::stop_site` por cada uno (errores tolerados).

**Borrar** (`delete_site` en `lib.rs` → `lib.rs::delete_site`):
1. `docker::stop_site` (apagado + export).
2. `docker::remove_container(site.container_name())` asegura que el `wp-<id>`
   no quede.
3. `docker::ensure_db` para poder ejecutar `wordpress::drop_database` (DROP
   DATABASE del esquema del proyecto).
4. `teardown_unused_shared` por si quedó algún compartido prendido.
5. Si `delete_folder = true`: `std::fs::remove_dir_all(site.path)`.
6. Si `delete_folder = false`: **desconectar** — `config.json` se renombra a
   `config.disconnected.json` (sidecar). `load_all_sites` solo escanea
   `config.json`, así que el panel la olvida; los archivos en `app/public`,
   `app/sql`, `conf/php`, `ssl`, `data` siguen en disco para re-importar.

**Re-importar desconectado** (`import_disconnected_site`):
- `config::list_disconnected_sites` recorre `~/panel-wp/*` y devuelve los
  directorios sin `config.json`: si tienen `config.disconnected.json` son
  `preserved`; si solo tienen `app/public/wp-config.php` se reconstruyen
  best-effort (`reconstruct_config` estima `dbName` parseando el define
  `DB_NAME` del `wp-config.php` con `config::parse_db_name`).
- `import_disconnected` restaura el `config.json` (corrigiendo `path` por la
  ruta real), le asigna un `id` nuevo si colisiona, fija `migration_pending =
  true`, borra el sidecar y registra el proyecto como pendiente.
- El usuario enciende luego con "Migrar y encender" (`migrate_site`).

**Re-importar LocalWP** (`import_localwp_site` → `localwp::import_site`):
- `localwp::read_raw` parsea `~/.config/Local/sites.json`.
- `localwp::cp_contents` clona `app/public` con `cp -a` (preserva atributos,
  más rápido que un walker en Rust).
- `localwp::import_site` deja una `imported.sql` en `app/sql` copiando
  `app/sql/local.sql` si existe; fija `migration_pending = true` y grupo
  `"LocalWP"`.
- PHP/MySQL se ajustan a versiones soportadas (`pick_supported`); la nota
  llega a la UI como `ImportResult.note`.

## Variantes

- **Pendiente de migración** (`migration_pending`): `site_status` lo pinta
  como `SiteStatus::MigrationPending` (en `types.ts` →
  `'migrationPending'`). La UI muestra un botón "Migrar y encender" gatilla
  `migrate_site` (ver ficha 04).
- **Worktree-project** (rama de un theme/plugin en aislamiento): ver
  `worktree::create_worktree`. El `SiteConfig.worktree_of` cambia el
  comportamiento de `docker::create_php_container` (monta el `public` del
  padre y sobrepone solo el worktree) y el de `nginx::render_vhost` (root
  del padre + `alias` para el repo objetivo).
- **Clone temporal** (`snapshot::create_snapshot` + `clone::create_clone`):
  ver ficha 06. `SiteConfig.clone_of` cambia el vhost para que
  `/wp-content/uploads/` se sirva primero del nuevo y haga fallback al
  padre (`try_files $uri @uploads_base`).
- **Proyecto apagado con datos**: `start_site` reusa el container existente
  (no lo recrea) si la imagen coincide; ver `docker::container_image`.
- **Imagen php nueva** (`IMAGE_REV` subido): `docker::start_site` detecta que
  el container tiene otro tag, lo borra con `force: true` y lo recrea
  (`docker::create_php_container`).

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `~/panel-wp/{slug}/config.json` | `config::load_all_sites`, `find_site`, `read_site_config` | `write_site_config` |
| `~/panel-wp/{slug}/app/public` | bind ro en `php-fpm` / `panel-nginx` | `wordpress::download_core`, `localwp::cp_contents` |
| `~/panel-wp/{slug}/ssl/{cert,key}.pem` | bind ro en `panel-nginx` (vhost) | `ssl::generate` (mkcert) |
| `~/panel-wp/{slug}/conf/php/php.ini` | bind ro en `php-fpm` (`zz-project.ini`) | `wordpress::write_php_ini`, `repair_all_php_ini` |
| `~/panel-wp/{slug}/app/wp-config.php` | bind ro en `php-fpm` (WordPress runtime) | `wp_config_create` (`wp config create`) |
| `~/.config/wordpress-panel/panel.json` | `PanelConfig.endpoint` | `save_endpoint`, `clear_endpoint` |
| `~/.config/wordpress-panel/db-data/{container}` | bind ro en MySQL/MariaDB/Postgres | `docker::ensure_db` (bind mount) |
| `~/.config/wordpress-panel/nginx/conf.d/{id}.conf` | `panel-nginx` (bind ro) | `nginx::write_vhost` / `remove_vhost` |
| `~/.config/wordpress-panel/groups.json` | (no aplica en este flujo) | `groups::create/rename/delete/reorder` |

## Containers / servicios

- `panel-net` (bridge): prerequisito (`docker::ensure_network`).
- `panel-{mysql|mariadb|postgres}-{ver}`: DB compartida on-demand (bind
  `config_dir/db-data/{container}` → `DbType::datadir()`).
- `panel-nginx`: reverse-proxy compartido on-demand; publica 80/443 (o
  puertos alternos) en `loopbackIp`.
- `panel-mailpit`, `panel-minio`, `panel-adminer`: ver ficha 05.
- `wp-{site-id}`: container php-fpm del proyecto. La imagen
  `panel-php:{ver}-{r3}` se construye con `docker build` (excepción al
  "Docker solo vía bollard") en `php::ensure_php_image`. Bind mounts:
  `public/` → `/var/www/html`, `php.ini` → `/usr/local/etc/php/conf.d/zz-project.ini`,
  `wp-cli.phar` → `/usr/local/bin/wp`. NO publica puertos.
- Containers auxiliares (adminer, mailpit, minio) y la DB se apagan solos al
  parar el último proyecto (`teardown_unused_shared`).

## Fallos y compensaciones

- **Container php con tag antiguo**: `docker::start_site` lo recrea a la
  fuerza (`remove_container force:true`) y crea uno nuevo.
- **Puerto host ocupado**: `preflight_endpoint` falla ANTES de levantar
  `panel-nginx` con un mensaje que nombra al proceso que lo retiene
  (`netcheck::holder_name`). Si el endpoint persistido quedó inservible,
  `docker::ensure_nginx` re-elige un par libre (`autoselect_endpoint`) y lo
  re-persiste.
- **DB no lista**: `wait_db_ready` sondea `mysql -h127.0.0.1 ...` cada 500 ms
  hasta 60 s. Falla con error claro si MySQL no abrió TCP (init
  `--skip-networking`).
- **Container DB legado sin bind**: `db_has_volume` distingue nuestro bind
  del `VOLUME` anónimo de la imagen; `migrate_db_to_volume` usa `docker cp`
  (excepción puntual) para extraer el datadir al host antes de recrear el
  container con bind.
- **ngnix zombie tras apagón sucio**: `docker::reload_nginx` detecta
  `exec` fallido (setns/nsexec) y recrea `panel-nginx` (un start limpio
  relee todo `conf.d`, equivale al reload).
- **Borrado sin carpeta**: el contenedor se quita antes de tocar el
  filesystem; si el borrado se cancela, el container ya no existe pero la
  carpeta sigue → la UI reintenta limpiamente.
- **Import de LocalWP sin `app/sql/local.sql`**: la nota (`note`) avisa al
  usuario y el proyecto queda `migration_pending` con esquema por crear.
- **Versión PHP/MySQL no soportada**: `pick_supported` usa la más reciente
  del whitelist y la nota avisa del ajuste.
- **Crash al crear** (entre paso 1 y 8): `delete_site` lo limpia todo
  porque (a) `config.json` se escribe después de carpetas, (b) el container
  se crea en `start_site` y se puede borrar con `remove_container`.

## UI / IPC / CLI / MCP disponibles

### IPC (Tauri commands en `src-tauri/src/lib.rs`)

- `get_sites` → `Vec<SiteState>` (config + status).
- `create_site(NewSiteRequest)` → `SiteConfig`.
- `start_site(id)` → `()`. Activa el watcher de auto-dump.
- `stop_site(id)` → `()`. Desactiva el watcher, export-al-detener, teardown.
- `stop_all_sites()` → `()`.
- `delete_site(id, deleteFolder)` → `()`.
- `migrate_site(id)` → `Migration` (migración de carpeta traída de otro sistema).
- `list_disconnected_sites()` → `Vec<DisconnectedSite>`.
- `import_disconnected_site(folderName)` → `ImportResult`.
- `list_localwp_sites()` → `Vec<LocalSite>`.
- `import_localwp_site(id)` → `ImportResult`.
- `panel_endpoint()` → `Endpoint` (URL effective).
- `system_status()` → `SystemStatus` (checklist de prerequisitos).
- `create_panel_network()` → `()`.
- `reset_endpoint()` → `()` (olvida el endpoint persistido).
- `open_admin(id, userId?)` → `()` (también relevante aquí).
- `open_site(id)`, `open_folder(id)`, `open_terminal(id)` → `()`.
- `repair_autologin(id)` → `SiteConfig` (re-inyecta mu-plugins del panel).
- `repair_all_php_ini()` → `String` (regenera `php.ini` en todos los proyectos).
- `stream_logs(id)` / `stop_logs(id)` → `()` (suscripción a `log:{id}`).

### UI (`src/routes/`, `src/lib/components/`)

- `/` master-detail (`src/routes/+page.svelte` + `ProjectDetail.svelte`):
  lista, encender, parar, borrar (modal `DeleteProjectModal.svelte` con
  ventana de gracia de 5 s y botón "Cancelar borrado").
- `/site/new` (`src/routes/site/new/+page.svelte`): alta con selección de
  PHP, motor DB, versiones WP (cache 24 h
  `config_dir/wp-versions.json`, `wordpress::fetch_versions`).
- `/site/[id]` (`src/routes/site/[id]/+page.svelte`): wrapper para deep-links.
- `/import-localwp` (`src/routes/import-localwp/+page.svelte`): lista sitios
  `~/.config/Local/sites.json` y deja el proyecto como pendiente.
- Modal `import-localwp` exportada del modal `ImportProjectModal.svelte`
  (re-importar carpetas desconectadas).
- `/services` (`src/routes/services/+page.svelte`): botones para abrir
  Mailpit, MinIO, instalar wrappers.
- `/domains` (`src/routes/domains/+page.svelte`): listado de dominios (informativo).
- `/settings` (`src/routes/settings/+page.svelte`): checklist de sistema +
  reset endpoint + repair php.ini.
- `/dumps` (`src/routes/dumps/+page.svelte`): log de volcados (ver ficha 06).
- `/cli` (`src/routes/cli/+page.svelte`): referencia para `wordpress-panel-cli`.

### CLI (`scripts/wordpress-panel-cli.sh`)

Subcomandos que tocan el ciclo de vida:

- `list | ls` — lista todos los proyectos con estado.
- `start [proyecto]` — enciende (id o nombre por subcadena).
- `stop [proyecto]` — apaga.
- `open admin | site | front | folder` — abre en el navegador / explorador.
- `containers` — lista contenedores del proyecto.
- `resources` — `docker stats --no-stream`.
- `logs [servicio] [-f] [-n N]` — logs de un container

`start`/`stop`/`open` hablan con el panel **en ejecución** por D-Bus
(`com.goldmediatech.WordpressPanel`).

### MCP (`mcp/server.mjs`)

- `list_projects`, `start_project`, `stop_project`.
- `open_project { what: admin | site | folder }`.
- `project_containers`, `project_resources`, `project_logs`.

## Tests

### Unit tests (rápidos, sin Docker)

- `backup::tests::rotate_conserva_los_n_mas_recientes_e_ignora_ruido` — la
  rotación de dumps solo borra `db-*.sql` y deja `imported.sql`, `local.sql`.
- `backup::tests::rotate_no_borra_si_hay_menos_o_igual_que_keep`.
- `config::tests::container_name_y_sql_dir`, `site_url_cuatro_ramas`,
  `github_normalize_pliega_legacy_en_repos`, `clone_info_serializa_en_camelcase`,
  `endpoint_serializa_en_camelcase`, `siteconfig_roundtrip_camelcase`.
- `clone::tests::find_free_slot_base_libre`, `find_free_slot_evita_colision_path`,
  `find_free_slot_evita_colision_dominio`, `slugify_etiquetas`.
- `wordpress::tests::slugify_basico`, `slugify_simbolos_y_trim`,
  `slugify_colapsa_separadores`, `slugify_alfanumerico_unicode`.
- `localwp::tests::major_minor_recorta_patch`, `pick_supported_soportada_sin_ajuste`,
  `pick_supported_no_soportada_usa_mas_reciente`.
- `netcheck::tests::v4_little_endian`, `listen_addr_matches_port_and_state`,
  `free_for_semantics`.
- `nginx::tests::vhost_normal_sin_uploads_block`,
  `vhost_clone_incluye_uploads_fallback_http` y `..._ssl`,
  `vhost_worktree_root_padre_y_alias_objetivo`.

### Integration tests (`cargo test -- --ignored --test-threads=1`)

- `integration_tests::import_disconnected_marks_pending` valida el ciclo
  básico de re-importación con `tauri::test::mock_app()`.
- `groups::tests::*` (marcados `#[ignore]`): mutan `config_dir()` real.

## Limitaciones

- El borrado NO es recuperable: la base de datos y la carpeta (si se
  consintió) se eliminan. El último cartucho es la carpeta desconectada
  (`config.disconnected.json`) o un snapshot (ver ficha 06).
- `start_site` no recrea la DB si la imagen cambió; solo la imagen php
  cambia se gestiona (container recreado).
- `delete_site` con `deleteFolder=false` deja la carpeta pero pierde la
  conexión a la DB compartida (la DB NO se recrea al re-importar —
  `migrate_site` la importa desde `imported.sql` o el último dump en
  `app/sql/`).
- `wsl`/entornos sin `docker` en PATH: `import_localwp_site` usa `cp -a`
  (`std::process::Command::new("cp")`), no bollard.

## Invariantes a NO romper

- **Container por proyecto NO publica puertos al host** — solo `panel-nginx`
  lo hace (vhost + `fastcgi_pass`).
- Naming: `wp-{site-id}` (proyecto) / `panel-*` (compartido).
- `SiteConfig.path` inmutable mientras el proyecto exista; cambiar `path`
  implica flujo desconectar → re-importar.
- `config.json` fuente de verdad. `load_all_sites` escanea cada vez; no hay
  registro central.
- `delete_site` con `deleteFolder=false` se hace con `rename` (atómico),
  nunca con `write` de un `config.json` vacío: la transferencia queda
  100% reversible.
- `teardown_unused_shared` se llama SIEMPRE después de un stop; nunca
  dejar contenedores corriendo por inercia.
- `worktree_of` y `clone_of` alteran el vhost y los mounts: `nginx::render_vhost`
  y `docker::create_php_container` los respetan. Borrar un proyecto que sea
  worktree/clone cae en `delete_site` y limpia con normalidad.

## Recomendaciones breves (rebuild)

- Reusar `wordpress::slugify` al construir paths; no inventar normalizaciones.
- Para borrar UI: `delete_site(id, deleteFolder)` directo desde el modal
  (no delegar en cleanup posterior). El sidecar `.disconnected.json` debe
  llamarse EXACTAMENTE así (lo lee `config::disconnected_config_path`).
- Al re-importar, **siempre** dejar `migration_pending = true` y `last_migrated_at = None`;
  la migración lo confirma.
- Los containers compartidos deben morir con el último proyecto activo
  (`teardown_unused_shared`) — NO dejarlos prendidos "por si acaso".
- `domain::ensure_wildcard` debe llamarse en cada `start_site` para
  reapuntar dnsmasq si alguien lo desactivó.

## Fuentes primarias

- `src-tauri/src/lib.rs` — comandos `create_site`, `start_site`, `stop_site`,
  `stop_all_sites`, `delete_site`, `import_disconnected*`, `import_localwp_site`,
  `open_admin`, `open_site`, `open_folder`, `open_terminal`, `repair_autologin`,
  `repair_all_php_ini`, `stream_logs`, `stop_logs`, `panel_endpoint`,
  `system_status`, `create_panel_network`, `reset_endpoint`.
- `src-tauri/src/docker.rs` — `DockerManager::connect/start_site/stop_site/teardown_unused_shared/ensure_db/ensure_nginx/reload_nginx/preflight_endpoint`.
- `src-tauri/src/wordpress.rs` — `create_site`, `create_dirs`, `write_php_ini`,
  `download_core`, `sync_mu_plugins`, `inject_*_muplugin`, `slugify`. `wp_config_create`
  (`wp config create`), `wp_core_install` (`wp core install`).
- `src-tauri/src/php.rs` — `ensure_php_image`, `wp_cli_phar_path`,
  `IMAGE_REV`.
- `src-tauri/src/config.rs` — `SiteConfig`, `SiteStatus`, `load_all_sites`,
  `find_site`, `write_site_config`, `projects_root`, `config_dir`,
  `disconnected_config_path`, `list_disconnected_sites`, `parse_db_name`.
- `src-tauri/src/nginx.rs` — `write_vhost`, `remove_vhost`, `render_vhost`,
  `ensure_tuning`.
- `src-tauri/src/ssl.rs` — `generate`, `has_cert`.
- `src-tauri/src/migrate.rs` — `migrate_site`, `latest_dump`, `fix_site_url`.
- `src-tauri/src/localwp.rs` — `list_sites`, `import_site`, `cp_contents`,
  `pick_supported`.
- `src-tauri/src/system.rs` — `status`, `mkcert_ca_installed`, `wrapper_installed`,
  `plasmoid_installed`.
- `src-tauri/src/domain.rs` — `wildcard_active`, `resolves_to`, `ensure_wildcard`,
  `install_wildcard`.
- `src-tauri/src/netcheck.rs` — `port_status`, `pick_alt_port`, `holder_name`.
- `src-tauri/src/cli.rs` — `install_cli_wrapper`, `open_terminal_at`.
- `src-tauri/src/worktree.rs` — `create_worktree`, `remove_worktree`.
- `src-tauri/src/snapshot.rs` — `create_snapshot`, `list_snapshots`,
  `delete_snapshot`.
- `src-tauri/src/clone.rs` — `create_clone`.
- `src-tauri/src/logs.rs` — `spawn_stream`, `event_name`.
- `scripts/wordpress-panel-cli.sh` — subcomandos `list`, `start`, `stop`,
  `open`, `containers`, `resources`, `logs`.
- `scripts/wp-wrapper.sh` — wrapper `wp` para terminal.
- `mcp/server.mjs` — herramientas `list_projects`, `start_project`,
  `stop_project`, `open_project`, `project_containers`,
  `project_resources`, `project_logs`.
- `src/lib/api.ts` — espejo de los comandos IPC.
- `src/lib/types.ts` — `SiteConfig`, `SiteState`, `SiteStatus`,
  `NewSiteRequest`, `DisconnectedSite`, `LocalSite`, `ImportResult`,
  `Endpoint`, `SystemStatus`.
- `src/lib/components/ProjectDetail.svelte`, `DeleteProjectModal.svelte`,
  `ImportProjectModal.svelte`, `OpConsole.svelte`.
- `src/routes/+page.svelte`, `src/routes/site/new/+page.svelte`,
  `src/routes/site/[id]/+page.svelte`, `src/routes/import-localwp/+page.svelte`,
  `src/routes/services/+page.svelte`, `src/routes/domains/+page.svelte`,
  `src/routes/settings/+page.svelte`.
