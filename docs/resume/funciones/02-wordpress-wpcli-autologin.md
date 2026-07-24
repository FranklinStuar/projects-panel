# 02 — WordPress, WP-CLI y auto-login

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para instalar WordPress, ejecutar
> WP-CLI dentro del container y abrir el admin con un solo clic (token
> efímero de un solo uso).

## Resultado para el usuario

El usuario crea un proyecto o migra uno ya existente, edita/plugins/themes vía
el admin web (sin teclear contraseñas, gracias al one-click admin) o desde la
terminal con `wp plugin list`, `wp user list`, etc. Cada comando WP-CLI corre
dentro del container php-fpm del proyecto, con la MISMA versión de PHP que el
sitio, y como `www-data` (los archivos generados quedan propiedad del host
gracias al remapeo PUID/PGID del entrypoint).

## Precondiciones

- El proyecto debe estar **encendido** (`docker::is_running(site.container_name())`).
- El container php-fpm (`wp-{site-id}`) debe tener montados
  `php-cli.phar` (`php::wp_cli_phar_path` → `/usr/local/bin/wp`) y
  `app/public/` (`/var/www/html`).
- Si `one_click_admin` está activo, el mu-plugin `panel-autologin.php` debe
  estar inyectado en `app/public/wp-content/mu-plugins/`
  (`wordpress::inject_autologin_muplugin`).
- Para el admin vía UI: el endpoint del panel resuelto (`config::endpoint_or_default`)
  y la URL pública construida con `Endpoint::site_url(domain, ssl)`.

## Flujo feliz

### A. Instalación de WordPress (en `create_site`)

1. `wordpress::create_dirs` crea `app/public`, `app/sql`, `conf/php`, `logs/php`,
   `ssl`, `data`.
2. `wordpress::write_php_ini` clona `docker/php.ini.tmpl` (con `memory_limit`,
   `upload_max_filesize`, `post_max_size`, `opcache.enable`, etc.) a
   `conf/php/php.ini`.
3. `wordpress::download_core(version, public)` baja
   `https://wordpress.org/wordpress-{version}.tar.gz` y lo extrae con `tar
   --strip-components=1 -C public`.
4. `wordpress::sync_mu_plugins` reescribe (idempotente) `panel-mailpit.php`
   (siempre) y `panel-autologin.php` (si `one_click_admin`).
5. `ssl::generate` deja `cert.pem`/`key.pem` con mkcert.
6. `docker::start_site` enciende el container.
7. `wordpress::wp_config_create` corre `wp config create --dbname=… --dbuser=root
   --dbpass=panel --dbhost={db_container} --skip-check --force`.
8. `wordpress::wp_core_install` corre `wp core install --url={site_url}
   --title=… --admin_user=… --admin_password=… --admin_email=… --skip-email`.
9. Si `locale != en_US`, invoca `wp language core install {locale} --activate`.

### B. WP-CLI (cualquier operación)

1. `wpcli::run(docker, site, args)` exige `is_running(container_name)`.
2. Construye `cmd = ["php", "/usr/local/bin/wp", "--path=/var/www/html", args…]`.
3. `docker::exec_as(container, cmd, Some("www-data"))` con timeout
   `WPCLI_TIMEOUT = 120s` — WP-CLI load puede colgar por un plugin/mu-plugin
   que hace una llamada HTTP (UpdraftPlus, etc.).
4. `exec_as` devuelve stdout+stderr combinados; cualquier código de salida
   != 0 se convierte en `anyhow::Error` (no se traga en silencio).

### C. Auto-login al admin (one-click)

1. `open_admin` exige `is_running(container_name)`.
2. Reinyecta el mu-plugin en disco (`wordpress::inject_autologin_muplugin`) por
   si el proyecto se creó antes de que se añadiera el soporte de `user_id`.
3. Genera un token: `Uuid::new_v4().simple()`.
4. WP-CLI: `wp transient set panel_autologin_{token} {user_id} 60` (60 s de
   vida, un solo uso).
5. Abre `{base}/?panel_autologin={token}` en el navegador.
6. El mu-plugin valida:
   - `$_GET['panel_autologin']` saneado con `preg_replace('/[^a-z0-9]/i', '')`.
   - `get_transient("panel_autologin_{token}")` (WP transients = opciones con
     expiración).
   - Si existe, `delete_transient` (un solo uso) y `$user_id` se interpreta
     como `intval($stored)`. `0` o vacío → primer admin.
   - `wp_set_current_user` + `wp_set_auth_cookie(true, …)` + `wp_safe_redirect`
     a `admin_url()` si `manage_options`, si no a `home_url('/')`.

