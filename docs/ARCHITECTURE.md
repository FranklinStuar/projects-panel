# Arquitectura

Referencia técnica del panel. Para producto/fases ver `PLAN.md`; para agregar
cosas ver `EXTENDING.md`. **Mantener este doc al día con el código.**

## Visión general

```
┌─────────────────────────────────────────────┐
│  Ventana Tauri (WebKitGTK)                    │
│  ┌─────────────────────────────────────────┐  │
│  │ Frontend SvelteKit (SPA, ssr=false)     │  │
│  │  routes/ + lib/api.ts (invoke IPC)       │  │
│  └───────────────┬─────────────────────────┘  │
│                  │ Tauri IPC (invoke/commands)  │
│  ┌───────────────▼─────────────────────────┐  │
│  │ Backend Rust (src-tauri/src/)            │  │
│  │  lib.rs ─ comandos                       │  │
│  │  config / docker / nginx / php / domain  │  │
│  │  wordpress / wpcli                       │  │
│  └───────────────┬─────────────────────────┘  │
└──────────────────┼────────────────────────────┘
                   │ bollard (socket Docker) + CLI docker (build img, import dump)
        ┌──────────▼───────────────────────────┐
        │ Docker (red panel-net)                │
        │  Compartidos on-demand:               │
        │   panel-nginx, panel-mysql-{ver}, …   │
        │  Por proyecto (solo activo):          │
        │   wp-{site-id}  (php-fpm)             │
        └───────────────────────────────────────┘
```

## Modelo de containers y recursos

Materializa el principio rector (ver `CLAUDE.md`).

### Red
- Bridge único **`panel-net`**. Todo container va aquí → resolución por nombre.
- `panel-nginx` → `wp-{id}:9000` (FastCGI). `wp-{id}` → `panel-mysql-84:3306`.
- Prerequisito de todo: `DockerManager::ensure_network()`.

### Compartidos (1 instancia para todos, on-demand)
| Container | Imagen | Notas |
|---|---|---|
| `panel-nginx` | `nginx:alpine` | Reverse-proxy. Publica `127.0.0.1:80` y `:443`. 1 vhost por proyecto activo. Recarga con `nginx -s reload`. |
| `panel-mysql-{ver}` | `mysql:{ver}` | 1 por versión (`panel-mysql-80`, `panel-mysql-84`). Root pass `panel`, `MYSQL_ROOT_HOST=%`. |
| `panel-mariadb-{ver}` | `mariadb:{ver}` | idem (sin alpine: no existe). |
| `panel-postgres-{ver}` | `postgres:{ver}-alpine` | idem. |
| `panel-mailpit` | `axllent/mailpit` | Captura de correo de todos los activos. UI host `127.0.0.1:8025`, SMTP `:1025` interno. Arranca con cualquier proyecto activo. |
| `panel-minio` | `minio/minio` | S3 local on-demand (solo si el proyecto tiene flag `minio`). API host `127.0.0.1:9100`, consola `127.0.0.1:9101`, datos en `~/.config/wordpress-panel/minio-data`, creds `panel`/`panel-secret`. |
| `panel-adminer` | `adminer:4` | Visor de DB on-demand (al pulsar «Ver base de datos»). UI host `127.0.0.1:8088`. Sirve MySQL/MariaDB/Postgres por `panel-net`. Monta `docker/adminer/autologin.php` como plugin: auto-login en cero clics (inyecta `auth` en GET con la pass fija `panel`; ver el archivo). Abre directo en la DB del proyecto vía `?server=…&db=…`. Se apaga cuando no queda proyecto activo. |

Nombre de DB compartida = `{prefix}-{version sin puntos}` (`DbType::service_prefix()`).

### Por proyecto (solo mientras activo)
- Container `wp-{site-id}`, imagen `panel-php:{ver}` (construida desde
  `docker/php/Dockerfile`, base `php:{ver}-fpm-alpine`).
- **No publica puertos al host.** Solo habla con `panel-nginx` por `panel-net`.
- Volúmenes: `app/public→/var/www/html`, `php.ini→conf.d/zz-project.ini` (ro),
  `wp-cli.phar→/usr/local/bin/wp` (ro).
- Env `PUID`/`PGID` = uid/gid del host (`docker::host_uid_gid()` vía getuid/getgid).

