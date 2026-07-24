# Runbook de proyectos

Este runbook cubre el ciclo de vida de un proyecto WordPress: crear, encender, detener, eliminar, migrar entre sistemas, importar de LocalWP, re-importar carpetas desconectadas, snapshot y clone, worktree, abrir en el navegador/VSCode/terminal, autologin, Adminer, Mailpit, MinIO, grupos, búsqueda de logs y administración de subidas PHP.

## Matriz de comandos por superficie

| Operación | UI (botón / ruta) | CLI (`wordpress-panel-cli`) | MCP (`mcp/server.mjs`) | D-Bus (`dbus.rs::Manager`) |
|---|---|---|---|---|
| Listar proyectos | `+page.svelte::load` (auto-actualiza con `sites-changed`) | `wordpress-panel-cli list` | `list_projects` | `ListSites` |
| Encender | botón "Encender" | `start [proyecto]` | `start_project` | `StartSite(id)` |
| Detener | botón "Detener" | `stop [proyecto]` | `stop_project` | `StopSite(id)` |
| Apagar todo | botón "Apagar todo" | `stop_all_sites` (sin CLI directo) | n/d | `StopAll` |
| Crear | ruta `/site/new` | n/d | n/d | n/d |
| Migrar y encender | botón "Migrar y encender" | n/d | n/d | n/d |
| Importar de LocalWP | ruta `/import-localwp` | n/d | n/d | n/d |
| Importar proyecto desconectado | modal `ImportProjectModal` | n/d | n/d | n/d |
| Eliminar | modal `DeleteProjectModal` | n/d | n/d | n/d |
| Puntos de guardado | tab "Puntos de guardado" | `snapshot {list,create,delete,clone}` | `list/create/delete/clone_snapshot` | `ListSnapshots`, `CreateSnapshot`, `DeleteSnapshot`, `CreateClone` |
| Worktree-project | tab "GitHub" | `worktree {list,create,remove}` | `worktree_{list,create,remove}` | `ListWorktrees`, `CreateWorktree`, `RemoveWorktree` |
| Git scan/pull/status | tab "GitHub" | `git {scan,status,pull,set-deploy,deploy}` | `git_{scan,status,pull,set_deploy,deploy}` | `GhScan`, `GhPull`, `GhBranchStatus`, `GhSetDeploy`, `GhDeploy` |
| Abrir en navegador | botón "Abrir" | `open {admin,site,folder}` | `open_project` | `OpenAdmin`, `OpenSite` |
| Terminal con wrapper | botón "Abrir terminal del proyecto" | n/d | n/d | n/d |
| VSCode | botón "Abrir en VSCode" | n/d | n/d | n/d |
| Adminer | botón "Ver base de datos" | n/d | n/d | n/d |
| Mailpit | botón "Abrir Mailpit" | n/d | n/d | n/d |
| MinIO | botón "Abrir consola MinIO" | n/d | n/d | n/d |
| SSL regenerar | menú "···" → "Regenerar SSL" | n/d | n/d | n/d |
| Tope de subida | tab "Info" → input MB | `php upload <MB>` | `set_php_upload_limit` | `SetUploadLimit` |
| Reparar auto-login | tab "Plugins/Themes" | n/d | n/d | n/d |
| Aplicar php.ini a todos | ruta `/settings` → Mantenimiento | n/d | n/d | n/d |
| Reparar nginx | ruta `/settings` → Mantenimiento | n/d | n/d | n/d |
| Ver/limpiar log de volcados | ruta `/dumps` | n/d | n/d | n/d |
| Estado del sistema | ruta `/settings` | n/d | n/d | n/d |
| Crear red panel-net | ruta `/settings` | n/d | n/d | n/d |
| Reasignar endpoint | ruta `/settings` | n/d | n/d | n/d |
| Cerrar el panel | botón "Apagar todo y cerrar" (plasmoid) | `Quit` | n/d | `Quit` |

Las herramientas de UI, CLI y MCP **no se mezclan** en una misma operación: si arrancas una desde la UI, espera a que la consola de progreso (`OpConsole`) termine antes de iniciar otra del mismo proyecto. Las mutaciones vía CLI/MCP emiten el evento `sites-changed` (`dbus.rs::notify_sites_changed`) y la UI se recarga sola (`+page.svelte::listen('sites-changed')`).

## 1. Precondiciones universales

- `panel-net` existe (`src-tauri/src/system.rs::status` lo reporta; `create_panel_network` lo crea).
- `dnsmasq` resuelve `*.test` a `127.0.0.1`.
- `mkcert` instaló la CA local para que `*.test` no muestre aviso en el navegador.
- Tienes al menos un `id` o nombre de proyecto existente; el CLI/MCP lo resuelven por subcadena case-insensitive (`scripts/wordpress-panel-cli.sh::resolve_pid`, `mcp/server.mjs::resolveProject`).