### D. Wrappers de terminal (`scripts/wp-wrapper.sh` + `scripts/wordpress-panel-cli.sh`)

- `~/.local/bin/wp` resuelve el proyecto por `PWD`
  (`wordpress-panel-cli detect-project`) y ejecuta
  `docker exec -i --user www-data wp-{PROJECT_ID} php /usr/local/bin/wp
   --path=/var/www/html "$@"`.
- `~/.local/bin/wordpress-panel-cli` reusa la lógica del panel por D-Bus
  (`com.goldmediatech.WordpressPanel`).
- Instalación idempotente al arrancar el panel (`lib.rs::run` →
  `cli::install_cli_wrapper`) y manual con `install_cli_wrapper`.

## Variantes

- **Sin one-click admin**: `open_admin` salta el bloque WP-CLI y abre
  `{base}/wp-admin/` directo (necesita login manual).
- **user_id específico**: `open_admin(id, Some(7))` loguea al usuario con id
  7 (mostrado en el selector del dropdown — `list_wp_users` los carga con
  `wp user list --fields=ID,user_login,display_name,roles --format=json`).
- **Idioma distinto de en_US**: `wp_core_install` invoca
  `wp language core install {locale} --activate` (best-effort, no aborta).
- **Migración** (`migrate_site`): `wp_core_install` NO se corre (ya existe DB);
  en su lugar `wp_config_create` + `fix_site_url` (con `--skip-plugins
  --skip-themes` para que un plugin colgado no bloquee la migración).
- **Worktree-project** (`worktree::create_worktree`): mismo `wp config create`
  + `wp config set WP_HOME / WP_SITEURL --type=constant` (si BD compartida,
  para no mutar la DB del padre).
- **Snapshot/clone**: `migrate::fix_site_url` también se corre aquí (clones
  cambian de dominio).
- **Repair-autologin** (`repair_autologin`): pensado para proyectos
  importados de LocalWP, que no traen los mu-plugins del panel. Fija
  `one_click_admin = true`, reescribe `config.json` y llama a
  `wordpress::sync_mu_plugins`.

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `~/panel-wp/{slug}/app/public/` | bind ro en `wp-{id}` (`/var/www/html`) | `download_core`, `cp_contents` |
| `~/panel-wp/{slug}/app/public/wp-config.php` | bind ro en `wp-{id}` | `wp config create` (WP-CLI), `wp config set WP_HOME/SITEURL` (worktree) |
| `~/panel-wp/{slug}/app/public/wp-content/mu-plugins/panel-{autologin,mailpit}.php` | bind ro en `wp-{id}` (carga por WP) | `sync_mu_plugins`, `inject_autologin_muplugin` |
| `~/panel-wp/{slug}/conf/php/php.ini` | bind ro en `wp-{id}` (`zz-project.ini`) | `write_php_ini`, `repair_all_php_ini` |
| `config_dir/wp-cli.phar` | bind ro en `wp-{id}` (`/usr/local/bin/wp`) | `wp_cli_phar_path` (descarga única) |
| `config_dir/wp-versions.json` | `wordpress::fetch_versions` (cache 24 h) | `fetch_versions` |
| Tabla `wp_options` (transients) | `get_transient` desde el mu-plugin | `wp transient set panel_autologin_{token}` (60 s) |
| Tabla `wp_options` (home/siteurl) | `wp option get home/siteurl` | `wp option update home/siteurl` (migración/clone) |
| `~/.local/bin/{wp,wordpress-panel-cli}` | ejecutable por el usuario | `cli::install_cli_wrapper` (idempotente) |

## Containers / servicios

- `wp-{site-id}` (php-fpm): el WP-CLI corre aquí con `docker::exec_as(...,
  "www-data")`. El uid se mapea al del host vía entrypoint (`PUID/PGID`).
- `panel-{db}-{ver}`: WP-CLI habla con la DB vía `--dbhost=panel-{db}-{ver}` y
  TCP 3306/5432.

## Fallos y compensaciones

- **WP-CLI excede 120 s**: `tokio::time::timeout` aborta con mensaje
  explícito ("¿un plugin/mu-plugin hace una llamada de red al cargar?"); el
  comando no se queda colgado para siempre.