### Ciclo de vida (`docker.rs`)
- **start_site**: ensure_network → ensure_db → crear/arrancar `wp-{id}` →
  `nginx::write_vhost` → ensure_nginx → reload_nginx → `domain::ensure_wildcard`.
- **stop_site**: parar `wp-{id}` → `nginx::remove_vhost` → reload →
  `teardown_unused_shared` (apaga DB si ningún activo la usa; apaga nginx si no
  queda ningún proyecto activo). **Resultado: N proyectos parados = 0 containers.**
- **site_status**: `MigrationPending` si el flag; si no, según `wp-{id}` corre o no.

### Worktree-projects (composición por montajes)
Un worktree-project (`worktree.rs`) NO tiene su propio `public`: su container
`wp-{id}` monta el `public` del **padre** en `/var/www/html` (compartido, rw) y
sobrepone solo dos cosas:
- el repo objetivo, un `git worktree` en `{path}/wt/{basename}` →
  `/var/www/html/{targetPath}` (la rama nueva, aislada);
- un `wp-config.php` propio (`{path}/wp-config.php`) →
  `/var/www/html/wp-config.php` (dominio + BD del worktree).

Docker ordena los binds por profundidad del destino, así el padre (raíz) se monta
antes y los overrides quedan encima. nginx sirve los estáticos desde el `public`
del padre (`root /srv/projects/{parentDir}/app/public`) y, para el objetivo, un
`location ~ ^/{targetPath}/…\.(css|js|img…)$` con `alias` al `git worktree`, para
que los assets de la rama se vean. La BD es la del padre (constantes
`WP_HOME`/`WP_SITEURL` evitan mutarla) o una copia propia. Eliminar = `git
worktree remove` (la rama persiste) + borrar carpeta: sin rastro.

### UID/GID (crítico)
`www-data` en alpine = uid 82 ≠ host (1000). El `entrypoint.sh` de la imagen php
ejecuta `usermod/groupmod` para alinear www-data a `PUID/PGID`. Sin esto WordPress
no escribe uploads/plugins y el usuario no puede editar archivos clonados con `gh`.

### nginx: por qué `/srv/projects` y `/var/www/html`
- nginx monta la raíz de proyectos **read-only** en `/srv/projects` (sirve estáticos)
  y los vhosts en `/etc/nginx/conf.d` (ro). php monta su `public` en `/var/www/html`.
- El vhost pone `root /srv/projects/{dir}/app/public` pero el FastCGI envía
  `SCRIPT_FILENAME /var/www/html$fastcgi_script_name` (la vista de php). Así nginx y
  php no necesitan compartir la misma ruta de montaje. Ver `nginx.rs::render_vhost`.

## Mapa de módulos (src-tauri/src/)