**Cero proyectos activos = cero recursos**: al detener el último proyecto, `docker.rs::DockerManager::teardown_unused_shared` apaga `panel-nginx`, `panel-mailpit` y `panel-adminer`; la DB compartida también si nadie más la usa. No hay un "apagar todo global": el equivalente en la UI es "Apagar todo" en el dashboard.

## 2. Crear un proyecto

### Precondiciones

- `docker ps` responde y `panel-net` existe.
- Versión PHP/DB/WordPress seleccionadas en el formulario (la lista de WP viene de la API de wordpress.org con cache de 24 h, `src-tauri/src/wordpress.rs::fetch_versions`).
- Credenciales admin de WP listas.

### Procedimiento

1. Abre la UI, click **+** (en el riel de íconos) o ve a `/site/new`.
2. Completa el formulario: nombre, dominio (autocompleta a `{slug}.test` y es editable), versión de WP, idioma, PHP, motor y versión de DB, admin, título, opciones (SSL, auto-login, XDebug, headless + framework opcional, MinIO, grupo).
3. Click **Crear proyecto**. La UI navega al dashboard; el proyecto aparece como `parado`.

### Lo que hace por debajo (`src-tauri/src/wordpress.rs::create_site`)

1. Crea estructura en `~/panel-wp/{slug}/` (`wordpress.rs::create_dirs`).
2. Escribe `php.ini` desde `docker/php.ini.tmpl` (`wordpress.rs::write_php_ini`).
3. Persiste `config.json` (`config::write_site_config`).
4. Arranca el motor DB compartido on-demand y crea la base vacía (`docker.rs::ensure_db` + `wordpress.rs::create_database`).
5. Descarga `wordpress-{version}.tar.gz` y lo extrae en `app/public/`.
6. Inyecta los mu-plugins del panel (`wordpress.rs::sync_mu_plugins`): `panel-mailpit.php` siempre; `panel-autologin.php` si `oneClickAdmin`.
7. Si SSL está activo, genera el cert con mkcert (`ssl::generate`). Se hace **antes** de encender para que `nginx -s reload` no falle por cert inexistente.
8. Enciende container php + vhost + recarga nginx (`docker::start_site`).
9. Genera `wp-config.php` y ejecuta `wp core install` (con `wp config set` y `wp core install`, ambos vía `wpcli::run`).
10. Devuelve la `SiteConfig` al frontend.

### Cambio esperado y evidencia

- El proyecto aparece en la lista y al encender abre en `https://{slug}.test` (o `http://` si SSL desactivado).
- `docker ps` muestra `wp-{id}` y, si SSL, el `panel-nginx`; al menos la DB compartida del motor.
- `ls -la ~/panel-wp/{slug}/{app/public,app/sql,conf/php,ssl,logs/php}` lista los subdirectorios esperados.

### Precondiciones, abortar y recuperar

- Si el slug ya existe: `wordpress::create_site` devuelve error y la UI lo muestra. Borra la carpeta o usa otro nombre.
- Si la versión PHP/DB no existe en Docker Hub: el build de la imagen falla, la creación se aborta y el panel no deja proyecto incompleto (los pasos son secuenciales).
- Si la CA de mkcert no está instalada y SSL está activo: `ssl::generate` falla al ejecutar `mkcert -cert-file … -key-file … dominio.test`. Revisa **Configuración** para confirmar el indicador; ejecuta `first-run.sh` o instala manualmente.
- Si el endpoint del panel es puerto alto (`:8443`), la URL del sitio se ve en el dashboard como `https://demo.test:8443`; el navegador acepta el cert de mkcert.

### Riesgos

- Descarga a internet obligatoria (tarball de wordpress.org, `wp-cli.phar` la primera vez, `gh` para clonar).
- Crea containers Docker y archivos en `~/panel-wp/`.
- Si el dump a importar de un proyecto existente se restaura en `app/sql/imported.sql`, no se sobrescribe `local.sql`/dumps previos.

## 3. Encender / detener un proyecto

### Precondiciones

- `panel-net` existe.
- `dnsmasq` resuelve `*.test` a `127.0.0.1`.

### Procedimiento para encender

- **UI**: botón **Encender** en la tarjeta o en `ProjectDetail`.
- **CLI**: `wordpress-panel-cli start mi-sitio` (en el CWD del proyecto, basta `start`).
- **MCP**: `start_project(project="mi-sitio")`.
- **D-Bus**: `StartSite("id")`.

### Lo que ocurre (`docker::start_site`)

