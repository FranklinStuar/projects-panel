# 05 — Mailpit, MinIO, Adminer y servicios compartidos

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para los contenedores compartidos
> on-demand (correo capturado, S3 local, visor de DB) y el wrapper
> `wp`/`wordpress-panel-cli` para la terminal.

## Resultado para el usuario

Mailpit captura el correo saliente de TODOS los proyectos activos, con
filtro `X-Project-ID` por header. MinIO provee S3 local bajo demanda
(activado por proyecto). Adminer es un visor de DB con auto-login en cero
clics (credenciales del entorno). El wrapper `wp` permite usar WP-CLI desde
la terminal del sistema, detectando el proyecto por el directorio actual.
Los wrappers se instalan una vez al arrancar el panel y bajo demanda.

## Precondiciones

- `panel-net` creado (`docker::ensure_network`).
- `panel-nginx` corriendo (no es estrictamente necesario para abrir Mailpit
  / MinIO / Adminer, pero `open_mailpit` exige `is_running(panel-mailpit)`).
- Daemon de Docker accesible.
- Para Mailpit: imagen `axllent/mailpit:latest`.
- Para MinIO: imagen `minio/minio:latest` + `config_dir/minio-data/`
  (auto-creado).
- Para Adminer: imagen `adminer:4` + `docker/adminer/autologin.php` (plugin
  PHP montado en el container).
- Para wrappers: `~/.local/bin` en el `PATH` (el instalador avisa si no).

## Flujo feliz

### A. Mailpit (correo capturado)

1. `docker::ensure_mailpit`:
   - Si `is_running(panel-mailpit)` → no-op.
   - Si existe pero parado → `start_container`.
   - Si no existe:
     - `ensure_image("axllent/mailpit:latest")`.
     - `HostConfig { network_mode: panel-net, port_bindings: 127.0.0.1:8025→8025/tcp }`.
     - SMTP interno `:1025` (no expuesto al host, solo `panel-net`).
     - `create_container` + `start_container`.
2. `docker::start_site` (proyecto) llama `ensure_mailpit` después de la
   DB. Mientras haya al menos un proyecto activo, mailpit queda prendido.
3. `docker::teardown_unused_shared` lo apaga cuando no queda ningún
   proyecto activo.
4. `wordpress::inject_mailpit_muplugin` inyecta
   `panel-mailpit.php` con `PHPMailer`:
   - `Host = 'panel-mailpit'`, `Port = 1025`, `SMTPAuth = false`.
   - `addCustomHeader('X-Project-ID', '{site.id}')` (placeholder
     `__PROJECT_ID__` reemplazado por `site.id`).
5. `open_mailpit` (command en `lib.rs`):
   - `is_running(panel-mailpit)` → `127.0.0.1:{MAILPIT_UI_PORT=8025}/`.
   - Errores: `"Mailpit no está corriendo (enciende algún proyecto)"`.

### B. MinIO (S3 local compartido)

1. `docker::ensure_minio`:
   - Si corriendo → no-op.
   - Si existe pero parado → `start_container`.
   - Si no existe:
     - Crea `config_dir/minio-data/`.
     - `cmd = ["server", "/data", "--console-address", ":9001"]`.
     - `env: ["MINIO_ROOT_USER=panel", "MINIO_ROOT_PASSWORD=panel-secret"]`.
     - `host_port_map: 127.0.0.1:9100→9000/tcp (API), 127.0.0.1:9101→9001/tcp (consola)`.
     - `bind_mount: {config_dir}/minio-data:/data`.
     - `create_container` + `start_container`.
2. `site.minio` (config) activa el servicio para un proyecto:
   - `start_site` lo arranca si `site.minio`.
   - `set_site_minio(id, enabled)` actualiza la config y lo arranca si
     está encendido.
   - `teardown_unused_shared` lo apaga si ningún proyecto con `minio` está
     activo.
3. `open_minio` (command en `lib.rs`): `127.0.0.1:{MINIO_CONSOLE_PORT=9101}/`.
   - Errores: `"MinIO no está corriendo (actívalo en un proyecto activo)"`.

### C. Adminer (visor de DB con auto-login)

1. `docker::ensure_adminer`:
   - Imagen `adminer:4`.
   - Bind mount `docker/adminer/autologin.php → /var/www/html/plugins-enabled/autologin.php:ro`.
   - `host_port_map: 127.0.0.1:8088→8080/tcp`.
   - `network_mode: panel-net` (habla con los containers DB por hostname).
   - `create_container` + `start_container`.
