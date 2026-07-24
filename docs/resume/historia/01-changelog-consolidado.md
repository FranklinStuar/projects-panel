# Changelog consolidado

Esta hoja consolida el `docs/CHANGELOG.md` del repositorio en una vista de fases corregida. La regla es: **el código actual prevalece sobre la prosa histórica**. Si una entrada entra en conflicto con el estado presente del repo, se anota al pie como **contradicción resuelta**.

Origen: `git log main` y `docs/CHANGELOG.md`. Solo se listan los hitos que el repositorio conserva o cuyo rastro se documenta en código.

## Fase 1 — MVP Core

### Hecho

- **Scaffold frontend**: SvelteKit + Svelte 5 + `adapter-static` modo SPA (`ssr=false`, fallback `index.html`). Tailwind dark/light. Dashboard con start/stop y estado real. Stubs de dominios/settings/site.
- **Backend Tauri 2 + orquestación Docker** (`docker.rs`):
  - red `panel-net` (`docker::NETWORK`); servicios compartidos on-demand (DB por versión, nginx).
  - container php por proyecto sin puertos host.
  - teardown de compartidos cuando ningún activo los usa (`docker::teardown_unused_shared`): cero recursos con cero proyectos encendidos.
  - mapeo UID/GID host↔www-data (`docker::host_uid_gid` + entrypoint de `docker/php/Dockerfile`/`entrypoint.sh`); `exec` y `exec_as` (con chequeo de exit code, fija usuario).
- **`nginx.rs`**: vhosts generados (root ro en `/srv/projects`, FastCGI a `wp-{id}:9000` con `SCRIPT_FILENAME` en la vista del container php) + `nginx -s reload`. Tuning global (`server_names_hash_bucket_size 128`, `client_max_body_size 0`).
- **`php.rs`**: build de imagen `panel-php:{ver}-r3` desde Dockerfile + WP-CLI phar (`wp_cli_phar_path`).
- **`domain.rs`**: dnsmasq wildcard `*.test` (best-effort + `resolves_to`).
- **`wordpress.rs`**: `create_site` end-to-end (tarball, DB, wp-config, core install vía WP-CLI, mu-plugin mailpit, locale); `fetch_versions` (cache 24h).
- **`wpcli.rs`**: WP-CLI dentro del container; `wpcli::WPCLI_TIMEOUT` = 120 s para evitar mu-plugins colgantes.
- **Assets `docker/`**: Dockerfile php-fpm + entrypoint UID/GID, `php.ini.tmpl`, `nginx/vhost.conf.tmpl`, `mu-plugins/panel-mailpit.php`.
- **Formulario Nuevo Proyecto** cableado a `create_site` con versiones WP en vivo.
- **`scripts/first-run.sh`**: primera config idempotente (panel-net, dnsmasq wildcard vía NetworkManager, mkcert, wrappers WP-CLI, plasmoid).
- **Verificado live**: imagen php 107 MB; entrypoint remapea www-data→uid host; extensiones gd/intl/mysqli/pdo_mysql/zip/opcache (+WebP en `r3`); `*.test`→127.0.0.1 con DNS externo intacto; GUI arranca sin panics.

### Pendiente original

- WP-CLI wrapper instalado en `~/.local/bin/` + binario `wordpress-panel-cli`.
- Verificación end-to-end de una provisión completa de WordPress real.

> **Contradicción resuelta — Fase 1 sigue marcada como "en curso" en `docs/CHANGELOG.md` aunque Fases 2, 3 y 4 están completas y los pendientes originales están implementados**: la instalación de wrappers se ejecuta en el `setup` de Tauri (`lib.rs::run::setup` → `cli::install_cli_wrapper`) y la provisión end-to-end está cubierta por `wordpress::create_site` con la suite de tests de integración.

## Fase 2 — Funcionalidades completas

### Hecho