1. `ensure_network`.
2. `ensure_db` (arranca la DB compartida si hace falta y bindea su datadir a `config_dir/db-data/{container}`; `db.rs::db_data_dir`).
3. `ensure_mailpit` (best-effort, queda corriendo para todos los activos).
4. `ensure_minio` solo si el proyecto lo activa (`site.minio`).
5. `ensure_php_image` (construye `panel-php:{ver}-r3` la primera vez; la revisión está en `src-tauri/src/php.rs::IMAGE_REV`).
6. Si el container ya existe con otra imagen (por cambio de revisión), lo recrea.
7. `create_php_container` (con montajes especiales si es worktree, ver §10).
8. `nginx::write_vhost` + `ensure_nginx` + `reload_nginx`.
9. `domain::ensure_wildcard`.
10. Engancha el watcher de auto-dump (`autodump::AutoDump::start`) que persiste `app/sql/db-*.sql` cuando hay escrituras reales en la DB.

### Procedimiento para detener

- **UI**: botón **Detener**.
- **CLI**: `wordpress-panel-cli stop mi-sitio`.
- **MCP**: `stop_project`.
- **D-Bus**: `StopSite(id)`.
- **Apagar todos**: botón en el dashboard (icono power del header), comando IPC `stop_all_sites` o `StopAll` por D-Bus.

### Lo que ocurre al detener (`docker::stop_site`)

1. Si está corriendo, `backup::export_db` deja un dump fresco en `app/sql/db-{timestamp}.sql` y lo registra en `dumplog::append` con `source="stop"`.
2. `backup::rotate_dumps(site, 3)` deja los 3 dumps más recientes; los `imported.sql`/`local.sql` no se tocan.
3. `stop_container`.
4. `nginx::remove_vhost` + `reload_nginx`.
5. `teardown_unused_shared` apaga DB compartida, `panel-minio`, `panel-nginx`, `panel-mailpit` y `panel-adminer` si nadie más los usa.
6. `autodump::AutoDump::stop(id)` cancela el watcher de ese proyecto.

### Cambio esperado y evidencia

- `docker ps` muestra `wp-{id}` cuando está encendido y lo omite al detener.
- `app/sql/db-*.sql` aparece tras detener.
- Si era el último proyecto activo, `panel-nginx` también desaparece.

### Abortar

- En la UI: pulsa el botón de nuevo (los comandos están en cola si llegan casi simultáneos, pero el panel no reembolsa un stop ya iniciado).
- En CLI: `Ctrl+C` no aborta; el comando espera a que termine `docker stop` (timeout 10 s en `docker.rs::stop_site`).
- `stop_all_sites` recorre todos los proyectos; durante la iteración un error en uno no detiene el resto (`docker::stop_site(...).await.ok()`).

### Recuperar tras un stop fallido

- Si `panel-nginx` no arranca, mira `docs/resume/operacion/07-diagnostico-y-mantenimiento.md` antes de borrar `panel.json`.
- Si el dump-al-detener no se generó, espera al próximo cambio real en la DB y el watcher de auto-dump lo creará; mientras tanto, enciende y apaga de nuevo tras un minuto de actividad.

## 4. Abrir el sitio, el admin, la carpeta y la terminal

### Precondiciones

- Proyecto encendido para `open site`, `open admin` y `open terminal`.
- `gh` instalado si vas a usar GitHub.

### Procedimiento

- **UI**:
  - Botones de acción rápida en `ProjectDetail` (Web, Admin, Carpeta).
  - Para la terminal, botón **Abrir terminal del proyecto** en el tab "Servicios".
- **CLI**: `wordpress-panel-cli open admin|site|front|folder`.
- **MCP**: `open_project(project, what="admin"|"site"|"folder")`.

### Cambio esperado y evidencia

- `open admin`: el navegador abre `https://{slug}.test/wp-admin/?panel_autologin={token}` y entra logueado. `autologin::open_admin` genera un transient de WP (60 s, un solo uso) con el `user_id` del selector; el mu-plugin `panel-autologin.php` lo valida y aplica `wp_set_auth_cookie`. Sin `oneClickAdmin=true` o si el proyecto fue importado antes del mu-plugin, el autologin no funciona: usar **Reparar auto-login** o `repair_autologin`.
- `open site|front`: abre la home sin autologin (`docker::is_running` debe devolver true; el helper `endpoint::site_url` aplica el puerto si no es 80/443).
- `open folder`: `xdg-open` sobre `site.path`.
- `open terminal`: el wrapper `wp` ya está instalado por el setup del panel y abre un emulador (`konsole`, `gnome-terminal`, `xfce4-terminal`, `kitty`, `alacritty`, `x-terminal-emulator`) con cwd en la carpeta del proyecto.

### Abortar y recuperar

- `open admin` puede fallar si el proyecto está recién encendido y nginx aún no propagó la config: recarga manualmente.
- Si el mu-plugin no está en `wp-content/mu-plugins/`, ejecuta `repair_autologin` (no requiere proyecto encendido).
- `open terminal` falla si ningún emulador está instalado; instala `konsole` o equivalente.

## 5. Auto-login con selector de usuario