2. `open_adminer(id)` (command en `lib.rs`):
   - `is_running(panel-adminer)` necesario.
   - `is_running(db_container)` necesario (la DB del proyecto debe estar
     encendida).
   - `ensure_adminer` (lo arranca).
   - **Parámetros de URL**:
     - MySQL / MariaDB: `?server={db_container}&username=root&db={db_name}`.
     - Postgres: `?pgsql={db_container}&username=panel&db={db_name}`.
   - `http://127.0.0.1:{ADMINER_UI_PORT=8088}/?server=panel-mysql-80&…`.
3. **Auto-login** (`docker/adminer/autologin.php`):
   - Solo en GET sin `$_POST['auth']` y con `$_GET['username']` set.
   - Inyecta `$_POST['auth'] = { driver, server, username, password='panel', db }`.
   - Token cualquiera no vacío (`$_POST['token'] = "1"`); Adminer lo
     sustituye por el válido internamente.
   - Resultado: login en cero clics.
   - En POST (ejecutar SQL, etc.) NO inyecta — el usuario ya está
     autenticado por la cookie de sesión.

### D. Wrappers de terminal

1. `cli::install_cli_wrapper` (idempotente):
   - `~/.local/bin/wordpress-panel-cli` ← `scripts/wordpress-panel-cli.sh`.
   - `~/.local/bin/wp` ← `scripts/wp-wrapper.sh`.
   - `chmod 0o755`.
   - Si `~/.local/bin` no está en `PATH`, devuelve un aviso con la línea
     de export.
2. `lib.rs::run` invoca `install_cli_wrapper` al arrancar el panel (best-effort).
3. `install_cli_wrapper` (command Tauri) muestra un mensaje con las rutas
   instaladas o un error.
4. `wp-wrapper.sh`:
   - `PWD → wordpress-panel-cli detect-project $PWD` → `PROJECT_ID`.
   - `docker exec -i --user www-data wp-${PROJECT_ID} php /usr/local/bin/wp
     --path=/var/www/html "$@"`.
   - Falla con `"wp: no se detectó ningún proyecto Panel WP en $PWD"` si
     no encuentra proyecto.
5. `wordpress-panel-cli.sh`:
   - `detect-project`, `snapshot`, `git`, `worktree`, `list`, `start`,
     `stop`, `open`, `containers`, `resources`, `logs`.
   - Habla con el panel **en ejecución** por D-Bus
     (`com.goldmediatech.WordpressPanel`); si no, `require_panel` falla.
   - `project_for(CWD)` itera `~/panel-wp/*/config.json` y busca
     `path`/`id` por coincidencia de prefijo.
6. `cli::open_terminal_at(path)`:
   - Intenta `konsole --workdir`, `gnome-terminal --working-directory=…`,
     `xfce4-terminal --working-directory=…`, `kitty --directory`,
     `alacritty --working-directory`, `x-terminal-emulator`.
   - El primero que exista se lanza (detached).
   - Error si no hay ninguno.
7. `open_terminal(id)` (command Tauri):
   - `install_cli_wrapper` (idempotente).
   - `open_terminal_at(site.path)`.

### E. Apertura desde el panel

- `open_admin(id, userId?)` — auto-login al wp-admin (ver ficha 02).
- `open_site(id)` — frontend (`http(s)://{domain}(:{port})`).
- `open_folder(id)` — explorador del sistema en `site.path`.
- `open_terminal(id)` — terminal con `wp`.

## Variantes

- **Proyecto sin MinIO**: `site.minio = false` (default). `set_site_minio`
  lo activa por checkbox en la UI.
- **Adminer y DB apagadas**: `open_adminer` devuelve error
  `"La base de datos no está corriendo (inicia el proyecto primero)"`.
- **DB Postgres**: Adminer detecta `?pgsql=…` y el driver Postgres en
  lugar de MySQL.
- **Wrapper `wp` en terminal**: solo funciona si el panel está abierto
  (D-Bus respondiendo). El `wp` en sí solo necesita el container PHP
  activo.
- **Panel nginx caído**: `open_site`/`open_admin` fallan (necesita el
  vhost). Mailpit/MinIO/Adminer se sirven en 127.0.0.1:port, no
  necesitan nginx.

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `config_dir/minio-data/` | bind ro en `panel-minio` (`/data`) | `docker::ensure_minio` (mkdir) |
| `config_dir/wp-cli.phar` | `php::wp_cli_phar_path` | descarga única |
| `~/.local/bin/{wp,wordpress-panel-cli}` | ejecutable | `cli::install_cli_wrapper` (chmod 0o755) |
| `docker/adminer/autologin.php` | bind ro en `panel-adminer` (`/var/www/html/plugins-enabled/autologin.php`) | — |
| `docker/mu-plugins/panel-mailpit.php` | bind ro en `wp-{id}` (carga por WP) | `wordpress::sync_mu_plugins` (re-escribe con `__PROJECT_ID__`) |
| Tabla `wp_options` (transients) | (en runtime) | `wp transient set panel_autologin_{token}` (autologin) |
| `http://127.0.0.1:8025/` | usuario | `open_mailpit` |
| `http://127.0.0.1:9101/` | usuario | `open_minio` |
| `http://127.0.0.1:8088/?server=…&username=…&db=…` | usuario | `open_adminer` |