| Módulo | Responsabilidad |
|---|---|
| `lib.rs` | Comandos `#[tauri::command]`, `run()` (incl. `GTK_CSD=0` en Linux para decoración nativa), registro en `invoke_handler!`. |
| `config.rs` | Modelos (`SiteConfig`, `Services`, `DbType`, `SiteStatus`, `SiteState`), rutas (`config_dir`, `projects_root`), persistencia (`load_all_sites`, `read/write_site_config`, `find_site`). **`Endpoint`** (dónde publica el panel en el host: `loopbackIp`/`httpPort`/`httpsPort`, helper `site_url`) + `PanelConfig` persistido en `panel.json` (`load/save_endpoint`, `endpoint_or_default`). |
| `docker.rs` | `DockerManager` (bollard): red, ensure_db/ensure_nginx, start/stop_site, teardown, `exec`/`exec_as` (fija usuario; chequea exit code), helpers uid/gid e imagen-context. `wait_db_ready` (gatea sobre TCP antes de usar la DB). `ensure_db` bindea el datadir a un dir durable del host (`db_data_dir` → `config_dir/db-data/{container}`, montado en `DbType::datadir()`) y migra containers legados sin bind (`migrate_db_to_volume` vía `docker cp` + recreado; `db_has_volume` detecta el bind). Selección de endpoint (`select_endpoint`/`autoselect_endpoint`/`preflight_endpoint`) con autodetección de puerto libre. |
| `nginx.rs` | Render/escritura/borrado de vhosts en `~/.config/wordpress-panel/nginx/conf.d/`. |
| `groups.rs` | Lista durable de grupos de proyectos en `config_dir/groups.json` (`{ order: [...] }`). `list/create/rename/delete/reorder`. La pertenencia sigue en `SiteConfig::group`; este archivo solo aporta el conjunto de grupos conocidos (incl. vacíos) y su orden. `rename`/`delete` reescriben los `config.json` afectados; `set_site_group` registra el grupo destino aquí al asignarlo por drag&drop. |
| `php.rs` | `ensure_php_image` (docker build por versión), `wp_cli_phar_path` (descarga el phar). |
| `domain.rs` | dnsmasq wildcard `*.test`: snippet + detección de resolución (`resolves_to`). Regla parametrizada por IP (`wildcard_rule`); `install_wildcard` la instala vía `pkexec` y recarga NetworkManager (para endpoint con IP loopback alterna). |
| `migrate.rs` | `migrate_site()`: provisiona un proyecto `migrationPending` en el sistema actual (crea DB, regenera wp-config, importa el último dump de `app/sql/`, fija `home`/`siteurl`, regenera SSL) y lo enciende. Devuelve config + aviso opcional. `import_dump()` acelera con pragmas de sesión, emite barra de progreso en vivo, y un watchdog cancela el `docker exec` si no hay avance en 3 min (mide stdin + tamaño real de DB) → `wordpress::reset_database` revierte y reintentar reanuda. |
| `localwp.rs` | Importa sitios de LocalWP: `list_sites()` (parsea `~/.config/Local/sites.json`), `import_site()` (copia `app/public` + dump, crea `config.json` `migrationPending`). |
| `progress.rs` | `log(app, msg)`: emite líneas de progreso en el evento `op-log` para operaciones largas (migración, import); el frontend las muestra en `OpConsole.svelte`. `log_progress(app, msg)`: línea "viva" (prefijo SOH) que `OpConsole` reescribe en sitio en vez de apilar (contadores/barras). |
| `system.rs` | `status()`: estado de prerequisitos para la pantalla de configuración (Docker, red `panel-net`, dnsmasq, CA mkcert, wrappers WP-CLI, plasmoid) + endpoint y rutas. |
| `netcheck.rs` | Lee `/proc/net/tcp{,6}` para clasificar puertos del host: `Free`/`Wildcard`/`Specific(IPs)`. Selectores `pick_loopback_ip`/`pick_alt_port` y `holder_name` (proceso que ocupa un puerto). Base de la selección de endpoint de `docker.rs`. |
| `wordpress.rs` | `create_site` end-to-end, `download_core` (tarball), `fetch_versions` (API wp.org, cache 24h), DB/wp-config/install vía WP-CLI. `sync_mu_plugins`: (re)inyecta los mu-plugins del panel (mailpit siempre + auto-login si `oneClickAdmin`); idempotente, lo usan `create_site`, `migrate` y `repair_autologin`. |
| `wpcli.rs` | `run()` WP-CLI dentro del container del proyecto, como `www-data` (WP-CLI rechaza root). |
| `logs.rs` | `spawn_stream`: sigue (`follow`) los logs del container y los emite como evento `log:{id}`. Cancelable vía `JoinHandle::abort()`. |
| `autologin.rs` | `open_admin`: token efímero (transient WP, 60s, un solo uso) + abre navegador; el mu-plugin `panel-autologin.php` valida y loguea al usuario. El transient almacena el `user_id` destino: `> 0` = ese usuario exacto, `0` = primer administrador (retrocompatible). Llama `wordpress::inject_autologin_muplugin` antes de crear el token para garantizar la versión actual del plugin aunque el proyecto se creara antes. El mu-plugin lo inyecta `wordpress::sync_mu_plugins` al crear/migrar; `repair_autologin` lo reinyecta en proyectos viejos. Redirect: admin WP si tiene `manage_options`, home si no. |
| `github.rs` | `gh`/`git` en el HOST (no container, los archivos están bind-montados): `status`, `clone`, `pull`, `remove_dir`, `propose_path`. `scan` autodetecta repos git bajo wp-content (`DetectedRepo`); `read_repo_meta` lee remoto/rama de un huérfano; `open_vscode` + `ensure_workspace` (genera `.code-workspace` multi-root, una vez). Repos en `github.repos` (lista genérica; `GithubConfig::normalize` pliega el legacy theme/plugins). Sin auth propia. |
| `ssl.rs` | `generate`: cert/key por dominio con mkcert en `ssl/` del proyecto. La CA local (`mkcert -install`) se hace una vez en `first-run.sh`. |
| `dbus.rs` | Servidor D-Bus (zbus) para el plasmoid KDE; arranca en el `setup` de Tauri. Ver sección D-Bus. |
| `backup.rs` | `dump_bytes`: captura `mysqldump` (dentro del container DB, socket local) en memoria. `export_db`/`export_db_to`: lo escriben en `app/sql/db-{timestamp}.sql`. `rotate_dumps`: deja solo los N `db-*.sql` más recientes. `stop_site` los invoca para exportar-al-detener. El auto-dump (`autodump.rs`) reusa `dump_bytes`. |
| `autodump.rs` | Estado Tauri `AutoDump` (`Mutex<HashMap<id, JoinHandle>>`): un watcher por proyecto activo que protege contra pérdida de datos por apagón. Sondea cada 20s; gate barato por `SHOW GLOBAL STATUS Innodb_rows_*` (no vuelca si la DB está ociosa); cuando hay escrituras, `dump_bytes` + hash y si difiere del último, persiste un dump nuevo en `app/sql/` + `rotate_dumps` + lo registra en `dumplog` (source `auto`). Se engancha en `start_site` y en el `setup` (sitios ya activos al abrir); se aborta en `stop_site`/`stop_all_sites`. |
| `dumplog.rs` | Log de volcados de DB (`config_dir/dump-log.jsonl`, JSONL `DumpLogEntry`): una línea por cada dump escrito en `app/sql/` para revisar y comparar. `append` (lo llaman auto-dump, export-al-detener y export manual con su `source`), `read_all` (más nuevos primero), `clean(before?, dbName?)` (poda por fecha y/o base de datos; sin filtros borra todo; no toca los `.sql`). |
| `snapshot.rs` | Puntos de guardado por proyecto en `~/panel-wp/{slug}/snapshots/{id}/` (`code.tar.zst` sin uploads/cache/wp-config/logs + `db.sql` + `meta.json` con `label`). `create_snapshot` (arranca solo el motor DB), `list_snapshots` (orden desc por fecha), `delete_snapshot`. **Exclusiones extra por proyecto**: `SiteConfig::snapshot_excludes` (rel. a public) se añaden como `--exclude` al tar; `detect_excludable` sugiere carpetas (subcarpetas de wp-content + backups conocidos: UpdraftPlus, All-in-One WP Migration, WPvivid, Duplicator…) con tamaño y flag `known`. El `meta.json` registra los `excludes` aplicados. |
| `clone.rs` | `create_clone(parent_id, snapshot_id)`: crea un `SiteConfig` con `clone_of` poblado desde un punto de guardado. Comparte engine DB + nginx; solo añade 1 container php + 1 schema. **Nombre del clone = `meta.label`** del punto de guardado; slug/carpeta/dominio derivan de esa etiqueta vía `slugify()` (`{parent_dirname}-{label_slug}`, desambiguación `-N` o UUID corto en `find_free_slot`). Uploads viejos servidos vía fallback nginx desde el padre (ro); los nuevos en la carpeta del clone (rw). En el dashboard el clone se muestra anidado bajo su padre (no como proyecto suelto). |
| `worktree.rs` | `create_worktree(parent_id, target_path, branch, base_branch?, shared_db)`: proyecto de prueba ligero atado a un repo del padre. NO copia código — el `public` del padre se comparte por **montaje Docker** y solo se sobrepone el repo objetivo (un `git worktree` sobre `branch`, en `{path}/wt/{basename}`) y un `wp-config.php` propio (`{path}/wp-config.php`). BD compartida (constantes `WP_HOME`/`WP_SITEURL`, sin mutar la DB) o copia (dump+import del padre). `remove_worktree` hace `git worktree remove` (la rama queda en el repo del padre) + drop del esquema si era copia + borra la carpeta. `list_worktrees(parent_id)` filtra `SiteConfig.worktree_of`. El branch de `docker::create_php_container`/`nginx::render_vhost` materializa la composición (ver más abajo). |
| `cli.rs` | `install_cli_wrapper`: copia `wp` y `wordpress-panel-cli` a `~/.local/bin` (chmod 755). El `wp` detecta el proyecto por el CWD vía `wordpress-panel-cli detect-project`. Se instala automáticamente al arrancar el panel (`run()` setup, idempotente), así no hay nada por-proyecto que instalar. `open_terminal_at`: lanza el primer emulador de terminal disponible (konsole, gnome-terminal, xfce4-terminal, kitty, alacritty, x-terminal-emulator) con cwd en la carpeta del proyecto. |
| `integration_tests.rs` | Solo en `#[cfg(test)]`: tests de integración `#[ignore]` (Docker / import LocalWP hermético). Ver `docs/TESTING.md §A.2`. |