`src-tauri/src/autologin.rs::open_admin` acepta `userId?: Option<u64>`. El frontend (`ProjectDetail.svelte::loadWpUsers`) llama `list_wp_users` para listar usuarios y los muestra en el `<select>` pegado al botón; persiste la selección en `localStorage` con clave `wp-panel:autologin:<id>`. La opción vacía = primer administrador (retrocompatible).

### Precondiciones

- Proyecto encendido, `oneClickAdmin=true`, mu-plugin actualizado.
- Si el proyecto es de LocalWP, **Reparar auto-login** antes o ejecuta `repair_autologin` (`repairAllPhpIni` no lo cubre).

### Procedimiento

1. Tab "Info" del proyecto; en la fila "One-click admin" verás el `<select>`.
2. Selecciona un usuario (o déjalo en blanco para primer admin).
3. Pulsa **Abrir admin**.

### Cambio esperado

- El navegador abre el admin como ese usuario exacto. Si el usuario no tiene `manage_options`, redirige a `home_url('/')`.

### Recuperar

- Si la lista de usuarios no carga (proyecto apagado o `oneClickAdmin=false`), no aparece el selector. Habilita `oneClickAdmin` en la fila correspondiente o enciende el proyecto.
- Si la redirección se va al home en vez de al admin, es porque el usuario seleccionado no es administrador.

## 6. Eliminar un proyecto

### Precondiciones

- Proyecto existe en la lista maestra (o como `pendiente de migración`).

### Procedimiento

1. Botón **Eliminar** en la tarjeta o menú "···" → "Eliminar" en el detalle.
2. Aparece el modal `DeleteProjectModal`. Marca o desmarca **Borrar también la carpeta del proyecto en disco** (la opción sin marcar es la antigua "desconexión").
3. Confirma. Se abre `OpConsole` con cuenta atrás de 5 s; el botón **Cancelar borrado** permite abortar.
4. Pasados los 5 s, el panel ejecuta `delete_site(id, deleteFolder)`.

### Lo que ocurre (`lib.rs::delete_site`)

1. `docker::stop_site` (que ya deja un dump fresco en `app/sql/db-*.sql`).
2. `docker::remove_container` (asegura que no quede container php huérfano).
3. `docker::ensure_db` + `wordpress::drop_database` (`DROP DATABASE` del esquema del proyecto en el motor compartido).
4. `docker::teardown_unused_shared` apaga la DB compartida si ya nadie la usa.
5. `deleteFolder=true` → `std::fs::remove_dir_all(site.path)`.
6. `deleteFolder=false` → renombra `config.json` a `config.disconnected.json` (`config::disconnected_config_path`). El proyecto desaparece del panel pero su carpeta y metadata quedan para re-importar.

### Cambio esperado y evidencia

- La tarjeta desaparece de la lista. La carpeta del proyecto está borrada (`deleteFolder=true`) o conservada con `config.disconnected.json` y sin `config.json` (`false`).
- La DB del proyecto ya no existe en el motor (`mysql -uroot -ppanel -e "SHOW DATABASES"` la omite).
- `app/sql/db-*.sql` existe (export-al-detener) si el proyecto estaba encendido al borrar.

### Abortar

- Durante los 5 s: click **Cancelar borrado** en la `OpConsole`. No se hace nada.
- Pasados los 5 s: no se puede abortar desde la UI. Apaga manualmente y vuelve a crear.
- En CLI/MCP no hay comando de borrado; usa la UI.

### Recuperar

- `deleteFolder=true`: recuperación vía backup externo (si tenías un snapshot, restáuralo).
- `deleteFolder=false`: abre el modal **Importar proyecto** en el dashboard, selecciona la carpeta y re-importa. Si la metadata era un `config.disconnected.json`, aparece como `preserved` (config conservada).
- Si eliminaste un proyecto equivocado que aún tiene carpetas intactas, usa `wordpress-panel-cli` solo para inspección; el comando `delete` no existe en el CLI, así que mantén la UI a mano.

### Riesgos

- El `DROP DATABASE` borra el esquema de la DB compartida; si otro proyecto activo usa el mismo nombre de esquema (no debería: el esquema = `{slug}_db`), lo pierde.
- `remove_dir_all` borra todo bajo `~/panel-wp/{slug}/`, incluyendo dumps `app/sql/`, config `php.ini`, cert SSL y mu-plugins.

## 7. Grupos

- **Asignar**: drag&drop de la fila del proyecto sobre la cabecera del grupo (`+page.svelte`). Si el grupo destino es nuevo, el backend lo registra en `groups.json` (`lib.rs::set_site_group` → `groups::create`).
- **Crear**: botón **+** al lado de la sección de grupos.
- **Renombrar / borrar / reordenar**: comandos IPC `rename_group`, `delete_group`, `reorder_groups`. Los grupos vacíos persisten con su orden.
- La pertenencia también se puede asignar programáticamente con `set_site_group(id, "Nombre")` desde un agente MCP/CLI que invoque el D-Bus correspondiente, pero hoy no hay método D-Bus para eso (la D-Bus se diseñó para el plasmoid, no para grupos).