## Containers / servicios

- `panel-mailpit` — axllent/mailpit. SMTP `:1025` solo en `panel-net`,
  UI `127.0.0.1:8025`.
- `panel-minio` — minio/minio. API `:9100` (loopback), consola `:9101`
  (loopback). Data en `config_dir/minio-data/`.
- `panel-adminer` — adminer:4. UI `127.0.0.1:8088`. Habla con los
  containers DB por `panel-net`. Plugin autologin
  `docker/adminer/autologin.php`.
- `panel-nginx` — vhost (ver ficha 03).
- `wp-{site-id}` — php-fpm con `panel-mailpit.php` mu-plugin (si
  `one_click_admin` también `panel-autologin.php`).

## Fallos y compensaciones

- **Mailpit no creado**: `start_site` tolera el error (`.ok()`) — un
  fallo al crear Mailpit no debe bloquear el proyecto.
- **MinIO no creado**: igual, tolerante.
- **Adminer no creado**: `open_adminer` falla con error claro; no se
  inicia automáticamente (es on-demand).
- **DB Postgres y Adminer**: el driver se autodetecta por la presencia
  de `?pgsql=`.
- **D-Bus no disponible**: el panel sigue funcionando (solo sin
  plasmoid). El CLI falla con mensaje claro.
- **Wrappers no en PATH**: `install_cli_wrapper` avisa al usuario con
  `export PATH="$HOME/.local/bin:$PATH"`.
- **Plugin autologin de Adminer no se carga**: el plugin lo monta
  `docker::ensure_adminer` vía bind mount; si adminer se creó con una
  versión vieja, el plugin no estará. Solución: borrarlo
  (`remove_container`) y reabrir.
- **`wp` no detecta proyecto**: falla con `wp: no se detectó ningún
  proyecto Panel WP en $PWD` (no es un error del panel).

## UI / IPC / CLI / MCP disponibles

### IPC (`lib.rs`)

- `open_mailpit()` — abrir UI Mailpit.
- `open_minio()` — abrir consola MinIO.
- `open_adminer(id)` — abrir Adminer con auto-login al esquema del proyecto.
- `set_site_minio(id, enabled)` — activar/desactivar MinIO para un proyecto.
- `install_cli_wrapper()` — (re)instalar wrappers `wp`/`wordpress-panel-cli`.
- `open_admin(id, userId?)` — auto-login al admin.
- `open_site(id)` — frontend.
- `open_folder(id)` — explorador.
- `open_terminal(id)` — terminal con `wp`.

### UI (`src/routes/`, `src/lib/components/`)

- `/services` (`src/routes/services/+page.svelte`):
  - "Abrir" → `open_mailpit`.
  - "Abrir consola" → `open_minio`.
  - "Instalar" → `install_cli_wrapper`.
- Tab `svc` del proyecto (`ProjectDetail.svelte`):
  - "Ver base de datos (Adminer)" → `open_adminer(id)`.
  - "Exportar base de datos" → `export_db`.
  - "Abrir Mailpit (correo)" → `open_mailpit`.
  - Checkbox "MinIO" → `set_site_minio(id, …)`.
  - "Abrir consola MinIO" → `open_minio` (solo si `site.minio`).
  - "Abrir terminal del proyecto" → `open_terminal(id)`.
  - "Solo instalar wrapper `wp`" → `install_cli_wrapper`.
- Botones en el detalle: "Abrir admin", "Abrir frontend", "Abrir carpeta".
- `/settings` — checklist de wrappers (`wrapper_installed`).
- `/cli` — referencia de `wordpress-panel-cli`.

### CLI (`scripts/wordpress-panel-cli.sh`)

- `wordpress-panel-cli open admin` → `open_admin`.
- `wordpress-panel-cli open site` → `open_site`.
- `wordpress-panel-cli open folder` → `xdg-open $ppath`.
- `wordpress-panel-cli --help` → ayuda completa.
- `wp {…}` desde la terminal del proyecto.

### MCP (`mcp/server.mjs`)

- `open_project { what: admin | site | folder }`.