- **Plugin/mu-plugin que falla al cargar**: `WPCLI_TIMEOUT` corta; en
  flujos críticos (migración, worktree), se añade `--skip-plugins
  --skip-themes` para saltarse plugins normales (los mu-plugins no se
  pueden saltar — se acota con el timeout).
- **`wp config create` falla porque ya existe `wp-config.php`**: lleva
  `--force` para reescribir.
- **`wp core install` con host inválido (cert autofirmado de MySQL 8)**: el
  dump sí se hace desde el **container DB** (socket local, sin TLS) vía
  `docker exec -i ... mysql` en `migrate::import_dump` (no por bollard, ver
  `CLAUDE.md`).
- **Token de auto-login expirado (>60 s) o ya usado**: el mu-plugin
  muestra wp-admin estándar (no error). El `delete_transient` asegura un
  solo uso.
- **Proyecto importado sin mu-plugins**: `one_click_admin` se reescribe a
  `true` y `sync_mu_plugins` se llama en `repair_autologin`.
- **`wp --path=/var/www/html` colisiona con un WordPress en esa ruta**:
  `--path` es fijo y `%PWD` se reemplaza por `--path` del container;
  siempre apunta al WP del proyecto.
- **Versión de WP no encontrada**: `fetch_versions` devuelve el cache
  aunque la API falle; si la versión concreta no está en la lista, el
  `download_core` recibe un 404 y devuelve un error claro.

## UI / IPC / CLI / MCP disponibles

### IPC (Tauri commands en `lib.rs`)

- `create_site(NewSiteRequest)` — (relacionado) crea + corre wp_config_create
  + wp_core_install.
- `exec_wpcli(id, args)` — `wp {args}` en el container del proyecto.
- `list_plugins(id)` / `list_themes(id)` — `wp plugin list --format=json` /
  `wp theme list --format=json`.
- `list_wp_users(id)` — `wp user list --fields=ID,user_login,display_name,roles
  --format=json`.
- `open_admin(id, userId?)` — abre el admin con auto-login.
- `open_site(id)` — abre el frontend (sin auto-login).
- `open_terminal(id)` — abre terminal con `wp` listo.
- `install_cli_wrapper()` — instala los wrappers `wp` y `wordpress-panel-cli`.
- `list_wp_versions()` — versiones WP con status (latest/outdated/insecure).
- `repair_autologin(id)` — re-inyecta mu-plugins + activa `one_click_admin`.
- `repair_all_php_ini()` — regenera `php.ini` desde el template actual.

### UI (`src/lib/components/ProjectDetail.svelte`)

- `openAdmin()` → `api.openAdmin(id, userId?)` con picker de usuarios
  (`loadWpUsers` carga desde `list_wp_users` cuando aba la pestaña y el
  proyecto está corriendo con `one_clickAdmin`).
- `openSite()` → `api.openSite(id)`.
- `openFolder()` → `api.openFolder(id)`.
- Consola de WP-CLI (botón "Abrir terminal del proyecto" en el tab `svc`).
- `OpConsole` recibe las líneas de `migrate_site`, `import_localwp_site`,
  `import_disconnected_site`, `delete_site` por el evento `op-log`.

### CLI (`scripts/wordpress-panel-cli.sh`)

- `wp {…}` (wrapper) — autodetecta el proyecto por `PWD`.
- `wordpress-panel-cli open admin` — equivalente a `open_admin`.
- `wordpress-panel-cli open site | front` — equivalente a `open_site`.
- `wordpress-panel-cli open folder` — abre explorador del proyecto.

### MCP (`mcp/server.mjs`)

- `open_project { what: admin | site | folder }` — equivalentes a los
  anteriores.

## Tests

### Unit tests

- `wordpress::tests::slugify_*` (básicos, símbolos/trim, colapsa separadores,
  alfanumérico unicode).
- `localwp::tests::major_minor_recorta_patch`, `pick_supported_*`.
- `config::tests::siteconfig_roundtrip_camelcase` — verifica `oneClickAdmin`,
  `xdebugEnabled`, `frontendFramework`, `migrationPending`, `lastMigratedAt`,
  `dbName` etc.

### Integration tests

- `integration_tests::import_disconnected_marks_pending` (con mock-app,
  cubre el camino completo hasta `migration_pending=true`).

## Limitaciones