## 8. Snapshots (puntos de guardado)

### Precondiciones

- Proyecto existente. El engine DB debe estar disponible (se arranca automáticamente en el paso [1/3] del snapshot).

### Procedimiento

- **UI**: tab "Puntos de guardado" → botón **Punto de guardado** (en el menú "···") o desde el tab.
- **CLI**: `wordpress-panel-cli snapshot create "etiqueta"`.
- **MCP**: `create_snapshot(project, label)`.
- **D-Bus**: `CreateSnapshot(id, label)`.

### Lo que ocurre (`snapshot::run`)

1. `ensure_db` arranca el motor si no estaba.
2. `backup::export_db_to` a `snapshots/{id}/db.sql`.
3. `tar --zstd -cf code.tar.zst` con exclusiones fijas (`./wp-content/uploads`, `./wp-content/cache`, `./wp-config.php`, `./*.log`) + `site.snapshot_excludes` (configurables en "Exclusiones").
4. Escribe `meta.json` con id, label, fecha, dbName, dbType, codeBytes, dbBytes, excludes.
5. Tolerancia a `tar` con código 1: se considera "avisos no fatales" (cache/logs mutan durante la copia); código 2+ aborta y borra el snapshot parcial.

### Cambio esperado y evidencia

- `~/panel-wp/{slug}/snapshots/{id}/` contiene `code.tar.zst`, `db.sql` y `meta.json`.
- El listado (UI/CLI/MCP/D-Bus) muestra la entrada con tamaño en MB.

### Exclusiones

- Tab "Puntos de guardado" → sección "Exclusiones" plegable.
- `detect_excludable` escanea `wp-content` (excepto `uploads`/`cache`) y carpetas de backup conocidas (`updraft`, `ai1wm-backups`, `wpvividbackups`, `backups-dup-lite`, `backups-dup-pro`, `backuply`, `wp-snapshots`).
- Marcar y pulsar **Guardar** persiste en `config.json` (`site.snapshot_excludes`).
- Las exclusiones se heredan al crear clones.

### Abortar y recuperar

- Si `tar` falla, el snapshot parcial se borra y la UI/CLI reporta el error.
- Un snapshot colgado durante el dump (DB parada) se queda con `db.sql` correcto pero `code.tar.zst` parcial; reintenta con la DB encendida.

### Eliminar y listar

- UI: botón **Borrar** en cada fila del listado.
- CLI: `wordpress-panel-cli snapshot list` y `snapshot delete <id>`.
- MCP/D-Bus: análogos.

## 9. Clones temporales desde un snapshot

### Precondiciones

- Snapshot existente (ver §8).

### Procedimiento

- **UI**: botón **Clonar desde aquí** en la fila del snapshot.
- **CLI**: `wordpress-panel-cli snapshot clone <snapshotId>`.
- **MCP**: `clone_snapshot(project, snapshotId)`.
- **D-Bus**: `CreateClone(parentId, snapshotId)`.

### Lo que ocurre (`clone::run`)

1. Carga el snapshot y deriva un slug libre: `{parent_dirname}-{label_slug}` con desambiguación `-N` (`clone::find_free_slot`).
2. Crea carpeta, `php.ini`, `config.json` con `clone_of` poblado y nombre = `meta.label` (no `"{padre} (clone)"`).
3. Extrae `code.tar.zst` en `app/public/`. Crea `app/public/wp-content/uploads/` vacío (rw para nuevos).
4. Crea la DB del clone (esquema separado `{slug}_db`).
5. Importa el dump del snapshot (`migrate::import_dump`).
6. Inyecta mu-plugins del panel; genera SSL si aplica.
7. Enciende el container y nginx; ajusta `home`/`siteurl` con `wp option update` (`migrate::fix_site_url`).
8. nginx sirve los uploads viejos desde el padre vía `try_files $uri @uploads_base` (ver `nginx::render_vhost` cuando `site.clone_of.is_some()`).

### Cambio esperado y evidencia

- Aparece un nuevo proyecto con badge ámbar en el dashboard, anidado bajo el padre.
- `docker ps` muestra `wp-{clone-id}` y, si es el único activo de su motor, su DB compartida.
- Al subir un archivo en el clone, aterriza en `~/panel-wp/{clone-slug}/app/public/wp-content/uploads/` y **no** en el padre.

### Abortar y recuperar

- Si la importación del dump se cuelga, se respeta el `IMPORT_IDLE_TIMEOUT` (3 min) y `reset_database` deja la DB vacía; reintenta.
- `find_free_slot` tiene fallback con UUID corto; si tu snapshot tiene una etiqueta muy larga, recorta el slug resultante.