## Tests

- `integration_tests::import_disconnected_marks_pending` (cubre el camino
  D-Bus básico del CLI).
- `groups::tests::*` (marcados `#[ignore]`, mutan `config_dir` real).
- `config::tests::site_url_cuatro_ramas` (ya cubierto en ficha 03).

## Limitaciones

- **Mailpit solo**: el panel no incluye SMTP real (todos los proyectos
  van a Mailpit). Salir de Mailpit requiere editar el mu-plugin.
- **MinIO solo**: solo MinIO. No hay alternativa como SeaweedFS.
- **Adminer solo**: no hay phpMyAdmin / TablePlus / etc.
- **El plugin autologin de Adminer es solo para entorno local**: en
  producción NO usar Adminer con auth automática.
- **Wrappers usan D-Bus**: en entornos sin D-Bus (algunos WSL, headless
  servers), el `wordpress-panel-cli` falla con `require_panel`.
- **MinIO consola no tiene autenticación automática**: hay que teclear
  `panel / panel-secret` (no se inyecta).
- **`wp` solo funciona con el proyecto encendido**: WP-CLI requiere el
  container PHP.

## Invariantes a NO romper

- **`panel-mailpit` se arranca en `start_site`**, no en `create_site`,
  para no dejarlo prendido al crear un proyecto que no se enciende.
- **`teardown_unused_shared` apaga los 3 compartidos juntos**
  (`[NGINX, MAILPIT, ADMINER]`) si `!any_active`.
- **MinIO se apaga si `!any_minio`** (no si `!any_active`).
- **`panel-minio` se crea con `MINIO_ROOT_USER=panel`/password** — no
  rotar.
- **`panel-adminer` se monta en `127.0.0.1:8088`** — no exponer.
- **`panel-mailpit` expone solo SMTP en `:1025` a `panel-net`** — el puerto
  `:1025` no se publica al host.
- **Auth de Adminer es estática (`panel`)** — la "magia" del auto-login
  vive en `docker/adminer/autologin.php`.
- **El mu-plugin `panel-mailpit.php` SIEMPRE sustituye `__PROJECT_ID__`**
  por el id del proyecto, así el filtro `X-Project-ID` separa sobres por
  proyecto en Mailpit.

## Recomendaciones breves (rebuild)

- `panel-mailpit` se llama en `start_site` y se apaga junto con nginx
  en `teardown_unused_shared`.
- MinIO se crea bajo demanda SOLO si `site.minio`, y se apaga si
  `!any_minio`.
- Adminer es totalmente on-demand; crear al primer `open_adminer`.
- `cli::install_cli_wrapper` debe ser idempotente (puede llamarse
  múltiples veces).
- `open_mailpit` rechaza si `panel-mailpit` no está activo.
- `open_adminer` exige DB del proyecto corriendo.
- `wp-wrapper.sh` usa SIEMPRE `--user www-data` y
  `--path=/var/www/html`.

## Fuentes primarias

- `src-tauri/src/docker.rs` — `DockerManager::ensure_mailpit`,
  `ensure_minio`, `ensure_adminer`, `MAILPIT`, `MAILPIT_UI_PORT`,
  `MINIO`, `MINIO_API_PORT`, `MINIO_CONSOLE_PORT`, `ADMINER`,
  `ADMINER_UI_PORT`, `teardown_unused_shared`.
- `src-tauri/src/cli.rs` — `install_cli_wrapper`, `open_terminal_at`.
- `src-tauri/src/wordpress.rs` — `inject_mailpit_muplugin`,
  `inject_autologin_muplugin`, `sync_mu_plugins`.
- `src-tauri/src/autologin.rs` — `open_admin`.
- `docker/adminer/autologin.php` — plugin Adminer para auto-login.
- `docker/mu-plugins/panel-mailpit.php` — mu-plugin PHPMailer Mailpit.
- `docker/mu-plugins/panel-autologin.php` — mu-plugin one-click admin.
- `scripts/wp-wrapper.sh` — wrapper `wp`.
- `scripts/wordpress-panel-cli.sh` — wrapper CLI completo.
- `src/lib/api.ts` — `openMailpit`, `openMinio`, `openAdminer`,
  `setSiteMinio`, `installCliWrapper`, `openAdmin`, `openSite`,
  `openFolder`, `openTerminal`.
- `src/lib/components/ProjectDetail.svelte` — botones en tab `svc`.
- `src/routes/services/+page.svelte` — Mailpit, MinIO, instalar CLI.
- `src/routes/cli/+page.svelte` — referencia del CLI.
- `mcp/server.mjs` — `open_project`.