- **Auto-login requiere proyecto ENCENDIDO** (`autologin::open_admin` falla
  con error claro si no).
- **WP-CLI no se puede correr si el container no está activo**
  (`wpcli::run` falla con "el proyecto 'X' no está encendido"). No hay
  fallback offline.
- **`wp-cli.phar` se descarga una vez al primer arranque** de un proyecto
  si no está en `config_dir`. Network failure → `wp_cli_phar_path` falla
  y no se pueden crear proyectos nuevos.
- **`wp language core install` es best-effort** — si falla no aborta la
  instalación. El sitio queda en `en_US` y el usuario debe traducirlo.
- **No hay "magic link" de WP-CLI**; el auto-login se implementa con un
  mu-plugin propio (transient + token). Esto requiere que el mu-plugin
  esté en disco; borrarlo o desactivarlo rompe el one-click admin.
- **Timeout WP-CLI es 120 s**. Operaciones legítimamente largas (resync
  de ElasticPress, etc.) podrían cortarse. Documentar / usuario debe
  invocarlas fuera del timeout.

## Invariantes a NO romper

- **WP-CLI DEBE correr como `www-data`** (root está prohibido por WP-CLI y
  rompe la propiedad de los archivos generados). `wpcli::run` lo fija con
  `exec_as(... Some("www-data"))`.
- `wp --path=/var/www/html` SIEMPRE: `wpcli::run` lo prepende.
- `wp config create` lleva `--force` para permitir reescritura idempotente
  en migración.
- `wp transient set panel_autologin_{token} … 60` SIEMPRE caduca a 60 s.
- El mu-plugin `panel-autologin.php` SIEMPRE sanea el token con
  `preg_replace('/[^a-z0-9]/i', '')` y borrra el transient tras usarlo.
- `sync_mu_plugins` SIEMPRE reescribe ambos mu-plugins (idempotente),
  incluidos los proyectos importados de LocalWP.
- `--skip-plugins --skip-themes` se añade en `migrate::fix_site_url` para
  no depender de plugins al ajustar URLs.

## Recomendaciones breves (rebuild)

- Centralizar TODA ejecución de WP-CLI a través de `wpcli::run` (no
  `docker exec` directo).
- Para translations, levantar siempre el `wp language core install` y
  hacerlo bloqueante (no `.ok()`).
- Para el autologin, guardar siempre el `userId` en localStorage
  (`wp-panel:autologin:{id}`) para no pedirselo al usuario en cada clic.
- En la UI, mostrar el dropdown de usuarios solo si `oneClickAdmin &&
  status === 'running'`.

## Fuentes primarias

- `src-tauri/src/wpcli.rs` — `run`, `WPCLI_TIMEOUT`.
- `src-tauri/src/autologin.rs` — `open_admin`.
- `src-tauri/src/wordpress.rs` — `create_site`, `wp_config_create`,
  `wp_core_install`, `sync_mu_plugins`, `inject_autologin_muplugin`,
  `inject_mailpit_muplugin`, `slugify`, `create_database`,
  `download_core`, `fetch_versions`.
- `src-tauri/src/php.rs` — `wp_cli_phar_path`, `IMAGE_REV`.
- `src-tauri/src/php.rs` — `IMAGE_REV = "r3"` (subir para forzar rebuild).
- `docker/mu-plugins/panel-autologin.php` — mu-plugin one-click.
- `docker/mu-plugins/panel-mailpit.php` — mu-plugin mailpit.
- `docker/php.ini.tmpl` — plantilla `php.ini`.
- `docker/php/Dockerfile` + `docker/php/entrypoint.sh` — imagen + entrypoint
  (PUID/PGID).
- `scripts/wp-wrapper.sh` — wrapper `wp` para terminal.
- `scripts/wordpress-panel-cli.sh` — `wp-cli` shell.
- `src/lib/api.ts` — `execWpcli`, `listPlugins`, `listThemes`, `listWpUsers`,
  `openAdmin`, `openSite`, `openTerminal`, `installCliWrapper`,
  `listWpVersions`, `repairAutologin`, `repairAllPhpIni`.
- `src/lib/components/ProjectDetail.svelte` — `openAdmin`, `openSite`,
  `openFolder`, `loadWpUsers`, `act`, listado de plugins y themes.
- `src/routes/cli/+page.svelte` — referencia del CLI.
- `mcp/server.mjs` — `open_project`.