> **Testing**: lógica pura en `#[cfg(test)] mod tests` por módulo; integración en
> `integration_tests.rs`; GUI con mock de IPC (`src/lib/dev/`) + Playwright
> (`e2e/`). Detalle completo en `docs/TESTING.md`.

## Catálogo de comandos IPC

Definidos en `lib.rs`, expuestos en `src/lib/api.ts`. Todos `async`, retornan
`Result<T, String>`.

| Comando | Args | Retorno | Hace |
|---|---|---|---|
| `get_sites` | — | `Vec<SiteState>` | Escanea proyectos + estado real Docker. |
| `start_site` | `id` | `()` | Enciende proyecto (ver start_site). |
| `stop_site` | `id` | `()` | Detiene + teardown compartidos. |
| `stop_all_sites` | — | `()` | Detiene todos. |
| `exec_wpcli` | `id, args[]` | `String` | WP-CLI en el container. |
| `create_site` | `req: NewSiteRequest` | `SiteConfig` | Crea/instala proyecto completo. |
| `list_wp_versions` | — | `Vec<WpVersion>` | Versiones WP (cache 24h). |
| `panel_endpoint` | — | `Endpoint` | Punto de publicación del panel (IP loopback + puertos); el frontend muestra el puerto si es alterno. |
| `system_status` | — | `SystemStatus` | Estado de prerequisitos (Docker, red, dnsmasq, mkcert, wrappers, plasmoid) + endpoint y rutas. |
| `create_panel_network` | — | `()` | Crea el bridge `panel-net` si falta. |
| `reset_endpoint` | — | `()` | Olvida el endpoint persistido (reasigna puerto al próximo arranque). |
| `migrate_site` | `id` | `Migration` | Migra un proyecto pendiente (DB + dump + SSL) y lo enciende. Emite `op-log`. |
| `delete_site` | `id`, `deleteFolder` | `()` | Borra un proyecto: apaga + container/vhost + DROP de su DB del servidor compartido. `deleteFolder=true` borra la carpeta entera; `false` la desconecta del panel renombrando `config.json` → `config.disconnected.json` (los archivos y la config quedan para re-importar). Emite `op-log` con cada paso. |
| `list_localwp_sites` | — | `Vec<LocalSite>` | Sitios de LocalWP candidatos a importar. |
| `import_localwp_site` | `id` | `ImportResult` | Importa un sitio de LocalWP (queda `migrationPending`). |
| `list_disconnected_sites` | — | `Vec<DisconnectedSite>` | Carpetas de `~/panel-wp/` desconectadas (sin `config.json`): con `config.disconnected.json` (`preserved`) o con `app/public/wp-config.php` (`reconstructed`). |
| `import_disconnected_site` | `folderName` | `ImportResult` | Re-importa una carpeta desconectada: restaura/reconstruye `config.json` y la deja `migrationPending`. Emite `op-log`. |
| `open_admin` | `id`, `userId?` | `()` | Abre el admin en el navegador (auto-login si está activo). `userId` = ID de usuario WP destino; omitir o `0` = primer administrador. |
| `list_wp_users` | `id` | `Vec<WpUser>` | Lista usuarios WP del proyecto (`ID`, `user_login`, `display_name`, `roles`). Requiere proyecto encendido. |
| `repair_autologin` | `id` | `SiteConfig` | Activa `oneClickAdmin` y reinyecta los mu-plugins del panel (auto-login + mailpit). Para proyectos importados de LocalWP sin el plugin. No requiere proyecto encendido. |
| `repair_all_php_ini` | — | `String` | Regenera el `php.ini` de todos los proyectos desde el template actual (aplica cambios como OPcache). Devuelve resumen de éxito/errores. Los proyectos deben reiniciarse para que surta efecto. |
| `stream_logs` | `id` | `()` | Inicia el stream de logs → eventos `log:{id}`. |
| `stop_logs` | `id` | `()` | Detiene el stream de logs. |
| `list_plugins` | `id` | `String` (JSON) | `wp plugin list`. |
| `list_themes` | `id` | `String` (JSON) | `wp theme list`. |
| `gh_status` | — | `GhStatus` | gh instalado/autenticado + usuario. |
| `gh_clone` | `id, kind, repo, branch, path?` | `SiteConfig` | Clona repo + registra en `github.repos`. `kind` (theme/plugin/muplugin) propone ruta bajo wp-content; `path` explícito (rel. a public/) la sobreescribe → cualquier ubicación. |
| `gh_pull` | `id, path, branch` | `String` | `git pull` de una carpeta. |
| `gh_pull_all` | `id` | `String` | Pull de todos los repos registrados. |
| `gh_remove` | `id, path` | `SiteConfig` | Borra carpeta + desregistra de `github.repos`. |
| `gh_scan` | `id` | `Vec<DetectedRepo>` | Escanea `wp-content` (prof. 4, salta node_modules/vendor) y lista repos git registrados + huérfanos (remoto, rama, `registered`). |
| `gh_register` | `id, path` | `SiteConfig` | Registra en config un git huérfano ya en disco (lee remoto/rama). No clona. |
| `gh_branch_status` | `id, path, branch` | `BranchStatus` | `git fetch` + compara la rama con `origin/<branch>`: ahead/behind, árbol sucio, `canPull`. No muta el árbol. |
| `gh_set_deploy` | `id, path, branch, buildCmd?, buildDirs` | `SiteConfig` | Guarda rama objetivo, comando de build (`GithubRepo.buildCmd`) y carpetas de build (`buildDirs`, rel. al repo) del deploy directo. Repo debe estar registrado. |
| `gh_build_dirs` | `id, path` | `Vec<String>` | Carpetas candidatas de build en el repo: raíz (`""`) y subcarpetas de nivel 1 con `package.json`. Para el selector de la UI. |
| `gh_deploy` | `id, path` | `()` | Deploy directo (staging): checkout + `git pull --ff-only` + build en host (login shell) en cada `buildDirs` (o raíz). Emite al op-log. |
| `open_vscode` | `id` | `()` | Genera (una vez) `<nombre>.code-workspace` (public/ principal + repos git detectados) y lo abre en VSCode/VSCodium. |
| `regenerate_ssl` | `id` | `()` | Regenera cert mkcert + reload nginx. |
| `set_site_group` | `id, group?` | `SiteConfig` | Asigna/quita grupo del proyecto (target de drag&drop). Registra el grupo en `groups.json` si es nuevo. |
| `list_groups` | — | `Vec<String>` | Grupos persistidos, en orden (`groups.json`). |
| `create_group` | `name` | — | Crea un grupo vacío (idempotente). |
| `rename_group` | `old, new` | — | Renombra el grupo y reasigna los proyectos que lo tenían. |
| `delete_group` | `name` | — | Borra el grupo; sus proyectos quedan sin grupo. |
| `reorder_groups` | `order` | — | Sobrescribe el orden de los grupos. |
| `set_site_minio` | `id, enabled` | `SiteConfig` | Activa/desactiva MinIO; arranca el servicio si el proyecto corre. |
| `export_db` | `id` | `String` (ruta) | Dump de la DB a `app/sql/` (lo registra en el log de volcados, source `manual`). |
| `dump_log` | — | `Vec<DumpLogEntry>` | Log de volcados de DB, más nuevos primero (para revisar/comparar). |
| `clean_dump_log` | `before?`, `dbName?` | `usize` | Borra entradas del log por fecha (anteriores a `before`, ISO) y/o por base de datos. Sin filtros borra todo. NO toca los `.sql`. Devuelve cuántas borró. |
| `install_cli_wrapper` | — | `String` | Instala `wp`/`wordpress-panel-cli` en `~/.local/bin`. También se ejecuta solo al arrancar el panel. |
| `open_terminal` | `id` | `()` | Instala el wrapper (idempotente) y abre un emulador de terminal con cwd en la carpeta del proyecto; dentro funciona `wp`. |
| `open_mailpit` | — | `()` | Abre la UI de Mailpit. |
| `open_minio` | — | `()` | Abre la consola de MinIO. |
| `open_adminer` | `id` | `()` | Arranca `panel-adminer` y abre el navegador en la DB del proyecto (requiere proyecto corriendo). |
| `create_snapshot` | `id, label` | `SnapshotMeta` | Crea un punto de guardado (tar código + dump DB). Emite `op-log`. |
| `list_snapshots` | `id` | `Vec<SnapshotMeta>` | Puntos de guardado del proyecto (desc por fecha). |
| `delete_snapshot` | `id, snapshotId` | `()` | Borra un punto de guardado del disco. |
| `detect_excludable` | `id` | `Vec<ExcludableEntry>` | Escanea `wp-content` y devuelve carpetas candidatas a excluir (subcarpetas + backups conocidos como UpdraftPlus/ai1wm), con tamaño y flag `known`. |
| `set_snapshot_excludes` | `id, excludes` | `()` | Persiste en `config.json` las rutas (rel. a public) a excluir del tar del punto de guardado. |
| `create_clone` | `id, snapshotId` | `SiteConfig` | Crea un clone temporal desde un punto de guardado; nombre = etiqueta del snapshot. Emite `op-log`. |
| `create_worktree_site` | `parentId, targetPath, branch, baseBranch?, sharedDb` | `SiteConfig` | Crea un worktree-project (`git worktree` del repo `targetPath` sobre `branch`). Emite `op-log`. |
| `remove_worktree_site` | `id, deleteBranch` | `()` | Elimina un worktree-project (`git worktree remove`; la rama queda salvo `deleteBranch`). Emite `op-log`. |
| `list_worktrees` | `parentId` | `Vec<SiteConfig>` | Worktree-projects de un padre. |
| `feature_stub` | `feature` | `String` (Err) | Stub Cloudflare/deploy/package (fase posterior). |