- **Logs en vivo** (`logs.rs`): stream `follow` de los logs del container → eventos `log:{id}`; tab "Logs" en la vista de proyecto con autoscroll del buffer (últimas 500 líneas). Atado al tab con `LogStreams`.
- **Auto-login one-click** (`autologin.rs` + mu-plugin `panel-autologin.php`): transient WP de 60 s, un solo uso; el mu-plugin aplica `wp_set_auth_cookie` y redirige adaptativamente (admin_url si `manage_options`, home si no). Inyectado por `wordpress::sync_mu_plugins` en `create_site` y al migrar.
- **Listado de plugins/themes** (`list_plugins`/`list_themes` vía WP-CLI JSON); tab en la vista de proyecto.
- **GitHub vía `gh`** (`github.rs`): clonar/pull/quitar theme y plugins en el host (bind-mount → cambios al instante), `gh_status`, registro en `config.json`; tab con UI de theme/plugins y "Pull todo".
- **SSL con mkcert** (`ssl.rs`): genera cert/key por dominio en creación si SSL activo; comando `regenerate_ssl`; CA local instalada vía `first-run.sh`. Botón "Regenerar SSL" en el menú "···".
- **Grupos de proyectos**: `set_site_group` + editor de grupo en Info (versión inicial). Reemplazado después por `groups.rs` + `groups.json` y drag&drop.
- **D-Bus + plasmoid KDE** (`dbus.rs` + `plasma/applets/wordpress-panel-plasmoid/`): servicio `com.goldmediatech.WordpressPanel` (GetRunningSites/StopSite/StopAll/Quit), arranca en el `setup`; plasmoid Plasma 6 que consulta vía qdbus6. Instalación: `kpackagetool6 --install` (lo hace `first-run.sh`).
- **Vista de proyecto con tabs** (Info / Logs / Plugins-Themes / GitHub) + start/stop.

### Pendiente

- Verificación visual del plasmoid en una sesión Plasma (no testeable headless).
- Botones de la barra de título (ver `docs/KNOWN_ISSUES.md`).

## Fase 3 — Servicios adicionales

### Hecho

- **Servicios compartidos on-demand** (`docker.rs::ensure_mailpit`, `ensure_minio`): `panel-mailpit` (`axllent/mailpit:latest`, UI 127.0.0.1:8025, SMTP `:1025` interno); `panel-minio` (`minio/minio:latest`, API 127.0.0.1:9100, consola 9101, datos en `~/.config/wordpress-panel/minio-data`). Mailpit arranca con cualquier proyecto activo; MinIO solo si el proyecto tiene flag `minio`. `teardown_unused_shared` los apaga si nadie los usa.
- **MariaDB y PostgreSQL**: soportados a nivel de infra (`DbType` → imagen, `db_env`, `create_database` con rama SQL por motor). Selector en el formulario.
- **Backup** (`backup.rs`): `export_db` vía `wp db export` y movido a `app/sql/db-{timestamp}.sql`. Comando `export_db`.
- **Wrapper WP-CLI** (`cli.rs` + `scripts/wp-wrapper.sh` + `scripts/wordpress-panel-cli.sh`): `install_cli_wrapper` copia a `~/.local/bin/`. `wp` corre con `--user www-data` (paridad con `exec_wpcli`).
- **Headless**: flags `headless` + `frontendFramework` en `NewSiteRequest`/`SiteConfig`; checkbox + selector de framework en el form. Diferido el container de frontend.
- **Botones stub** (`feature_stub`): Cloudflare Tunnel / Deploy / Empaquetado — UI preparada, lógica posterior.
- **Frontend**: tab **Servicios** en la vista de proyecto (backup, abrir Mailpit, toggle + abrir MinIO, instalar wrapper, stubs) y ruta **`/services`**. Flag `minio` en `SiteConfig`/types.
- **Comandos IPC nuevos**: `set_site_minio`, `export_db`, `install_cli_wrapper`, `open_mailpit`, `open_minio`, `feature_stub`.

### Pendiente real (diferido)

- Provisión de un container de frontend para proyectos headless.
- Plugin S3 que conecte WP a MinIO.

## Fix — Conflicto de puerto 80/443 del host (coexistencia con LocalWP)

- **`netcheck.rs`** (nuevo): lee `/proc/net/tcp{,6}` y clasifica cada puerto en `Free`/`Wildcard`/`Specific`. Detectaba el `0.0.0.0:80` de LocalWP. Selectores `pick_loopback_ip` y `pick_alt_port`; `holder_name` (best-effort, vía `/proc/<pid>/fd`).
- **`config.rs::Endpoint`** (nuevo): `loopbackIp`/`httpPort`/`httpsPort` global, persistido en `panel.json`. Helper `site_url` aplica el puerto solo si no es estándar. `autoselect_endpoint` elige el primer par libre desde 8080/8443; el plan original preveía un fallback de IP loopback alterna, pero el código actual siempre cede 80/443 y publica en puertos altos.
- **`docker.rs::ensure_nginx`**: preflight con `preflight_endpoint` (mensaje legible nombrando al proceso que ocupa el puerto en vez del 500 opaco). Recrea `panel-nginx` para arrastrar bindings viejos. Poda vhosts huérfanos antes de bindear (`prune_orphan_vhosts`) y expone `repair_nginx` (Fase 4+).
- **`domain.rs`**: regla dnsmasq parametrizada por IP; `resolves_to` + `install_wildcard` (pkexec) para la IP alterna.
- **URLs**: `wordpress.rs` (core install) y `autologin.rs` usan el endpoint. `panel_endpoint` + tipo TS `Endpoint`/`siteUrl`. Dashboard y detalle muestran el puerto cuando es alterno.

