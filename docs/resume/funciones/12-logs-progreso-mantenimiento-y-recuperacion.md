# 12 · Logs, progreso, mantenimiento y recuperación

Cubre los flujos que mantienen los proyectos utilizables en el día a día:
streaming de logs del container, consola de progreso para operaciones
largas, auto-dump (protección frente a apagón), export-al-detener,
log de volcados, mantenimiento del sistema, y los caminos de
recuperación ante fallos.

## Resultado para el usuario

- **Ver logs en vivo** del container php (`tab Logs` en
  `ProjectDetail.svelte`). Stream `follow` con las últimas 200 líneas y
  autoscroll del buffer (máx 500 en UI).
- **Consola de progreso** (`OpConsole.svelte`) para operaciones largas:
  migración, import LocalWP, import disconnected, snapshot, clone,
  worktree, deploy directo, borrado. Muestra líneas en vivo con
  contador/ barra reescritos en sitio.
- **Auto-dump** durante la vida del proyecto: cada 20 s se sondea la DB;
  si hubo escrituras (`SHOW GLOBAL STATUS Innodb_rows_*`), se vuelca y
  se persiste solo si el hash cambió. Mantiene los últimos 3 dumps en
  `app/sql/db-{ts}.sql` (`rotate_dumps`).
- **Export-al-detener**: al detener un proyecto ordenadamente, se deja
  un dump fresco en `app/sql/`.
- **Export manual** desde el tab Servicios (`Ver base de datos (Adminer)`,
  `Exportar base de datos`).
- **Log de volcados** en `/dumps`: cada dump escrito queda registrado
  (auto / stop / manual); se puede revisar y limpiar (sin tocar los
  `.sql`).
- **Estado del sistema** en `/settings`: checklist de prerequisitos
  (Docker, red `panel-net`, dnsmasq wildcard, CA mkcert, wrappers WP-CLI,
  plasmoid) + endpoint + rutas + acción «Regenerar php.ini en todos».
- **Reparar auto-login** en proyectos importados de LocalWP (reinyecta
  los mu-plugins del panel).
- **Regenerar SSL** desde el menú «···» del proyecto.

## Precondiciones

- **Logs**: requieren el container `wp-{id}` corriendo
  (`site.status === 'running'`). Si no, la pestaña muestra «Enciende el
  proyecto para ver logs en vivo».
- **Auto-dump / export-al-detener**: requieren el motor DB compartido
  corriendo. Si se apaga, el watcher salta ese ciclo (`autodump.rs::watch`,
  `if !docker.is_running(...).await { continue; }`).
- **Capacidad de evento**: `core:event:default` en
  `src-tauri/capabilities/default.json` (ver
  `docs/ARCHITECTURE.md §"Capability obligatoria para eventos"`). Sin
  ella, `OpConsole` y el tab Logs salen vacíos.
- **Mailpit / MinIO**: ver `docs/ARCHITECTURE.md`. Mailpit arranca con
  cualquier proyecto activo; MinIO solo si el proyecto tiene
  `minio = true`.
- **`/settings`**: la acción «Reasignar puerto» requiere apagar todos
  los proyectos antes (los `siteurl` guardan el puerto actual).

## Flujo feliz (numerado)

### Logs en vivo (tab Logs)

1. UI: al entrar al tab `logs`, `$effect` (`ProjectDetail.svelte:352`)
   verifica `site.status === 'running'`. Si lo está, llama
   `startLogs()`.
2. `startLogs()` (`ProjectDetail.svelte:334`):
   - `listen<string>('log:{id}', ...)` se suscribe al canal
     `logs::event_name(id)`.
   - `api.streamLogs(id)` → `stream_logs(id)` (`lib.rs::468`):
     - Estado `LogStreams` (managed): si ya hay stream para ese id,
       no-op (dedup).
     - `logs::spawn_stream(app, id)` (`logs.rs::20`) hace
       `docker.raw().logs("wp-{id}", LogsOptions{ follow: true,
       stdout: true, stderr: true, tail: "200" })` (bollard) y emite
       cada item como `app.emit("log:{id}", line)`.