**Eventos** (backend → frontend, vía `app.emit`): `log:{id}` — una línea de log de
container por evento; `op-log` — línea de progreso de una operación larga
(migración/import/borrado), mostrada en `OpConsole.svelte`. El frontend se suscribe con `listen()` de `@tauri-apps/api/event`. Estado
de los streams activos en `LogStreams` (managed state, `Mutex<HashMap>`).
El canal `op-log` también lo emite el **frontend** (con `emit()`) para las líneas
de la ventana de gracia del borrado (cuenta atrás de 5 s antes de llamar a
`delete_site`); como el listener es el mismo, se ven igual que los pasos del backend.

> **Capability obligatoria para eventos.** En Tauri 2 los comandos propios
> (`#[tauri::command]`) no pasan por el ACL, pero `listen()`/`emit()` usan el
> plugin `core:event`, que **sí** está gateado. Sin una capability que lo
> conceda, `listen('op-log')` queda bloqueado y la consola sale vacía. La
> capability vive en `src-tauri/capabilities/default.json` (`core:default` +
> `core:event:default`, ventana `main`); Tauri autodescubre `capabilities/*.json`.
> Al añadir cualquier evento nuevo backend→frontend, basta con que esa capability
> siga concedida.

## D-Bus (plasmoid KDE)