## Fix — Instalación de WordPress fallaba en silencio (sitio a medias)

- **`docker.rs::exec`**: `inspect_exec` chequea el exit code y devuelve `Err` con el output si ≠ 0.
- **WP-CLI corría como root**: `exec_as(user)`; `wpcli::run` y `backup` corren como `www-data` (paridad de permisos; los archivos quedan con el dueño del host vía remapeo uid).
- **`ensure_db` no esperaba readiness**: `wait_db_ready` (gate sobre TCP, timeout 60 s con `mysql -h127.0.0.1 …` o `pg_isready -h 127.0.0.1`).

## Diseño — Tema "DevFlow Dark Blue"

- Paleta navy de `DESIGN.md` en `tailwind.config.js` (escala `zinc` remapeada). Token `primary` (#4d8eff). `.input` global en `app.css`. `accent-color` azul para checkbox/radio. App dark-only navy.

## Fase 4 — Polish

### Settings completo (estado del sistema + primera configuración)

- **`system.rs`** (nuevo): `system_status` reúne en una lectura Docker, red, dnsmasq, mkcert, wrappers, plasmoid + endpoint y rutas. Best-effort.
- **Comandos IPC**: `system_status`, `create_panel_network` (crea `panel-net` si falta, reusa `ensure_network`), `reset_endpoint` (`config::clear_endpoint`).
- **UI** (`/settings`): checklist con semáforo; botones para acciones sin privilegios; las que requieren sudo (dnsmasq, mkcert CA, plasmoid) remiten a `scripts/first-run.sh`. Endpoint con badges "URLs limpias"/"puerto alterno" + botón "Reasignar puerto".

### Migración entre sistemas + export automático al detener

- **`migrate.rs`** (nuevo): `migrate_site` provisiona un proyecto `migrationPending` en el sistema actual. Pasos:
  1. Sincroniza mu-plugins (`wordpress::sync_mu_plugins`).
  2. `ensure_db` + `create_database` (idempotente).
  3. SSL si aplica (`ssl::generate`).
  4. Enciende php + vhost + nginx.
  5. Regenera `wp-config.php` con credenciales del panel.
  6. Importa el último dump de `app/sql/` con `import_dump` (ver más abajo).
  7. `fix_site_url` (`wp option update home/siteurl` con `--skip-plugins --skip-themes`).
  8. Marca `migration_pending = false`, `last_migrated_at = now()`.
- **Comando** `migrate_site(id) -> Migration`. Botón "Migrar y encender" en el dashboard y en el detalle del proyecto.
- **Export-al-detener**: `docker::stop_site` llama `backup::export_db` y `rotate_dumps(site, 3)` (deja los 3 `db-*.sql` más recientes; `imported.sql`/`local.sql` no se tocan).

### Importación desde LocalWP

- **`localwp.rs`** (nuevo): `list_sites` parsea `~/.config/Local/sites.json`. `import_site` copia `app/public` con `cp -a`, el dump como `imported.sql` (de `app/sql/local.sql`), mapea versiones PHP/MySQL a las soportadas y avisa si ajusta. Escribe `config.json` con `migrationPending=true` y grupo "LocalWP".
- **Comandos**: `list_localwp_sites`, `import_localwp_site`. Sección "Importar desde LocalWP" en `/settings`.
- **`migrate.rs::fix_site_url`**: tras importar el dump, fija `home`/`siteurl` al dominio del panel (`*.test`). Sin `search-replace` (limitación documentada).

### Re-importar proyectos desconectados

- **Conservar al desconectar**: `delete_site` con `deleteFolder=false` renombra `config.json` → `config.disconnected.json` (no lo borra). `load_all_sites()` solo escanea `config.json`, así que el panel la olvida; re-importar la restaura sin pérdida.
- **`config.rs::list_disconnected_sites`**: escanea `~/panel-wp/` y devuelve las carpetas sin `config.json`: con sidecar (`preserved`) o con `app/public/wp-config.php` (`reconstructed`).
- **Comandos**: `list_disconnected_sites`, `import_disconnected_site`. `import_disconnected_site` restaura/reconstruye `config.json` con `migrationPending=true`, regenera id si colisiona, fija la ruta actual y borra el sidecar.
- **Frontend**: botón "Importar proyecto" en el dashboard → `ImportProjectModal.svelte` (lista con badge `config conservada`/`reconstruido`, con/sin dump, progreso en `OpConsole`).
- **Test**: `integration_tests::list_e_import_disconnected_hermetico` (sin Docker).

### Empaquetado del plasmoid

- **`scripts/package-plasmoid.sh`** (nuevo): genera `dist/wordpress-panel.plasmoid` (zip de `metadata.json` + `contents/`). Idempotente.

### Fix — import/export de DB en el container DB (sin TLS)

- **Causa**: `wp db import/export` desde el container php fallaba: `env: can't execute 'mysql'` (sin cliente en la imagen) y luego `TLS/SSL error: self-signed certificate` (MySQL 8).
- **Solución**: import/export se ejecutan dentro del container DB (`panel-mysql-*`) por socket local, sin TLS.
  - `docker.rs`: `exec_stdin` (alimenta `mysql` por stdin → importar dump) y `exec_capture` (captura stdout de `mysqldump` → exportar). Helper `db_container_name`.
  - `migrate.rs::import_dump` usa `docker exec -i … mysql` (CLI, excepción justificada: bollard `exec_stdin` se cuelga con dumps grandes).
  - `backup.rs::export_db` con `mysqldump --single-transaction` vía `exec_capture`.
- La imagen php suma `mariadb-client` y el tag lleva revisión (`panel-php:{ver}-r3`); al subirla, `ensure_php_image` reconstruye y `start_site` recrea los containers con tag viejo (compara `container_image`).

### Fix — migración: generar SSL antes de encender

El vhost referencia `ssl/cert.pem`; `migrate_site` encendía el sitio antes de generar el cert → reload fallaba. Reordenado: `ssl::generate` antes de `start_site`.

### Consola de progreso + cancelar importación

- **Consola en vivo** (`progress.rs`, `OpConsole.svelte`): `progress::log` emite líneas de paso en `op-log`; `OpConsole` modal con autoscroll, "Cerrar" deshabilitado mientras corre.
- **Cancelar importación**: botón "Cancelar" en proyectos `migrationPending` (dashboard y detalle) → `delete_site` con `deleteFolder=true`. Para deshacer una importación con proyecto equivocado.

### Fix — migración se colgaba importando dumps grandes

- **Causa**: `docker::exec_stdin` con stdin adjunto: el stream de salida de bollard no emite `None` al terminar el proceso, así que el lector quedaba esperando.
- **Solución**: `migrate::import_dump` ahora usa CLI `docker exec -i … mysql` con `wait_with_output`. 7 MB en ~15 s.
- **`docker.rs`**: eliminado `exec_stdin` (sin uso).
- **`wpcli.rs`**: timeout defensivo de 120 s. `migrate::fix_site_url` con `--skip-plugins --skip-themes` (no carga plugins normales; los mu-plugins no se evitan con esos flags, por eso el timeout).
- **`OpConsole.svelte`**: el listener de `op-log` se engancha en `onMount` (no al abrir) para no perder las primeras líneas.

### Fix — consola de progreso vacía: faltaba la capability de eventos

- **Causa**: `app.emit`/`listen` usan el plugin `core:event`, que sí está gateado por ACL. Sin capability, `listen('op-log')` quedaba bloqueado. Los tests e2e usan IPC mockeado y no lo detectaban.
- **`src-tauri/capabilities/default.json`** (nuevo): concede `core:default` + `core:event:default` a la ventana `main`. Tauri autodescubre.
- **`migrate.rs`**: mensajes de progreso más descriptivos y numerados (`[n/6]`). `migrate_site` envuelve el flujo real y ante cualquier error emite `✗ La migración falló: …` antes de propagar.

### Fix — import del dump: timeout con rollback, aceleración y barra de progreso

- **Aceleración**: pragmas de sesión antepuestos al stream (`SET foreign_key_checks=0; unique_checks=0; autocommit=0; … COMMIT;`).
- **Watchdog con rollback + resume**: chunks de 1 MiB; cancela `docker exec` si ni el stdin avanza ni crece la DB durante 3 min (`IMPORT_IDLE_TIMEOUT`). `wordpress::reset_database` (nuevo) hace `DROP DATABASE` + recrea vacía. Reintentar reanuda: la DB queda limpia, el import vuelve a empezar de cero.
- **Indicador de vida correcto**: el pipe del OS es de ~64 KB; medir solo bytes-por-stdin daba falsos timeouts. El watchdog usa también el **tamaño real de la DB** (`information_schema.tables WHERE table_schema='{db}'`, vía `query_db_size`).
- **Barra de progreso en sitio**: `progress::log_progress` con prefijo SOH (``); `OpConsole.svelte` reescribe en sitio. Formato `12/53 MB ━━━━━──── 1:23` que se actualiza cada 2 s.

## Visor de bases de datos (Adminer)

- **`panel-adminer`** (`adminer:4`), servicio compartido on-demand: MySQL/MariaDB/Postgres. UI 127.0.0.1:8088.
- **Plugin `docker/adminer/autologin.php`** (montado en `plugins-enabled/`): auto-login en cero clics. En GET inyecta `$_POST["auth"]` con la pass fija `panel`; como los plugins se construyen antes del bloque de auth de Adminer y este reemplaza el token del POST de `auth` por el de sesión válido, `verify_token()` pasa sin formulario. Solo en GET (no pisa los POST reales del usuario, p. ej. ejecutar SQL). Sin restricción de vista.
- **Comando `open_adminer(id)`**: exige el proyecto corriendo, arranca Adminer y abre `?{driver}={db_container}&username={root|panel}&db={dbName}` (driver `pgsql` para Postgres, `server` para MySQL/MariaDB). Botón "Ver base de datos (Adminer)" en la sección Base de datos del proyecto.

## Fix — Auto-login en proyectos importados de LocalWP

- **Causa**: `localwp::import_site` y `migrate` no inyectaban los mu-plugins.
- **`wordpress::sync_mu_plugins(site)`**: helper idempotente; `repair_autologin` lo invoca y activa `oneClickAdmin`.

## Terminal WP-CLI en un clic

- **`cli::open_terminal_at(path)`**: lanza el primer emulador de terminal disponible (konsole, gnome-terminal, xfce4-terminal, kitty, alacritty, x-terminal-emulator) con cwd en la carpeta del proyecto, detached.
- **Comando `open_terminal(id)`**: instala el wrapper (idempotente) y abre la terminal ya situada en el proyecto.
- **Auto-instalación del wrapper**: en `lib.rs::run::setup` (`cli::install_cli_wrapper`).
- **UI** (tab *Servicios*): botón "Abrir terminal del proyecto" (requiere proyecto encendido) + texto de ayuda con ejemplos.

## Fix — el wrapper `wp` de terminal corría como root

- `wp-wrapper.sh` ahora hace `docker exec -i --user www-data "wp-${PROJECT_ID}" php /usr/local/bin/wp --path=/var/www/html "$@`. El refresco del script se aplica solo al reabrir el panel.

## Git/GitHub — repos en cualquier ruta, autodetección y VSCode

- **Modelo genérico** (`config.rs`): `GithubConfig` pasa a una lista única `repos: Vec<GithubRepo>` en cualquier ruta bajo `public/`. Los campos legacy `theme`/`plugins` se conservan para leer `config.json` antiguos; `GithubConfig::normalize()` (en `read_site_config`) los pliega en `repos` y los deja de serializar.
- **Clonar a cualquier sitio** (`gh_clone`): `kind` (theme/plugin/muplugin) propone ruta bajo wp-content; `path` explícito la sobreescribe.
- **Autodetección** (`github::scan` → `gh_scan`): recorre `wp-content` (prof. 4, salta `node_modules`/`vendor`), devuelve `DetectedRepo` con ruta, remoto, rama, `registered`. `gh_register` adopta huérfanos.
- **Abrir en VSCode** (`open_vscode` + `github::ensure_workspace`): genera (una vez) `<nombre>.code-workspace` en la raíz del proyecto (carpeta principal `app/public` + cada repo git detectado como root adicional, multi-root) y lo abre con el primer binario disponible (`code`/`codium`/`code-insiders`). Para worktree-projects, el workspace apunta al `wt/{basename}` (no al `public` vacío).

## Borrar proyecto (con opción de conservar la carpeta)

- **Siempre borra todos los datos**: apaga + quita container/vhost + `DROP DATABASE` del esquema del proyecto (`wordpress::drop_database`).
- **Modal de confirmación propio** (`DeleteProjectModal.svelte`): titula con el nombre + checkbox "Borrar también la carpeta del proyecto en disco".
- **Consola con ventana de gracia**: 5 s con botón "Cancelar borrado". Pasados los 5 s, "Cancelar" se reemplaza por "Cerrar" deshabilitado hasta que termine.
- **API**: `delete_site(id, deleteFolder)`. "Cancelar importación" usa `deleteFolder=true`.

## Auto-login con selector de usuario

- **Selector en la UI**: cuando `oneClickAdmin=true` y el proyecto corre, `<select>` pegado a "Abrir admin" con la lista de usuarios. Selección persistida en `localStorage` (`wp-panel:autologin:<id>`). Vacío = primer administrador.
- **Comando `list_wp_users`** (`wpcli`): `wp user list --fields=ID,user_login,display_name,roles --format=json`.
- **`open_admin` con `userId?`**: el transient almacena el `user_id` (string numérico). `> 0` = ese usuario exacto; `0` / ausente = primer administrador.
- **mu-plugin actualizado**: redirect adaptativo (`admin_url()` si `manage_options`, `home_url('/')` si no).
- **OPcache en php.ini**: `opcache.enable=1 / validate_timestamps=1 / revalidate_freq=0`.
- **`repair_all_php_ini`**: regenera el `php.ini` de todos los proyectos desde el template actual; resumen OK/errores. Disponible en **Configuración → Mantenimiento**.

## Clones como sublista del padre + nombre desde el punto de guardado

- **Dashboard**: clones anidados bajo el padre con sangría + conector `└`, fondo tenue. Emparejado por `cloneOf.parentId`; huérfanos (sin padre) caen a primer nivel.
- **`clone.rs`**: el nombre del clone = etiqueta del snapshot (`meta.label`), no `"{padre} (clone)"`. Slug derivado vía `slugify()` (`{parent_dirname}-{label_slug}`, desambiguación `-N`).

## DB durable + auto-dump (protección ante apagón)

- **DB durable** (`docker.rs`): `ensure_db` bindea el datadir del container DB compartido a `config_dir/db-data/{container}/` (`DbType::datadir()`). Sobrevive al recreado del container y al apagón. Los containers legados se migran una sola vez sin pérdida: `migrate_db_to_volume` copia con `docker cp` (excepción CLI) y recrea con el bind. `db_has_volume` exige `source == host_dir` (no basta con el destino, para no confundir con un volumen anónimo de la imagen).
- **Auto-dump** (`autodump.rs`): un watcher por proyecto activo sondea cada 20 s con `SHOW GLOBAL STATUS Innodb_rows_*`; si hay escrituras, `backup::dump_bytes` + hash; si difiere, persiste `db-*.sql` + `rotate_dumps(3)`. Enganchado en `start_site` y en el `setup` (sitios ya activos). Línea base sembrada desde el último dump en disco.
- **Fixes**:
  - `db_has_volume` exige `source == host_dir` además del destino.
  - `mysqldump --skip-dump-date` para que la línea `Dump completed on` no rompa el dedup por hash.
  - Auto-dump siembra desde disco: una edición con el panel cerrado o al arrancar se detecta en el primer sondeo.
- **Log de volcados** (`dumplog.rs`): `~/.config/wordpress-panel/dump-log.jsonl`. Comandos `dump_log` (lista, más nuevos primero) y `clean_dump_log(before?, dbName?)` (limpieza; no toca los `.sql`). UI en **Configuración** y `/dumps`.

## Exclusiones en puntos de guardado

- **`SiteConfig::snapshot_excludes`** (config.rs): rutas relativas a public que se añaden como `--exclude=./{ruta}` al tar del código, sobre las fijas (uploads, cache, wp-config, *.log).
- **`snapshot::detect_excludable`**: escanea subcarpetas de wp-content (excepto uploads/cache) y carpetas de backup conocidas (UpdraftPlus, All-in-One WP Migration, WPvivid, Duplicator, Duplicator Pro, Backuply). Marca `known` (recomendado excluir).
- **Comandos** `detect_excludable` / `set_snapshot_excludes`.
- **UI** (pestaña Puntos de guardado): panel "Exclusiones" plegable con checkboxes (tamaño + badge del plugin), campo para añadir rutas a mano y persistencia. Cada snapshot muestra cuántas carpetas excluyó.

## Worktree-projects (probar una rama de un repo en aislamiento)

- **`worktree.rs`** (`create_worktree`/`remove_worktree`/`list_worktrees`): crea un `SiteConfig` con `worktree_of` (`config::WorktreeInfo`). NO copia código: el repo objetivo se materializa como un `git worktree` sobre una rama nueva en `{path}/wt/{basename}`, y un `wp-config.php` propio.
- **Composición por montajes Docker** (`docker::create_php_container`): el container del worktree monta el `public` del padre en `/var/www/html` (compartido) y sobrepone solo el `git worktree` y el `wp-config.php` propio.
- **nginx** (`render_vhost`): el `root` del vhost son los estáticos del padre; un `location ~ ^/{target}/…(css|js|img…)$ { alias /srv/projects/{dir}/wt/{basename}/$1; }` (antes del static genérico).
- **Base de datos**: compartida (constantes `WP_HOME`/`WP_SITEURL` en el wp-config propio, sin mutar la DB del padre) o copia propia (dump + import del padre). Se pregunta al crear.
- **Tres superficies**: comando IPC `create_worktree_site`/`remove_worktree_site`/`list_worktrees`; UI en `/site/[id]`; y subcomando `wordpress-panel-cli worktree {list|create|remove}` que habla con el panel en ejecución por D-Bus.

## Rediseño UI — master-detail estilo LocalWP

- **Riel de íconos** (`+layout.svelte`): sustituye los links de texto del sidebar por un riel angosto (Proyectos/Dominios/Servicios/Configuración + botón «+»).
- **Master-detail de proyectos** (`+page.svelte`): columna izquierda con la lista agrupada (grupos de `groups.json` fusionados con `config.group`; alta de grupo inline, **drag&drop** nativo HTML5; power/estado como íconos; **grupos plegables** con estado en `localStorage`; sección fija **"En ejecución"** con los proyectos `running` al inicio sin duplicarlos) y panel grande con el detalle del proyecto **seleccionado por estado** (`selectedId`, sin navegar) vía `{#key}<ProjectDetail/>`.
- **`ProjectDetail.svelte`**: cabecera descongestionada (una acción primaria encender/detener, menú «···» con acciones secundarias, accesos rápidos bajo el nombre). El selector de usuario de auto-login en la pestaña Info.
- **Grupos persistentes** (`groups.rs` + `groups.json`): la asignación de grupo dejó de ser un input dentro del proyecto; ahora se hace por **drag&drop** de la fila sobre la cabecera del grupo (`set_site_group`). Comandos `list/create/rename/delete/reorder_groups`.

## CLI ampliada (snapshots + git + deploy)

`wordpress-panel-cli` (habla con el panel en ejecución por D-Bus) cubre:

- `snapshot {list,create <label>,delete <id>,clone <id>}`.
- `git {scan,status,pull,set-deploy,deploy}`.

## MCP para agentes IA (`mcp/`)

- Servidor MCP (`mcp/server.mjs`, sin dependencias, protocolo por stdio a mano) que envuelve `wordpress-panel-cli` (CLI → D-Bus). 19 herramientas.
- Recarga reactiva de la UI: métodos D-Bus que mutan proyectos (start/stop/all, worktree, clone) emiten `sites-changed`; la UI se suscribe y se recarga sola.

## Fases 4+ / pendientes

### feat(php): tope de subida por proyecto + nginx sin límite de body

- **Comando `set_php_upload_limit` (`lib.rs`)**: reescribe `upload_max_filesize` y `post_max_size` en el `php.ini` del proyecto y recarga `php-fpm` en caliente (SIGUSR2) si está activo. `mb=0` vuelve al default del template (64M). Persistido en `services.php.uploadMaxMb` (camelCase en JSON, `Option<u32>` en Rust).
- **Espejo en CLI**: `wordpress-panel-cli php upload <MB>`. MCP: `set_php_upload_limit(project, mb)`. D-Bus: `SetUploadLimit(id, mb)`.
- **nginx tuning**: `client_max_body_size 0` en `00-panel-tuning.conf`; el límite real lo pone PHP.
- **UI**: tab "Info" → input MB → botón "Guardar".

### feat(nginx): autocura de vhosts huérfanos + comando repair_nginx

- **`prune_orphan_vhosts`** (`docker.rs`): borra `{id}.conf` cuyo container `wp-{id}` no corre. Se ejecuta antes de arrancar nginx en `ensure_nginx` para que un upstream caído no aborte el arranque entero con "host not found in upstream".
- **`repair_nginx` (`docker.rs`) + comando `lib.rs::repair_nginx`**: poda + recrea `panel-nginx`. Botón en **Configuración → Mantenimiento**.

### Otros commits clave (no incluidos en narrativa)

- `feat(php): soporte WebP en GD (imagen r3)` (subida de revisión de la imagen php).
- `feat(groups): lista durable de grupos en groups.json` (formalización de los grupos, que ya estaban en el plan desde la fase 2).
- `feat(ui): rediseño master-detail estilo LocalWP con grupos por drag&drop` (cierre del rediseño UI).
- `fix(worktree): validar nombre de rama y limpiar worktrees a medio crear` (cubre ramas con espacios, sugerencia de `guess_branch` y limpieza del container/vhost/carpeta en `run_create::catch`).
- `fix(snapshot): tolerar avisos no fatales de tar y mostrar su stderr` (cambio en `snapshot::run` para no abortar con código 1).
- `feat(deploy): deploy directo por repo desde el panel (staging)` (rama, build, dirs en `SiteConfig::GithubRepo`).
- `fix(worktree): git worktree prune antes de add` (evita "missing but already registered" tras intentos fallidos).
- `fix(nginx): subir server_names_hash_bucket_size a 128` (worktrees con slugs largos desbordaban el bucket por defecto).
- `fix(worktree): abrir el git worktree en VSCode, no el public vacío` (workspace apunta a `wt/{basename}`).
- `feat(cli): snapshots, clones y git/deploy desde la terminal` (CLI ampliada).
- `fix(dbus): usar el runtime Tokio en zbus (feature tokio)` (los handlers de D-Bus que tocan bollard necesitan el reactor tokio).
- `feat(ui): sección CLI en el riel con documentación de comandos` (ruta `/cli` con tabla de comandos).
- `feat(cli): control (start/stop/open), contenedores, recursos y logs`.
- `feat(cli): list de proyectos con estado y start/stop por nombre/id` (`ListSites` por D-Bus, `resolve_pid` por nombre).
- `fix(docker): recrear panel-nginx zombie tras apagón sucio` (`setns/nsexec` en `reload_nginx` → `remove_container` + `ensure_nginx`).
- `feat(mcp): servidor MCP para agentes IA + recarga reactiva de la UI` (catálogo de 19 herramientas, `sites-changed` para recarga).
- `chore(deploy): fix AppImage install on Manjaro and add .desktop creation` (`NO_STRIP=1` requerido en Manjaro/Arch; AppImage con `WEBKIT_DISABLE_DMABUF_RENDERER=1`; `.desktop` y `.png` en hicolor).

### Diferido fuera de fase

- **Botones de la barra de título** no respetan la config de KDE (ver `docs/KNOWN_ISSUES.md`).
- **Fase 5 IA** (`agent.rs`): pendiente.

## Resumen de cambios por fase

| Fase | Estado | Resumen |
|---|---|---|
| 1 | Funcional | MVP core: red `panel-net`, php-fpm por proyecto, nginx compartido, dnsmasq, mkcert, primera config. |
| 2 | Funcional | Logs en vivo, auto-login, plugins/themes, GitHub vía `gh`, SSL, grupos (versión inicial), D-Bus + plasmoid. |
| 3 | Funcional | Mailpit, MinIO, MariaDB, PostgreSQL, backup, wrapper WP-CLI, headless, stubs. |
| 4 | Funcional | Settings + estado del sistema, migración entre sistemas, import LocalWP, re-import desconectados, empaquetado del plasmoid, visor Adminer, consola de progreso, fixes de import/dump, auto-login con selector, exclusiones de snapshot, worktree-projects, rediseño UI, CLI ampliada, MCP. |
| 4+ | Funcional (post-Fase 4) | Tope de subida por proyecto, autocura de vhosts huérfanos + `repair_nginx`. |
| 5 | Pendiente | Agentes IA (`agent.rs`). |

Volver al índice principal de `docs/resume/`.