3. El listener del frontend acumula las líneas en `logLines[]` (máx 500,
   FIFO).
4. Al cambiar de tab o desmontar: `stopLogs()` (`ProjectDetail.svelte:343`)
   hace `api.stopLogs(id)` + `unlisten?.()`. `stop_logs`
   (`lib.rs::481`) hace `JoinHandle::abort()` del watcher.
5. En `onDestroy` del componente: `stopLogs()` también.

### Consola de progreso (`OpConsole.svelte`)

1. La consola se monta en `onMount` y se suscribe al evento `op-log`
   (`progress::EVENT`, `progress.rs::10`).
2. La línea que llega se clasifica:
   - Si empieza con `` (`PROGRESS_PREFIX`): línea «viva» —
     reemplaza la última línea viva en sitio
     (`OpConsole.svelte:43-54`).
   - Si no: se apila en `lines[]` (máx 300).
3. El frontend abre la consola en operaciones largas (`snapshot`,
   `clone`, `worktree`, `migrate`, `import_localwp`,
   `import_disconnected`, `delete` con ventana de gracia).
4. El backend emite con `progress::log` y `progress::log_progress`
   (`progress.rs::22,29`).
5. Auto-scroll al final cuando llegan líneas (`$effect` en
   `OpConsole.svelte:68`).

### Auto-dump

1. Al encender un proyecto (`start_site`, `lib.rs::67`), se engancha un
   watcher con `autodump.start(site)` (`autodump.rs::36`). Al setup del
   panel (`lib.rs::988`), también se enganchan los contenedores que ya
   estuvieran activos antes de abrir el panel.
2. El watcher (`autodump::watch`, `autodump.rs::55`) corre cada 20 s
   (`POLL`):
   - Si el motor DB no está corriendo, salta el ciclo.
   - Gate barato: `write_counter(db_container, db_type)` consulta
     `SHOW GLOBAL STATUS Innodb_rows_*` y suma los contadores
     (`Innodb_rows_inserted/updated/deleted`). Si el contador no cambió,
     no vuelca. Para Postgres no hay gate fiable; se confía en el hash.
   - Si hubo escrituras (o sin gate): `backup::dump_bytes(site)` →
     captura `mysqldump --single-transaction --no-tablespaces
     --skip-dump-date {db}` dentro del container DB. Hash del volcado.
   - Si el hash coincide con el último dump en disco (`latest_dump_hash`),
     no escribe.
   - Si difiere: persiste `app/sql/db-{ts}.sql` +
     `dumplog::append(site, file, "auto")` +
     `backup::rotate_dumps(site, 3)` (mantiene los 3 más recientes).
3. Línea base del hash se siembra al arrancar el watcher desde el último
   dump en disco (no desde cero), para detectar cambios que ocurrieron
   con el panel cerrado.
4. Al detener (`stop_site`, `lib.rs::79`): `autodump.stop(id)` aborta el
   handle antes del `docker::stop_site` (este último ya hace el
   export-al-detener final).

### Export-al-detener

1. `docker::stop_site(site, all)` (`docker.rs::779+`):
   - Para `wp-{id}`.
   - `nginx::remove_vhost` + reload.
   - `teardown_unused_shared(site, all)` apaga DB / nginx / mailpit /
     minio si ya no quedan proyectos activos.
2. Antes del teardown, `backup::export_db(docker, site)` se llama para
   dejar un dump fresco en `app/sql/db-{ts}.sql` (lo registra
   `dumplog::append(site, file, "stop")`).
3. El dump queda en disco aunque se haya apagado el motor DB (es un
   archivo, no un recurso vivo).

### Export manual

- UI: tab `Servicios` → «Exportar base de datos» →
  `api.exportDb(id)` → `export_db(id)` (`lib.rs::588`) →
  `backup::export_db` + `dumplog::append(site, file, "manual")`.