### Eliminación

- Trátalo como un proyecto cualquiera (§6). `delete_site` apaga + quita vhost + `DROP DATABASE` de `{slug}_db`. Los uploads nuevos del clone se borran con la carpeta. El padre no se toca.

### Limitaciones documentadas

- `try_files` cubre lectura web de media vieja, no lectura por filesystem desde PHP (p. ej. regenerar thumbnails que escanea el dir de uploads del clone, que solo tiene los nuevos).
- Al pausar (stop) los uploads nuevos se conservan; al destruir (borrar) se van con la carpeta.

## 10. Worktrees (probar una rama de un repo en aislamiento)

### Precondiciones

- Proyecto padre encendido.
- Repo git clonado en `wp-content/{themes|plugins|...}/{repo}` (`gh_clone` o `gh_register`).
- El repo debe tener al menos un commit (los repos vacíos no crean worktree).

### Procedimiento

- **UI**: tab "GitHub" del padre → sección "Worktrees" → formulario "Nuevo worktree" con nombre de rama, opcional `--target` (si no, infiere del CWD del editor) y opcional `--base`.
- **CLI**: `wordpress-panel-cli worktree create feature/rama [--target wp-content/themes/mi-theme] [--base main] [--copy-db]`.
- **MCP**: `worktree_create(project, branch, target?, base?, copyDb?)`.
- **D-Bus**: `CreateWorktree(parentId, targetPath, branch, baseBranch, sharedDb)`.

### Lo que ocurre (`worktree::run_create`)

1. Carga el padre y valida que `targetPath` sea un repo git.
2. Deriva slug libre: `{parent_dirname}-{branch_slug}` con desambiguación (`worktree::find_free_slot`).
3. Prepara carpeta: `create_dirs`, `write_php_ini`, `worktree_root` (`{path}/wt/`), `worktree_wp_config` (`{path}/wp-config.php` con `<?php\n` inicial; se rellena luego con `wp config create`).
4. `git worktree prune` (idempotente) y `git worktree add -b {branch} {dest} [{base}]`. Si la rama ya existe, reintenta sin `-b` para hacer checkout de la existente.
5. DB: `shared_db=true` (default) usa la del padre; `shared_db=false` clona la DB (`backup::dump_bytes` del padre + `import_dump` en el esquema del worktree).
6. SSL si aplica.
7. `docker::start_site` con montajes especiales (`docker::create_php_container` cuando `site.worktree_of.is_some()`):
   - `parent_public:/var/www/html` (raíz, montado antes).
   - `wt_target:/var/www/html/{targetPath}` (el worktree encima).
   - `worktree_wp_config:/var/www/html/wp-config.php` (encima del padre).
   - `php.ini` y `wp-cli.phar` como en cualquier proyecto.
8. `wp config create` con las credenciales del worktree. Si `shared_db`, además `wp config set WP_HOME` y `WP_SITEURL` con `--type=constant` (la DB del padre sigue apuntando a su dominio).
9. `fix_site_url` solo si `shared_db=false`.

### Cambio esperado y evidencia

- Aparece un nuevo proyecto con badge violeta "Worktree" en el dashboard, anidado bajo el padre.
- La URL del worktree (`{padre}-{rama}.test`) muestra el sitio con la rama activa en el repo objetivo.
- En nginx, `root` apunta al `public` del padre pero el `location` regex sirve los assets del objetivo desde el worktree (`nginx::render_vhost` con `site.worktree_of`).
- `~/panel-wp/{padre}/app/public/wp-content/themes/mi-theme/` sigue en su rama; `~/panel-wp/{padre-worktree-feat}/wt/mi-theme/` es la rama nueva.

### Abortar y recuperar

- Si el worktree se creó a medias, el `run_create` envuelve todo en un `build.await` y, ante error, limpia container + vhost + carpeta (`worktree.rs::run_create::catch`).
- Si la rama tiene espacios o caracteres no válidos (`feature/x y` o `git checkout -b feature/x`), el panel rechaza y sugiere la rama extraída del pegado (`worktree::invalid_branch_reason` + `worktree::guess_branch`).
- Si el repo está sucio en el padre, el `git worktree add` puede fallar; revísalo desde el editor.

### Eliminación

- **UI**: botón **✕** en la fila del worktree.
- **CLI**: `wordpress-panel-cli worktree remove <id> [--delete-branch]`.
- **MCP**: `worktree_remove(project, worktreeId, deleteBranch)`.
- **D-Bus**: `RemoveWorktree(id, deleteBranch)`.

`worktree::remove_worktree` apaga el worktree, quita vhost/container, hace `git worktree remove --force`, y si `!shared_db` borra el esquema del worktree (¡nunca si era compartida!). La rama del padre se conserva salvo `deleteBranch=true`.