`dbus.rs` publica en la sesión del usuario el servicio
`com.goldmediatech.WordpressPanel`, objeto `/com/goldmediatech/WordpressPanel`,
interfaz `…Manager`:

| Método | Retorno | Hace |
|---|---|---|
| `GetRunningSites` | `String` (JSON `[{id,name,domain}]`) | Proyectos activos. |
| `StopSite(id)` | `bool` | Detiene un proyecto. |
| `StopAll` | `bool` | Detiene todos. |
| `Quit` | — | Cierra el panel (`app.exit(0)`). |
| `ListWorktrees(parentId)` | `String` (JSON) | Worktree-projects de un padre. |
| `CreateWorktree(parentId, targetPath, branch, baseBranch, sharedDb)` | `String` (JSON `{ok,…}`) | Crea un worktree-project (lo usa el wrapper CLI). |
| `RemoveWorktree(id, deleteBranch)` | `bool` | Elimina un worktree-project. |

El plasmoid (`plasma/applets/wordpress-panel-plasmoid/`, Plasma 6) consulta cada
3s vía `qdbus6` (DataSource `executable`) y pinta los proyectos activos con botón
de detener + "Apagar todo y cerrar". No hay "encender todos" (requisito del
usuario). Instalación: `kpackagetool6 --install` (lo hace `first-run.sh`).
El servidor arranca en el `setup` de Tauri; si no hay sesión D-Bus, el panel
sigue funcionando sin widget.