- Devuelve la ruta del `.sql` generado.

### Log de volcados (`/dumps`)

1. Carga: `api.dumpLog()` → `dump_log` (`lib.rs::598`) →
   `dumplog::read_all` (`dumplog.rs::65`) lee
   `~/.config/wordpress-panel/dump-log.jsonl` (JSONL) y devuelve más
   nuevos primero.
2. UI: tabla con timestamp, siteName, dbName, file, bytes, source
   (`auto`/`stop`/`manual` con etiqueta en español).
3. **Limpieza**: tres botones:
   - «Borrar por fecha» (`<input type="date">` → `cleanDumpLog(before,
     null)`).
   - «Borrar por base» (`<select>` con `dbNames` derivados → `null,
     dbName`).
   - «Borrar todo» (`cleanDumpLog(null, null)`).
4. Cada filtro borra entradas que cumplen **TODOS** los filtros
   (`dumplog::clean`, `dumplog.rs::84`). Sin filtros = borra todo.
5. **No toca los `.sql`**: solo el log; los archivos siguen hasta
   `backup::rotate_dumps(site, 3)` del auto-dump (que solo afecta
   `db-{ts}.sql` del propio proyecto).

### Estado del sistema (`/settings`)

1. Carga: `api.systemStatus()` → `system_status` (`lib.rs::135`) →
   `system::status` (`system.rs::33`):
   - `docker_ok` = `DockerManager::connect().is_ok()`.
   - `network_ok` = la red `panel-net` existe.
   - `dnsmasq_ok` = `domain::wildcard_active()` (sistema).
   - `mkcert_ok` = `mkcert -CAROOT` + `rootCA.pem` existe.
   - `cli_wrapper_ok` = `~/.local/bin/wp` existe.
   - `plasmoid_ok` = `~/.local/share/plasma/plasmoids/{id}` existe.
   - `endpoint` + `projects_root` + `config_dir`.
2. UI: checklist con dot verde/rojo. Acciones inline:
   - «Crear red» si `!network_ok` → `createPanelNetwork()`.
   - «Instalar/Reinstalar» wrappers → `installCliWrapper()`.
   - «Reasignar puerto» si URLs no limpias → `resetEndpoint()`.
3. Acciones que requieren privilegios (dnsmasq, CA mkcert, plasmoid)
   se delegan a `bash scripts/first-run.sh` (el panel no las automatiza).

### Mantenimiento: regenerar php.ini

1. UI `/settings` → «Aplicar a todos» → `api.repairAllPhpIni()` →
   `repair_all_php_ini` (`lib.rs::406`):
   - Recorre `load_all_sites()`, llama `wordpress::write_php_ini(site)`
     en cada uno (regenera desde el template actual).
   - Devuelve `"{ok}/{total} proyectos. Reinicia los que estén
     encendidos."` (los proyectos encendidos necesitan reinicio para
     releer el `php.ini`).

### Reparar auto-login

1. UI: tab `Plugins / Themes` en un proyecto → «Reparar auto-login» →
   `api.repairAutologin(id)` → `repair_autologin` (`lib.rs::394`):
   - `site.one_click_admin = true`.
   - `wordpress::sync_mu_plugins(site)` (reinyecta mu-plugins del panel
     en `wp-content/mu-plugins/`).
   - No requiere el proyecto encendido.

### Regenerar SSL

1. UI: menú «···» → «Regenerar SSL» (solo si `site.config.services.nginx.ssl`):
   `api.regenerateSsl(id)` → `regenerate_ssl` (`lib.rs::518`):
   - `ssl::generate(site)` corre `mkcert` para regenerar `cert.pem` /
     `key.pem` en `~/panel-wp/{slug}/ssl/`.
   - `docker::reload_nginx()` aplica el nuevo cert sin reiniciar el
     container php.

### Abrir Mailpit / MinIO / Adminer