## 11. Despliegue directo (git pull + build) por repo

### Precondiciones

- Repo registrado (`github.repos`).
- Rama objetivo definida; comando de build opcional; carpetas de build opcionales (vacío = raíz del repo).

### Procedimiento

- **UI**: tab "GitHub" → panel "Deploy ▾" en la fila del repo. Define rama/comando/carpetas, pulsa **Pull + build**.
- **CLI**: `wordpress-panel-cli git set-deploy --branch main --build "pnpm install && pnpm build" --dirs dist` (una vez); luego `git deploy`.
- **MCP/D-Bus**: análogos (`git_set_deploy`, `git_deploy`).

### Lo que ocurre (`github::deploy`)

1. `git checkout {branch}` (si el árbol está sucio, falla con sugerencia de abrir el editor).
2. `git pull --ff-only origin {branch}`. Si diverge (hay commits locales por delante), falla con el error y recomienda resolver desde el editor.
3. Si hay `build_cmd` no vacío, ejecuta `sh -lc {cmd}` en cada `build_dirs` (raíz o subcarpetas). `-lc` carga el perfil del usuario (nvm/pnpm).
4. Emite cada línea por `op-log` y reporta éxito o fallo con el código de salida.

### Cambio esperado y evidencia

- El código del repo en `wp-content/{ruta}` queda en la rama objetivo, al día con el remoto (salvo `--ff-only` falle), y los artefactos de build (si aplica) están en `dist/`, `build/`, etc.
- En el dashboard no hay un badge específico; en la UI de deploy, el resumen de `branch_status` muestra `behind: 0` y `canPull: false` tras el pull exitoso.

### Abortar y recuperar

- `Ctrl+C` no aborta; el deploy espera al `git checkout`/`pull`/build.
- Si el pull diverge: `git -C {ruta} status` y `git log` para entender, luego `git pull --rebase` o merge desde el editor, y reintenta el deploy.
- Si el build falla, edita la config de deploy (cambia comando/carpetas) y reintenta; el código del pull sigue en la rama objetivo.

## 12. Logs de un container

- **UI**: tab "Logs" del proyecto (Stream de `log:{id}` vía Tauri events).
- **CLI**: `wordpress-panel-cli logs [php|db|nginx|mailpit|minio] [-f] [-n 200]`.
- **MCP**: `project_logs(project, service="php", lines=200)`.

El servicio `php` se resuelve al `wp-{id}`; los demás vienen de `dbus.rs::Manager::project_containers` filtrando por `role`.

## 13. Recursos (docker stats)

- **CLI**: `wordpress-panel-cli resources` (sin UIs).
- **MCP**: `project_resources(project)`.

`scripts/wordpress-panel-cli.sh::resources` hace `docker inspect` por nombre de container y ejecuta `docker stats --no-stream` sobre los existentes. Si no hay containers del proyecto corriendo, devuelve `exit 1`.

## 14. Volcados de DB (manual, log, auto-dump, limpieza)

### Manual

- **UI**: tab "Info" → **Exportar DB** o comando `export_db`.
- **CLI**: no hay subcomando dedicado (usa el IPC `export_db` desde un agente).

El dump va a `app/sql/db-{timestamp}.sql` y se registra en `dump-log.jsonl` con `source="manual"`.

### Auto-dump (`autodump.rs::watch`)

Se engancha en `start_site` y en el `setup` de Tauri. Sondea cada 20 s con `SHOW GLOBAL STATUS WHERE Variable_name IN ('Innodb_rows_inserted','Innodb_rows_updated','Innodb_rows_deleted')`; si la suma de ins/upd/del cambió desde el último sondeo, hace `backup::dump_bytes` y compara el hash. Si difiere del último persistido, escribe `db-{stamp}.sql`, lo registra con `source="auto"` y rota dejando los 3 más recientes.

La línea base se siembra desde el último dump en disco (`autodump::latest_dump_hash`): una edición hecha al arrancar (o con el panel cerrado) se detecta en el primer sondeo y se vuelca.

### Export-al-detener

`docker::stop_site` llama a `backup::export_db` antes de parar el container y registra la entrada con `source="stop"`. `backup::rotate_dumps(site, 3)` deja los 3 más recientes y no toca `imported.sql`/`local.sql`.

### Log de volcados y limpieza

- **UI**: ruta `/dumps` (tabla más nuevos primero; `dumpEntries` y limpieza con confirmación por `confirm()`).
- **CLI**: no hay subcomando, pero el IPC está disponible para agentes.

`dumplog::clean(before?, dbName?)` reescribe el JSONL conservando las entradas que no cumplen todos los filtros dados; el archivo `app/sql/*.sql` no se toca (lo cuida `rotate_dumps`).

### Cambio esperado y evidencia