## Rutas en disco

```
~/.config/wordpress-panel/        (config_dir)
├── nginx/conf.d/{site-id}.conf    vhosts montados ro en panel-nginx
├── wp-cli.phar                    montado ro en cada wp-{id}
├── wp-versions.json               cache 24h de versiones WP
├── minio-data/                    datos del S3 compartido (panel-minio)
├── db-data/{container}/           datadir durable de cada DB compartida (bind)
├── panel.json                     estado global del panel (Endpoint elegido)
├── dump-log.jsonl                 log de volcados de DB (revisión + limpieza)
└── dnsmasq-panel.conf             snippet wildcard (referencia)

~/panel-wp/{slug}/                 (projects_root) — FUENTE DE VERDAD
├── config.json                    SiteConfig serializado
├── app/public/                    WordPress (montado en php y nginx)
├── app/sql/                       dumps de DB
├── conf/php/php.ini               montado ro como zz-project.ini
├── logs/php/                      logs
├── ssl/                           cert.pem/key.pem (mkcert, Fase 2)
└── data/                          datos locales

Sistema (fuera del repo): /etc/NetworkManager/dnsmasq.d/wordpress-panel.conf
Origen import LocalWP (solo lectura): ~/.config/Local/sites.json + ~/Local Sites/{site}/
```