- `api.openMailpit()` → `open_mailpit` (`lib.rs::617`): abre la UI de
  Mailpit (`127.0.0.1:8025`). Falla si `panel-mailpit` no está
  corriendo.
- `api.openMinio()` → `open_minio` (`lib.rs::629`): abre la consola
  MinIO (`127.0.0.1:9101`). Falla si `panel-minio` no está corriendo.
- `api.openAdminer(id)` → `open_adminer` (`lib.rs::643`): arranca
  `panel-adminer` (si no está), abre la URL con parámetros `server`,
  `db`, `username`. El mu-plugin `autologin.php` (`docker/adminer/`)
  inyecta el `auth` con la contraseña fija `panel` para auto-login en
  cero clics.

## Variantes y casos borde

- **Watcher de auto-dump saltado**: si el motor DB no está corriendo,
  `watch` continúa sin error; cuando vuelva a estar activo, retoma con
  el último hash conocido.
- **`Innodb_rows_*` no disponibles** (p. ej. MariaDB con esquema
  distinto): `write_counter` devuelve `None` y se hace dump en cada
  ciclo, confiando en el hash para no escribir de más.
- **Postgres**: no hay gate de `Innodb_rows_*`; se confía en el hash.
- **Hash estable entre dumps**: `mysqldump --skip-dump-date` quita la
  línea `Dump completed on`; sin esto, dos dumps seguidos tendrían
  hashes distintos aunque el contenido sea idéntico y el auto-dump
  escribiría siempre.
- **`dump_bytes` con DB vacía**: error «mysqldump no produjo salida
  para {dbname}». Si la DB se ha vaciado, el watcher no persiste (el
  dump falla).
- **Container zombie** (apagón sucio deja container sin red): el watcher
  y `docker::is_running` lo detectan; teardown normal lo limpia.
  `panel-nginx` zombie tras apagón: si el container queda pero el socket
  no responde, `docker::ensure_nginx` lo recrea al próximo
  `start_site` (comportamiento documentado pero fuera del flujo normal).
- **Logs con buffer grande**: el frontend acumula 500 líneas; el `tail:
  "200"` del stream de bollard envía las 200 más recientes al
  engancharse.
- **OpConsole con varias operaciones simultáneas**: el listener es
  global; si dos operaciones emiten a la vez, las líneas se intercalan
  en la misma consola. La práctica actual es abrir una `OpConsole` por
  acción desde el componente que la dispara.
- **Borrado con ventana de gracia** (`DeleteProjectModal.svelte`):
  durante 5 s, un botón «Cancelar borrado» aborta. Tras el
  countdown, `api.deleteSite(id, deleteFolder)` ejecuta. Las líneas de
  la cuenta atrás se emiten por `emit('op-log', ...)` desde el
  frontend con prefijo `` (línea viva); tras el countdown, el
  backend emite los pasos reales (`Apagando el proyecto...`,
  `Borrando la base de datos...`, etc.).
- **`endpoint` cargado pero `endpoint.is_default() == false`**: la UI
  `/settings` muestra la etiqueta «puerto alterno» en ámbar.
- **Reasignar puerto**: `api.resetEndpoint()` borra `endpoint` de
  `panel.json`. El siguiente arranque de `panel-nginx` reasigna puertos
  libres. Los sitios ya instalados guardan `siteurl` con el puerto
  viejo, por eso hay que migrarlos (`migrate_site`) o ajustarlos
  manualmente.

## Datos persistidos

- **`/var/log/php-fpm/*`** (dentro del container) → el stream los emite
  al frontend. No se persisten en disco del host por defecto.
- **`app/sql/db-{ts}.sql`** del proyecto: solo los 3 más recientes
  (rotación por `backup::rotate_dumps(site, 3)`).
- **`config_dir/dump-log.jsonl`** (`dumplog.rs`): log JSONL con
  `{timestamp, siteId, siteName, dbName, file, bytes, source}`. Sin
  rotación; se limpia por UI con filtros.