- `cat ~/.config/wordpress-panel/dump-log.jsonl` muestra las entradas; `ls ~/panel-wp/{slug}/app/sql/` lista los dumps `db-*.sql`.

### Abortar y recuperar

- La limpieza del log es reversible solo desde un backup del propio JSONL (no se hace automáticamente).
- Si necesitas espacio en disco, `rotate_dumps` ya purga los `db-*.sql` viejos al rotar; el JSONL puede crecer si lo dejas sin podar.

## 15. Tope de subida por proyecto (413)

### Precondiciones

- Proyecto existente (encendido o no).

### Procedimiento

- **UI**: tab "Info" → fila "Tope de subida" → input en MB → **Guardar**.
- **CLI**: `wordpress-panel-cli php upload <MB>` (0 = default 64M).
- **MCP**: `set_php_upload_limit(project, mb)`.
- **D-Bus**: `SetUploadLimit(id, mb)`.

### Lo que ocurre

- `lib.rs::set_php_upload_limit`:
  1. Persiste `services.php.uploadMaxMb` en `config.json` (None si 0).
  2. `wordpress::write_php_ini` reescribe el `php.ini` del proyecto (anexa `upload_max_filesize = {mb}M` + `post_max_size = {mb}M`).
  3. Si el proyecto está encendido, `docker exec` con `kill -USR2 1` recarga `php-fpm` en caliente (SIGUSR2 es la señal estándar de reload de `php-fpm`).
- nginx, por su parte, no pone límite de body (`nginx.rs::ensure_tuning` escribe `client_max_body_size 0` en `00-panel-tuning.conf`), por lo que el tope real lo pone PHP.

### Cambio esperado y evidencia

- En `php.ini` del proyecto aparece la línea de override.
- Una subida de un theme de ~50 MB no se corta con 413.

### Abortar y recuperar

- `php -m` o `wp status` desde el wrapper confirman la nueva configuración.
- Si nginx devuelve 413 antes que PHP, ejecuta `bash scripts/first-run.sh` para reinstalar el tuning o revisa `~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf`.

## 16. Grupos y drag&drop (detalles)

- Los grupos viven en `config_dir/groups.json` (orden + lista durable). La pertenencia está en `site.group` (string opcional).
- Crear un grupo inline (`+page.svelte::createGroup`) es idempotente.
- Renombrar (`groups::rename`) reescribe `site.group` de los proyectos afectados.
- Borrar (`groups::delete`) deja los proyectos sin grupo.
- Reordenar (`groups::reorder`) sobrescribe el orden. Si haces reorder con grupos que no existen en `groups.json`, se conservan los huérfanos que ya estuvieran en disco.
- El drag&drop es nativo HTML5; si el navegador lo deshabilita (p. ej. accesibilidad), usa el formulario del tab "Info" del proyecto o los comandos IPC.

## 17. Reparaciones y mantenimiento

- **Reparar auto-login** (`lib.rs::repair_autologin`): activa `oneClickAdmin` y reinyecta los mu-plugins del panel (`wordpress::sync_mu_plugins`). No requiere proyecto encendido. Es idempotente. Uso típico: proyectos importados de LocalWP antes del fix.
- **Reparar php.ini** (`lib.rs::repair_all_php_ini`): regenera el `php.ini` de TODOS los proyectos desde `docker/php.ini.tmpl`. Devuelve `"{ok}/{total} proyectos. Reinicia los que estén encendidos."`. Tras correrlo, los proyectos encendidos deben reiniciarse para que el cambio aplique (stop + start). Disponible en **Configuración → Mantenimiento**.
- **Reparar nginx** (`lib.rs::repair_nginx`): `docker::prune_orphan_vhosts` + recrea `panel-nginx`. Úsalo cuando ningún sitio carga tras un apagón sucio (un upstream `wp-{id}` caído aborta el arranque de nginx con "host not found in upstream"). El panel corre esta poda automáticamente al arrancar nginx (`ensure_nginx`), pero la expuesta manualmente es la manera de forzar la recuperación.

## 18. Stubs de Fase 3

`lib.rs::feature_stub` devuelve un error con texto informativo para `cloudflare`/`deploy`/`package`. La UI los pinta como botones en el tab "Servicios"; no toques esos botones esperando acción. El **deploy real** existe como `gh_deploy` (§11) y es por repo, no por sitio.

## 19. Criterio de salida

- El proyecto arranca, el sitio abre en el navegador y la consola `OpConsole` no muestra líneas `✗`.
- En estado normal, `docker ps` solo muestra containers de proyectos encendidos + servicios compartidos en uso.
- En la UI de **Configuración** todos los indicadores del sistema son verdes.
- Si el proyecto es nuevo, ejecuta `pnpm test:e2e` o `cargo test` para confirmar que la refactorización no rompió nada.

Pasar al ciclo de migración / import / DR se cubre en `04-runbook-importacion-migracion-y-recuperacion.md`.