## Frontend

- SvelteKit + Svelte 5 (runes: `$state`, `$derived`, `$effect`, `$props`).
- `adapter-static` modo SPA: `+layout.ts` con `ssr=false`, `prerender=false`,
  fallback `index.html`. El routing es 100% cliente.
- **Layout de 3 columnas** (estilo LocalWP): `+layout.svelte` es un **riel de
  íconos** angosto (Proyectos `/`, Dominios, Servicios, Configuración + botón «+»
  Nuevo proyecto), con la sección activa por `page.url.pathname`. La ruta `/`
  (`+page.svelte`) es un **master-detail**: columna izquierda = lista de proyectos
  agrupada (grupos de `groups.json` fusionados con `config.group`; alta de grupo
  inline, **drag&drop** nativo HTML5 de la fila de proyecto sobre la cabecera de
  grupo → `set_site_group`; power/estado como íconos; **grupos plegables** con
  estado en `localStorage` y una sección fija **"En ejecución"** que sube los
  proyectos `running` al inicio sin duplicarlos), y panel grande = el detalle
  del proyecto **seleccionado por estado** (`selectedId`, sin navegar) vía
  `{#key}<ProjectDetail/>`. Dominios/Servicios/Configuración siguen siendo páginas
  sueltas con padding estándar.
- `lib/api.ts` envuelve `invoke`. `lib/types.ts` = espejo de los modelos serde
  (incl. `Endpoint` + helper `siteUrl`).
- Componentes (`lib/components/`): `ProjectDetail.svelte` — todo el detalle de un
  proyecto (cabecera con acción primaria encender/detener + menú «···» de acciones
  secundarias + accesos rápidos; tabs Info/Logs/Plugins-Themes/GitHub/Servicios/
  Puntos de guardado; el selector de usuario de auto-login vive en el tab Info).
  Recibe `id` por prop y notifica al master-detail vía `onChanged`/`onDeleted`/
  `onSelect`. Lo monta `/` (embebido) y el wrapper `/site/[id]` (deep-link).
  `OpConsole.svelte` — consola modal que escucha
  `op-log` y muestra los pasos en vivo (botón «Cerrar» bloqueado mientras corre;
  botón «Cancelar borrado» opcional). `DeleteProjectModal.svelte` — borrado de un
  proyecto: modal de confirmación (titulado con el nombre + checkbox para borrar
  también la carpeta) y, al confirmar, `OpConsole` con la ventana de gracia de 5 s
  y `delete_site`. Se usa en `ProjectDetail` (`bind:site`).
  `ImportProjectModal.svelte` — modal del dashboard (botón «Importar proyecto»)
  que lista las carpetas desconectadas (`list_disconnected_sites`) y re-importa
  la elegida (`import_disconnected_site`) mostrando el progreso en `OpConsole`.
- Tailwind (`darkMode: 'class'`, clase `dark` en `<html>`). Tema dark-only navy
  **"DevFlow Dark Blue"** (`DESIGN.md`): la escala `zinc` está remapeada a navy
  en `tailwind.config.js`, así los `dark:bg-zinc-*` existentes heredan el tema
  sin tocarse; token `primary` (#4d8eff). Estilos base de inputs (fondo navy,
  texto claro, foco azul) globales en `app.css` (`@layer base`/`components`).