- **`config_dir/panel.json`** (`PanelConfig`): estado global (Endpoint
  elegido).
- **`config_dir/db-data/{container}/`**: datadir durable de cada motor
  DB (bind). Sobrevive al recreado del container y al apagón.

## Containers y Docker

- **`panel-nginx`**: comparte; se apaga si no queda ningún proyecto
  activo (`teardown_unused_shared`). Si se reinicia, los vhosts
  persisten en `config_dir/nginx/conf.d/`.
- **Motores DB**: comparten; se apaga cada versión si no queda ningún
  proyecto que la use.
- **`panel-mailpit`**: arranca con el primer proyecto activo; se apaga
  al apagar todos.
- **`panel-minio`**: solo se enciende si el proyecto tiene
  `minio = true` y está activo. Se apaga al apagar todos los proyectos
  que lo usen.
- **`panel-adminer`**: on-demand por `open_adminer`; se apaga al
  apagar todos los proyectos.
- **Watchers de auto-dump**: estado Tauri `AutoDump` (managed); vive
  mientras la app esté abierta. Al cerrar el panel, los watchers
  terminan (los `JoinHandle` se abortan).
- **`docker` CLI**: usado por `migrate::import_dump` (stdin adjunto) y
  por `php::ensure_php_image` (build). El resto va por bollard.

## Fallos y compensaciones

- **`docker::is_running` cuelga** (engine zombie): el timeout de bollard
  es alto; el watcher queda bloqueado hasta que Docker responde. Para
  recuperar: reiniciar el daemon Docker.
- **`OpConsole` sin capability `core:event`**: la consola sale vacía.
  Revisar `src-tauri/capabilities/default.json` (`core:event:default`).
- **`OpConsole` abierta antes del `invoke`**: el listener se engancha en
  `onMount` (no al abrir), así las primeras líneas de la operación no
  se pierden (`OpConsole.svelte:41-56`).
- **`dump_bytes` falla en `mysql`**: error propagado al watcher; el
  ciclo continúa (`continue`). El siguiente ciclo lo reintenta.
- **`dumplog::append` falla**: best-effort, no rompe el volcado. El
  `.sql` queda en disco aunque la entrada del log no se haya escrito.
- **`clean_dump_log` con filtro que no encaja**: `removed = 0`; UI lo
  reporta como `0 entradas borradas del log.`.
- **Reasignar puerto con proyectos encendidos**: `panel-nginx` puede
  no responder al reload; los sitios pueden quedar inaccesibles por
  HTTPS hasta que el panel asuma el nuevo endpoint. La acción pide
  confirmar.
- **`regenerate_ssl` con `cert.pem` bloqueado por el container**: el
  cert nuevo se escribe pero nginx no recarga; `docker::reload_nginx`
  debería refrescarlo (si el container responde).
- **`repair_autologin` con `mu-plugins/` escribible pero `wp-config.php`
  readonly**: `sync_mu_plugins` (`wordpress.rs:389`) escribe los
  mu-plugins; no toca `wp-config`. Si el proyecto se creó con
  `oneClickAdmin=false` pero luego se activa, `open_admin` funciona
  igual (el mu-plugin de auto-login se inyectó).

## Superficies

### UI (SvelteKit, SPA)

- **`/site/[id]`** → tabs `Logs`, `Plugins / Themes` (con «Reparar
  auto-login»), `Servicios` (Mailpit, MinIO, terminal, exportar DB,
  Adminer), `Puntos de guardado` (con OpConsole).
- **`/dumps`** (`src/routes/dumps/+page.svelte`): tabla del log de
  volcados con filtros de limpieza.
- **`/settings`** (`src/routes/settings/+page.svelte`): checklist de
  prerequisitos + endpoint + rutas + «Aplicar a todos» (regenerar
  `php.ini`).

### IPC (Tauri commands en `lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `stream_logs` | `id` | `logs::spawn_stream`; emite `log:{id}` |
| `stop_logs` | `id` | `JoinHandle::abort()` |
| `system_status` | — | `system::status` |
| `create_panel_network` | — | `docker::ensure_network` |
| `reset_endpoint` | — | `config::clear_endpoint` |
| `regenerate_ssl` | `id` | `ssl::generate` + reload nginx |
| `repair_autologin` | `id` | `wordpress::sync_mu_plugins` |
| `repair_all_php_ini` | — | Regenera `php.ini` en todos |
| `set_site_minio` | `id, enabled` | Activa/desactiva MinIO |
| `export_db` | `id` | Dump manual a `app/sql/` |
| `dump_log` | — | `dumplog::read_all` |
| `clean_dump_log` | `before?`, `dbName?` | `dumplog::clean` |
| `open_mailpit` | — | Abre UI de Mailpit |
| `open_minio` | — | Abre consola MinIO |
| `open_adminer` | `id` | Arranca `panel-adminer` + URL con auto-login |
| `install_cli_wrapper` | — | Copia `wp`/`wordpress-panel-cli` a `~/.local/bin` |

`api.ts` (`src/lib/api.ts`) expone los espejos.

### CLI (`scripts/wordpress-panel-cli.sh`)

`logs [servicio] [-f] [-n N]`: `docker logs --tail $TAIL $FOLLOW
$CONTAINER`. `servicio ∈ php|db|nginx|mailpit|minio` o nombre literal.
No requiere el panel abierto para logs de `php` (es solo
`docker logs`). Para `db/nginx/mailpit/minio` consulta
`ProjectContainers` por D-Bus.

### MCP (`mcp/server.mjs`)

`project_logs(project, service?, lines?)` (default php, 200 líneas).

### D-Bus (`src-tauri/src/dbus.rs`)

- `GetRunningSites`, `ListSites`, `StartSite`, `StopSite`, `StopAll`:
  para gestionar el estado.
- `ProjectContainers(id)`: lista php + compartidos (db/nginx/mailpit/
  minio opcional).

## Tests

- `dumplog::tests::clean_por_fecha_borra_anteriores`,
  `clean_por_db_borra_solo_esa`, `clean_combinado_es_interseccion`,
  `clean_sin_filtros_borra_todo`: cobertura completa de la lógica de
  filtros del log.
- `backup::tests::rotate_conserva_los_n_mas_recientes_e_ignora_ruido`,
  `rotate_no_borra_si_hay_menos_o_igual_que_keep`: rotación de dumps
  no toca `imported.sql` ni `local.sql`.
- `config::tests::site_url_cuatro_ramas`: `Endpoint` con puertos
  estándar vs alternos produce URLs correctas.
- `autodump.rs` no tiene tests puros (depende de Docker y del tiempo);
  la lógica hash y gate se valida manualmente.

## Límites conocidos

- **Postgres sin gate barato**: el hash del dump es el único gate; el
  dump puede tardar más en sitios grandes.
- **No hay cuotas de disco**: si el `dump-log.jsonl` crece mucho, hay
  que limpiarlo manualmente con los filtros de `/dumps`.
- **`dump_bytes` ejecuta `mysqldump` dentro del container DB** (socket
  local, sin TLS): el cliente del container php falla con MySQL 8 por
  verificación de cert, por eso el dump va por el container DB.
- **`auto-dump` solo mantiene 3 dumps `db-{ts}.sql`**: los más viejos
  se borran automáticamente. Los `imported.sql` / `local.sql` /
  `from-parent-{ts}.sql` no se tocan.
- **Logs de php-fpm**: stream desde bollard; si el container cae, el
  stream termina con `Err(_)` (`logs.rs::42`) y el frontend deja de
  recibir líneas. No hay reconexión automática.
- **OpConsole global**: dos operaciones simultáneas se mezclan. La
  práctica actual es una consola por acción.
- **`resetEndpoint` no migra sitios**: los `siteurl` viejos siguen con
  el puerto anterior; los sitios no funcionan hasta migrar.
- **`regenerate_ssl` solo regenera el cert del proyecto**: la CA de
  mkcert debe estar instalada (lo hace `first-run.sh`).
- **`repair_all_php_ini` requiere reiniciar proyectos**: el `php.ini`
  se monta ro; el container php tiene que reiniciar para releerlo.
- **Mailpit / MinIO UI en localhost**: solo accesibles desde la máquina
  del panel. Para acceso remoto habría que túneles (Cloudflare Tunnel
  → `feature_stub("cloudflare")`).
- **Adminer autologin**: la contraseña fija `panel` es pública
  (cualquiera con acceso al puerto 8088 puede entrar). Solo local.

## Invariantes y recomendación rebuild

- **`dump-log.jsonl` separado de los `.sql`**: limpiar el log no borra
  dumps; rotar dumps no borra el log. Son dos sistemas paralelos.
- **`endpoint` se elige una vez y persiste**: cambiarlo después no es
  retroactivo. Los sitios nuevos heredan el nuevo; los viejos guardan el
  puerto viejo en su `siteurl`.
- **Auto-dump no exporta worktrees ni clones**: solo proyectos normales.
  Los clones viven independientemente y heredan el ciclo del motor DB si
  están encendidos.
- **`core:event:default` es necesaria para CUALQUIER evento backend→frontend**
  (`log:{id}`, `op-log`, `sites-changed`). Si se añade un evento nuevo
  sin la capability, la UI no lo recibe.
- **`OpConsole` se suscribe en `onMount`**: si la consola se monta
  después del `invoke`, las primeras líneas se pierden. El patrón es
  montar la consola antes de disparar la acción.
- **Borrado con ventana de gracia**: la cuenta atrás es por frontend;
  el backend no sabe que está pendiente. Si el usuario cierra la
  ventana durante la cuenta atrás, el borrado se cancela (no se ha
  llamado al backend aún).
- **Rebuild desde cero**: perder `dump-log.jsonl` no afecta a la
  operatividad, solo a la trazabilidad. Perder `db-data/{container}/`
  borra TODAS las DB de proyectos que usaban ese motor — los `db.sql`
  en `app/sql/` (auto-dump + stop-dump) son la única foto.
- **Watchers de auto-dump no sobreviven al cierre del panel**: al
  reabrir, `lib.rs::988` los vuelve a enganchar para los containers que
  ya estén corriendo. Los cambios entre el cierre y la reapertura no se
  vuelcan; el watcher reanuda con el último hash conocido y solo
  detecta nuevos cambios.

## Fuentes

- `src-tauri/src/logs.rs`
- `src-tauri/src/progress.rs`
- `src-tauri/src/autodump.rs`
- `src-tauri/src/backup.rs`
- `src-tauri/src/dumplog.rs`
- `src-tauri/src/system.rs`
- `src-tauri/src/wordpress.rs::sync_mu_plugins`
- `src-tauri/src/ssl.rs`
- `src-tauri/src/cli.rs` (instalar wrappers)
- `src-tauri/src/docker.rs` (ciclo de vida, teardown, ensure_adminer)
- `src-tauri/src/lib.rs` (todos los comandos listados arriba +
  `start_site`/`stop_site` que enganchan el auto-dump)
- `src/lib/components/OpConsole.svelte`
- `src/lib/components/ProjectDetail.svelte` (tabs Logs, ext, svc;
  `startLogs`/`stopLogs`)
- `src/routes/dumps/+page.svelte`
- `src/routes/settings/+page.svelte`
- `src/lib/api.ts`
- `src-tauri/capabilities/default.json` (capability `core:event:default`)
- `scripts/wordpress-panel-cli.sh`, `mcp/server.mjs`,
  `src-tauri/src/dbus.rs`
- `docs/ARCHITECTURE.md` (secciones sobre logs, auto-dump, dumplog,
  eventos, capability)
