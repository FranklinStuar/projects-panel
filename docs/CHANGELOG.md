# Changelog

Registro de lo construido, por fase. **Añade una entrada al cerrar cada cambio
relevante.** Para el plan completo ver `PLAN.md`.

## Fase 1 — MVP Core (en curso)

Cimientos de la optimización de recursos.

### Hecho
- **Scaffold frontend**: SvelteKit + Svelte 5 + `adapter-static` modo SPA
  (`ssr=false`, fallback `index.html`). Tailwind dark/light. Dashboard con
  start/stop y estado real; stubs de dominios/settings/site.
- **Backend Tauri 2 + orquestación Docker** (`docker.rs`):
  - red `panel-net`; servicios compartidos on-demand (DB por versión, nginx).
  - container php por proyecto sin puertos host.
  - teardown de compartidos cuando ningún activo los usa (0 recursos si parado).
  - mapeo UID/GID host↔www-data (entrypoint), exec.
- **`nginx.rs`**: vhosts generados (root ro en `/srv/projects`, FastCGI a
  `wp-{id}:9000` con `SCRIPT_FILENAME` en vista del container php) + reload.
- **`php.rs`**: build de imagen `panel-php:{ver}` desde Dockerfile + WP-CLI phar.
- **`domain.rs`**: dnsmasq wildcard `*.test` (best-effort + detección).
- **`wordpress.rs`**: `create_site` end-to-end (tarball, DB, wp-config, core
  install vía WP-CLI, mu-plugin mailpit, locale); `fetch_versions` (cache 24h).
- **`wpcli.rs`**: WP-CLI dentro del container.
- **Assets `docker/`**: Dockerfile php-fpm + entrypoint UID/GID, php.ini.tmpl,
  nginx/vhost.conf.tmpl, mu-plugins/panel-mailpit.php.
- **Formulario Nuevo Proyecto** cableado a `create_site` con versiones WP en vivo.
- **Decoración de ventana nativa**: `GTK_CSD=0` en Linux para que los botones
  respeten la config del usuario en KDE (izq/der), portable entre máquinas.
- **`scripts/first-run.sh`**: primera config idempotente (panel-net, dnsmasq
  wildcard vía NetworkManager, mkcert).
- **Verificado live**: imagen php 107MB; entrypoint remapea www-data→uid host;
  extensiones gd/intl/mysqli/pdo_mysql/zip/opcache; `*.test`→127.0.0.1 con DNS
  externo intacto; GUI arranca sin panics.

### Pendiente de Fase 1
- WP-CLI wrapper instalado en `~/.local/bin/` + binario `wordpress-panel-cli`.
- Verificación end-to-end de una provisión completa de WordPress real.

## Fase 2 — Funcionalidades completas (COMPLETA)

### Hecho
- **Logs en vivo** (`logs.rs`): stream `follow` de los logs del container →
  eventos `log:{id}`; tab "Logs" en la vista de proyecto con autoscroll del
  buffer (últimas 500 líneas). Start/stop atado al tab + estado `LogStreams`.
- **Auto-login one-click** (`autologin.rs` + mu-plugin `panel-autologin.php`):
  token efímero de un solo uso (transient WP 60s) → botón "Abrir admin" abre el
  navegador logueado. Inyectado en `create_site` si `oneClickAdmin`.
- **Listado de plugins/themes** (`list_plugins`/`list_themes` vía WP-CLI JSON);
  tab "Plugins / Themes" en la vista de proyecto.
- **GitHub vía `gh`** (`github.rs`): clonar/pull/quitar theme y plugins en el
  host (bind-mount → cambios al instante), estado de `gh`,
  registro en config.json; tab "GitHub" con UI de theme/plugins y "Pull todo".
- **SSL con mkcert** (`ssl.rs`): genera cert/key por dominio en creación si
  SSL activo; comando `regenerate_ssl`; CA local instalada (sistema + NSS
  navegadores) vía `first-run.sh`. Botón "Regenerar SSL" en Info.
- **Grupos de proyectos**: `set_site_group` + editor de grupo en Info; el
  dashboard ya agrupa por `config.group`.
- **D-Bus + plasmoid KDE** (`dbus.rs` + `plasma/applets/wordpress-panel-plasmoid/`):
  servicio `com.goldmediatech.WordpressPanel` (GetRunningSites/StopSite/StopAll/
  Quit), arranca en el setup de Tauri; plasmoid Plasma 6 que consulta vía qdbus6
  y muestra proyectos activos con detener + "Apagar todo y cerrar". Verificado en
  vivo con busctl/qdbus6; plasmoid instalado con kpackagetool6.
- **Vista de proyecto con tabs** (Info / Logs / Plugins-Themes / GitHub) + start/stop.

Fase 2 completa. Pendiente de verificación: comportamiento visual del plasmoid en
una sesión Plasma (no testeable headless) y la barra de título (KNOWN_ISSUES).

## Fase 3 — Servicios adicionales (COMPLETA)

### Hecho
- **Servicios compartidos on-demand** (`docker.rs`): `ensure_mailpit` (axllent/
  mailpit, UI `127.0.0.1:8025`, SMTP `:1025` interno) y `ensure_minio` (minio/
  minio, API `127.0.0.1:9100`, consola `127.0.0.1:9101`, datos en
  `~/.config/wordpress-panel/minio-data`). Mailpit arranca con cualquier proyecto
  activo; MinIO solo si el proyecto tiene el flag `minio`. `teardown_unused_shared`
  los apaga cuando ningún activo los usa (mailpit/nginx si no queda ninguno; minio
  si nadie lo pide). Helper `host_port_map` (bind solo a loopback).
- **MariaDB y PostgreSQL**: ya soportados a nivel de infra (`DbType` → imagen,
  `db_env`, `create_database` con rama SQL por motor). Selector en el form.
- **Backup** (`backup.rs`): `export_db` vía `wp db export` a la raíz pública
  (montada) y movido a `app/sql/db-{timestamp}.sql`. Comando `export_db`.
- **Wrapper WP-CLI** (`cli.rs` + `scripts/wp-wrapper.sh` +
  `scripts/wordpress-panel-cli.sh`): `install_cli_wrapper` copia `wp` y
  `wordpress-panel-cli` a `~/.local/bin` (chmod 755) y avisa si no está en PATH.
  `wp` detecta el proyecto por el CWD y ejecuta WP-CLI en su container.
- **Headless**: flags `headless` + `frontendFramework` en `NewSiteRequest`/
  `SiteConfig`; checkbox + selector de framework en el form; visible en Info.
- **Botones stub** (`feature_stub`): Cloudflare Tunnel / Deploy / Empaquetado —
  UI preparada, devuelven mensaje "no implementado, fase posterior".
- **Frontend**: tab **Servicios** en la vista de proyecto (backup, abrir Mailpit,
  toggle + abrir MinIO, instalar wrapper, stubs) y ruta **`/services`**
  (servicios compartidos del panel). Flag `minio` en `SiteConfig`/types.
- **Comandos IPC nuevos**: `set_site_minio`, `export_db`, `install_cli_wrapper`,
  `open_mailpit`, `open_minio`, `feature_stub`.

Pendiente real de Fase 3 (diferido): provisión de un container de frontend para
proyectos headless (hoy solo se guardan los flags) y plugin S3 que conecte WP a
MinIO (el servicio se ofrece, la integración WP queda al usuario).

## Fix — Conflicto de puerto 80/443 del host (coexistencia con LocalWP)

Al crear el primer proyecto, Docker fallaba con un 500 opaco
(`failed to bind host port 127.0.0.1:80/tcp: address already in use`) porque
LocalWP escucha en `0.0.0.0:80` (wildcard). Verificado en vivo: el kernel
rechaza bindear *cualquier* `127.0.0.x:80` mientras exista un listener wildcard
en ese puerto (la treta de otra IP loopback solo sirve si el ocupante está atado
a una IP concreta, no a `0.0.0.0`).

- **`netcheck.rs`** (nuevo): lee `/proc/net/tcp{,6}` y clasifica cada puerto en
  `Free` / `Wildcard` / `Specific(IPs)`. Decodificación IPv4 little-endian
  (`0100007F`→127.0.0.1), detección de wildcard IPv6 `::`. Selectores
  `pick_loopback_ip` (127.0.0.x libre) y `pick_alt_port`; `holder_name`
  (best-effort, nombre del proceso que ocupa el puerto vía inodo→/proc). Tests.
- **`config.rs`**: `Endpoint { loopbackIp, httpPort, httpsPort }` (global del
  panel, persistido en `~/.config/wordpress-panel/panel.json`). Se elige UNA vez
  y se mantiene estable (WP guarda `siteurl` con puerto). Helper `site_url`
  (puerto solo si no es estándar).
- **`docker.rs` `ensure_nginx`**: autoselección por capas →
  1) `127.0.0.1:80/443` si libre; 2) conflicto por IP concreta → otra IP loopback
  en 80/443 (URLs limpias, dnsmasq repuntado vía pkexec); 3) conflicto wildcard
  (LocalWP) → `127.0.0.1` + puerto alterno (`sitio.test:8080`). **Preflight**
  con error legible (nombrando al proceso) en vez del 500. Recrea el `panel-nginx`
  parado para no arrastrar bindings viejos.
- **`domain.rs`**: regla dnsmasq parametrizada por IP; `resolves_to` +
  `install_wildcard` (pkexec, recarga NetworkManager) para la IP alterna.
- **URLs**: `wordpress.rs` (core install) y `autologin.rs` usan el endpoint.
  Comando `panel_endpoint` + tipo TS `Endpoint`/`siteUrl`; el dashboard y la
  vista de proyecto muestran el puerto cuando es alterno.

## Fix — Instalación de WordPress fallaba en silencio (sitio a medias)

Al investigar por qué el admin abría el instalador de WP, salieron tres bugs
encadenados que dejaban el proyecto creado pero sin WordPress instalado, y aun
así `create_site` devolvía Ok:

- **`docker.rs` `exec` se tragaba el exit code**: capturaba stdout/stderr pero
  nunca miraba el código de salida → cualquier fallo (DB, WP-CLI) pasaba como
  éxito. Ahora `inspect_exec` y `Err` con el output si el código ≠ 0.
- **WP-CLI corría como root**: `exec` no fijaba usuario (uid 0) y WP-CLI rechaza
  root (`YIKES`). Nuevo `exec_as(user)`; `wpcli::run` y `backup` corren como
  `www-data` (además los archivos quedan con el dueño del host vía remapeo uid).
- **`ensure_db` no esperaba readiness**: en el primer arranque MySQL acepta el
  socket local antes de abrir TCP; `create_database`/`wp config create` corrían
  en esa ventana y fallaban. Nuevo `wait_db_ready` que gatea sobre TCP
  (`mysql -h127.0.0.1 …` / `pg_isready -h 127.0.0.1`), timeout 60s.

## Diseño — Tema "DevFlow Dark Blue"

- Adoptada la paleta navy de `DESIGN.md` (electric blue + deep navy) en vez del
  zinc-950 anterior. Implementada **remapeando la escala `zinc` de Tailwind** a
  los tonos navy (`tailwind.config.js`): los componentes existentes con
  `dark:bg-zinc-*` heredan el tema sin tocarse. Token `primary` (#4d8eff) añadido.
- **Fix inputs blancos**: `.input` definía `bg-white` y vivía en el `<style>` de
  `site/new` (no global, sin color de texto). Movido a estilo global en
  `app.css` (`@layer base` para todo input/select/textarea + `@layer components`
  para `.input`): fondo navy `zinc-900`, texto `zinc-100`, foco azul, placeholder
  atenuado. `accent-color` azul para checkbox/radio. App ahora dark-only navy.

## Fase 4 — Polish

### Settings completo (estado del sistema + primera configuración)

`src/routes/settings/+page.svelte` pasó de stub a pantalla real.

- **`system.rs`** (nuevo): `system_status()` reúne en una lectura el estado de
  los prerequisitos — Docker accesible, red `panel-net`, dnsmasq `*.test`, CA de
  mkcert (`mkcert -CAROOT`/rootCA.pem), wrappers WP-CLI (`~/.local/bin/wp`),
  plasmoid (`~/.local/share/plasma/plasmoids/{id}`) — más el endpoint y las
  rutas (projectsRoot, configDir). Best-effort: un chequeo que falla es `false`.
- **Comandos**: `system_status`, `create_panel_network` (crea `panel-net` si
  falta, reusa `ensure_network`), `reset_endpoint` (`config::clear_endpoint` →
  reasigna puerto en el próximo arranque). `docker.rs` gana `network_exists`.
- **UI**: checklist con semáforo por ítem y botones para lo que no necesita
  privilegios (crear red, instalar wrappers); las acciones con sudo (dnsmasq,
  mkcert CA, plasmoid) remiten a `bash scripts/first-run.sh`. Muestra el endpoint
  (badge URLs limpias / puerto alterno) con botón "Reasignar puerto", y las rutas.

### Migración entre sistemas + export automático al detener

El modelo ya tenía `migrationPending`/`lastMigratedAt` y el dashboard pintaba el
estado, pero no había forma de migrar ni de exportar la DB al apagar.

- **`migrate.rs`** (nuevo): `migrate_site()` provisiona un proyecto pendiente en
  el sistema actual → crea la base de datos (idempotente), enciende php+vhost,
  **regenera `wp-config.php`** con las credenciales del panel (el origen pudo
  usar otro host/disco/LocalWP), importa el último dump de `app/sql/` y regenera
  el certificado SSL. Devuelve la config + un aviso opcional (p. ej. "sin dump").
  El dump se importa copiándolo a la raíz pública (montada) porque `app/sql/` no
  está bind-montado en el container. Reusa `create_database`/`wp_config_create`
  de `wordpress.rs` (ahora `pub(crate)`).
- **Comando** `migrate_site(id) -> Migration`. Botón **Migrar y encender**
  (ámbar) en el dashboard y en la vista de proyecto, reemplazando el texto
  estático "Pendiente de migración".
- **Export-al-detener**: `stop_site` (docker.rs) ahora exporta la DB y rota
  dumps **antes** de apagar el container (best-effort, no bloquea el stop).
  `backup::rotate_dumps(site, 3)` deja solo los 3 `db-*.sql` más recientes (no
  toca otros `.sql` como `imported.sql`).

### Importación desde LocalWP

- **`localwp.rs`** (nuevo): `list_sites()` parsea `~/.config/Local/sites.json`
  (deserialización tolerante) y lista los sitios (nombre, dominio `.test`, PHP,
  MySQL, multisite, xdebug, ya-importado). `import_site()` crea el proyecto del
  panel: copia `app/public` con `cp -a`, copia el dump `app/sql/local.sql` como
  `imported.sql`, mapea versiones a las soportadas (avisa si ajusta) y escribe un
  `config.json` con `migrationPending=true` y grupo "LocalWP". La DB se
  materializa luego con "Migrar y encender". Reusa `slugify`/`create_dirs`/
  `write_php_ini` de `wordpress.rs` (ahora `pub(crate)`).
- **Comandos**: `list_localwp_sites`, `import_localwp_site`. Sección "Importar
  desde LocalWP" en `/settings`.
- **`migrate.rs`**: tras importar el dump, fija `home`/`siteurl` al dominio del
  panel (`fix_site_url`) para que el admin funcione aunque el dump venga de
  `*.local`. Limitación (sin `search-replace`) documentada en `KNOWN_ISSUES.md`.

### Empaquetado del plasmoid

- **`scripts/package-plasmoid.sh`** (nuevo): genera `dist/wordpress-panel.plasmoid`
  (zip de `metadata.json` + `contents/`) instalable con
  `kpackagetool6 --install`. Idempotente. `first-run.sh` lo menciona; `dist/`
  ignorado en git.

### Fix — import/export de DB: hacerlo en el container DB (sin TLS)

`wp db import/export` desde el container php fallaba en cadena: primero
`env: can't execute 'mysql'` (sin cliente en la imagen), luego —con el cliente—
`TLS/SSL error: self-signed certificate` (MySQL 8 ofrece TLS con cert
autofirmado y el cliente mariadb lo verifica). Solución: **import/export se
ejecutan dentro del container DB** (`panel-mysql-*`) por socket local, sin TLS:
- `docker.rs`: `exec_stdin` (alimenta `mysql` por stdin → importar dump) y
  `exec_capture` (captura stdout de `mysqldump` → exportar). Helper
  `db_container_name`.
- `migrate.rs` importa el último dump con `exec_stdin`; `backup.rs` exporta con
  `mysqldump --single-transaction` vía `exec_capture` (antes movía un archivo
  desde la raíz pública).
- La imagen php suma `mariadb-client` (para `wp db` desde el wrapper de terminal)
  y el tag lleva revisión (`panel-php:{ver}-{rev}`, `php::IMAGE_REV`): al subirla
  `ensure_php_image` reconstruye y `start_site` **recrea** los containers con tag
  viejo (compara `container_image`).

### Fix — migración: generar SSL antes de encender

El vhost referencia `ssl/cert.pem`; `migrate_site` encendía el sitio (escribe
vhost + `nginx -s reload`) antes de generar el cert → reload fallaba. Reordenado:
`ssl::generate` antes de `start_site`, igual que `create_site`.

### Consola de progreso + cancelar importación

- **Consola en vivo**: operaciones largas (migración, import LocalWP) parecían
  colgadas (reconstrucción de imagen php, copia de archivos, dump de 100&nbsp;MB).
  `progress.rs` (nuevo) emite líneas de paso en el evento `op-log`; el componente
  `OpConsole.svelte` las muestra en un modal en vivo (autoscroll, "Cerrar"
  deshabilitado mientras corre). `migrate_site`/`import_localwp_site` reciben
  `AppHandle` y reportan cada paso.
- **Cancelar importación**: botón "Cancelar" junto a "Migrar y encender" en
  proyectos `migrationPending` (dashboard y vista de proyecto) → comando
  `delete_site` (apaga + quita container/vhost + borra la carpeta). Para deshacer
  una importación del proyecto equivocado.

### Fix — migración se colgaba importando dumps grandes

Migrar un sitio real (p. ej. desde LocalWP) se quedaba clavado tras "Generando
certificado SSL" y no avanzaba nunca (minutos hasta matar la app). Causa: la
importación del dump usaba `DockerManager::exec_stdin` (bollard, `exec` con stdin
adjunto). Con stdin adjunto el **stream de salida de bollard no emite `None`** al
terminar el proceso, así que tras volcar el dump el lector quedaba esperando para
siempre. Dumps chicos (~1&nbsp;MB) colaban de chiripa; uno de 7&nbsp;MB colgaba.

- **`migrate.rs`**: `import_dump` ahora usa el CLI **`docker exec -i … mysql`**
  (excepción al "Docker solo por bollard", como ya lo era `docker build` de la
  imagen php). `wait_with_output` drena stdout/stderr mientras una tarea escribe
  el dump por stdin → sin deadlock de pipe. Importa 7&nbsp;MB en ~15&nbsp;s.
- **`docker.rs`**: eliminado `exec_stdin` (quedaba sin uso y era justo el que se
  colgaba); nota en su lugar apuntando a `migrate::import_dump`.
- **`wpcli.rs`**: timeout defensivo de 120&nbsp;s en todo WP-CLI. WP-CLI arranca
  WordPress entero; un plugin/mu-plugin del sitio que haga una llamada de red al
  cargar (licencia/update-check; p. ej. UpdraftPlus) colgaría el comando —y la
  migración— indefinidamente. `migrate::fix_site_url` además corre con
  `--skip-plugins --skip-themes` (no carga plugins normales para repuntar URLs).
- **`OpConsole.svelte`**: el listener de `op-log` se engancha en `onMount`, no al
  abrir. `listen()` es async y competía con el `invoke` de la operación en el
  mismo tick → se perdían las primeras líneas de progreso (la consola salía
  vacía). Ahora limpia el buffer al abrir y no pierde líneas.

### Fix — consola de progreso vacía: faltaba la capability de eventos

La migración funcionaba pero la consola (`OpConsole`) salía **vacía**: solo se veía
el ícono verde al terminar, sin ninguna línea de progreso ni el error si fallaba.
Causa: el proyecto **no tenía ninguna capability de Tauri 2**. Los comandos propios
(`#[tauri::command]`) no pasan por el ACL y por eso `migrate_site` funcionaba, pero
`listen('op-log')` usa el plugin **`core:event`**, que sí está gateado por permisos;
sin capability que lo conceda, el `listen` quedaba bloqueado y nunca llegaban los
eventos. Los tests e2e usan IPC mockeado (no Tauri real), así que no lo detectaban.

- **`src-tauri/capabilities/default.json`** (nuevo): concede `core:default` +
  `core:event:default` a la ventana `main`. Tauri 2 autodescubre `capabilities/*.json`,
  no hace falta tocar `tauri.conf.json`.
- **`migrate.rs`**: mensajes de progreso más descriptivos y numerados (`[n/6]`):
  arrancar DB + esquema, SSL, encender, regenerar wp-config, importar dump (con
  nombre y tamaño), ajustar URLs (con el destino `scheme://dominio`). El flujo real
  va en `run_migration`; `migrate_site` lo envuelve y, ante **cualquier** error,
  emite una línea `✗ La migración falló: …` a la consola antes de propagarlo, para
  que el fallo se vea ahí y no solo en el banner.

### Fix — import del dump: timeout con rollback, aceleración y barra de progreso

Un dump grande podía quedarse "clavado" en la importación (mysql lento, o el
`docker exec` colgado) sin forma de cancelar ni señal de avance, dejando además
la DB a medio importar (corrupta) si se mataba la app. Tres cambios en
`migrate::import_dump`:

- **Aceleración (causa real de la lentitud)**: se anteponen pragmas de sesión al
  stream (`SET foreign_key_checks=0; unique_checks=0; autocommit=0; … COMMIT;`).
  Evita el fsync y la revalidación de índices/FK por statement, que es lo que hace
  que decenas de MB tarden minutos.
- **Watchdog con rollback + resume**: el dump se escribe por chunks y un watchdog
  cancela el `docker exec` si **ni el stdin avanza ni crece la DB** durante 3 min
  (`IMPORT_IDLE_TIMEOUT`). Al cancelar, `wordpress::reset_database` (nuevo) hace
  `DROP DATABASE` + recrea vacía, para no dejar un dump aplicado a medias.
  Reintentar la migración **reanuda**: los pasos 1–4 son idempotentes y la DB
  queda limpia, así que el import vuelve a empezar de cero (única forma segura: no
  se puede retomar un dump SQL a mitad de statement).
- **Indicador de vida correcto**: medir solo bytes-por-stdin daba falsos timeouts
  —el pipe del OS es de ~64&nbsp;KB; tras el primer chunk `write_all` se bloquea
  hasta que mysql consume stdin, y mysql lo consume tan rápido como **aplica** el
  SQL, así que durante un statement grande no fluye ni un byte aunque el import
  avance—. El watchdog usa además el **tamaño real de la DB** (`information_schema`,
  vía `query_db_size`).
- **Barra de progreso en sitio**: `progress::log_progress` (nuevo) marca líneas
  "vivas" con un prefijo SOH (`PROGRESS_PREFIX = '\u0001'`); `OpConsole.svelte` las
  **reescribe en sitio** en vez de apilarlas, así un contador que tickea cada 2&nbsp;s
  no inunda la consola. El import muestra `12/53 MB ━━━━━──── 1:23` actualizándose.

**Fase 4 completa.** Falta solo Fase 5 (asistente IA, `agent.rs`).

## Testing — dos vías (ver `docs/TESTING.md`)

- **Sin panel (lógica, Rust)**: unit puros (`cargo test`, sin Docker) para
  `slugify`, mapeo de versiones LocalWP, `rotate_dumps`, `Endpoint::site_url` y
  roundtrip serde camelCase. Integración `#[ignore]` en `integration_tests.rs`
  (`cargo test -- --ignored --test-threads=1`): import LocalWP hermético (sin
  Docker, vía `HOME`/`XDG` temporales), ciclo de DB y e2e crear→exportar→migrar.
  `progress::log` y `migrate_site`/`import_site` ahora son genéricos sobre
  `Runtime` para usar `tauri::test::mock_app()`.
- **Con panel (GUI, Playwright)**: capa mock de IPC (`src/lib/dev/`) que sirve el
  SPA con fixtures (`pnpm dev:mock`, `VITE_MOCK_IPC=1`); specs en `e2e/`
  (dashboard, migrar, cancelar import, settings, nuevo, a11y) vía `pnpm test:e2e`.
  No necesita backend ni Docker.

## Visor de bases de datos (Adminer)

- **`panel-adminer`** (`adminer:4`), servicio compartido on-demand: visor web de
  DB para MySQL, MariaDB y Postgres (un solo tool para los tres motores). UI en
  `127.0.0.1:8088`, habla con los containers DB por `panel-net`. Se apaga con el
  resto de compartidos cuando no queda proyecto activo (`teardown_unused_shared`).
- **Plugin `docker/adminer/autologin.php`** (montado en `plugins-enabled/`):
  auto-login en **cero clics**. En peticiones GET inyecta `$_POST["auth"]` con la
  pass fija del entorno (`panel`); como los plugins se construyen antes del bloque
  de auth de Adminer y este reemplaza el token del POST de `auth` por el de sesión
  válido, `verify_token()` pasa y se entra sin formulario. Solo en GET (no pisa
  los POST reales del usuario, p. ej. ejecutar SQL). Sin restricción de vista
  (dev): un servidor/DB mal escrito falla de forma natural. Verificado: un solo
  GET sin POST aterriza en `Database: foo_db` con sesión iniciada.
- **Comando `open_adminer(id)`** (`lib.rs`): exige el proyecto corriendo (DB
  arriba), arranca Adminer y abre el navegador en
  `?{driver}={db_container}&username={root|panel}&db={dbName}` (driver `pgsql`
  para Postgres, `server` para MySQL/MariaDB). Botón «Ver base de datos (Adminer)»
  en la sección Base de datos de la página de proyecto.

## Fix — Auto-login en proyectos importados de LocalWP

- **Causa**: `create_site` inyectaba los mu-plugins del panel (mailpit +
  `panel-autologin.php`) en el paso 6, pero `localwp::import_site` y `migrate` no.
  Un proyecto importado llegaba sin `panel-autologin.php`, así que «Abrir admin»
  no auto-logueaba (el mu-plugin que valida el token no existía).
- **`wordpress::sync_mu_plugins(site)`**: helper idempotente que (re)inyecta
  mailpit (siempre) + auto-login (si `oneClickAdmin`). `create_site` ahora lo usa;
  `migrate` lo llama tras verificar la carpeta — cubre el import de LocalWP **y**
  copias entre sistemas (mu-plugins desfasados se refrescan).
- **Comando `repair_autologin(id)`** (`lib.rs`): para proyectos **ya importados**
  antes del fix. Activa `oneClickAdmin` y reinyecta los mu-plugins; idempotente y
  no requiere el proyecto encendido (los mu-plugins van montados desde disco).
  Botón «Reparar auto-login» en el tab *Plugins / Themes* de la página de proyecto
  (`api.repairAutologin`; mock en `src/lib/dev/mock-ipc.ts`).

## Terminal WP-CLI en un clic

- **Antes**: solo existía el botón «Instalar wrapper `wp`». El usuario tenía que
  abrir su propia terminal, hacer `cd` a la carpeta del proyecto y recordar
  ejecutar `wp`. No había forma de abrir la terminal desde el panel.
- **`cli::open_terminal_at(path)`**: lanza el primer emulador de terminal
  disponible (konsole, gnome-terminal, xfce4-terminal, kitty, alacritty,
  `x-terminal-emulator`) con cwd en la carpeta del proyecto, detached.
- **Comando `open_terminal(id)`** (`lib.rs`): instala el wrapper (idempotente) y
  abre la terminal ya situada en el proyecto; dentro `wp <args>` funciona porque
  el wrapper detecta el proyecto por el CWD.
- **Auto-instalación del wrapper**: el wrapper es **global del usuario**
  (`~/.local/bin`, no por-proyecto). `run()` lo instala una vez al arrancar
  (idempotente, best-effort), así proyectos nuevos no necesitan ninguna acción.
- **UI** (tab *Servicios* de la página de proyecto): botón «Abrir terminal del
  proyecto» (requiere proyecto encendido) + texto de ayuda con ejemplos
  (`wp plugin list`, `wp user list`) y aviso de no usar `sudo`. Se conserva «Solo
  instalar wrapper `wp`» para reinstalar/reparar. `api.openTerminal`.

## Fix — el wrapper `wp` de terminal corría como root

- **Bug**: `scripts/wp-wrapper.sh` hacía `docker exec` **sin** `--user www-data`,
  así WP-CLI arrancaba WordPress como root → bloqueo anti-root (`YIKES! …running
  this as root`). `wp cli info` funcionaba (no bootea WP) pero `wp plugin list` no.
  El comando in-app `exec_wpcli` sí usaba www-data; el wrapper se quedó sin paridad.
- **Fix**: añadido `--user www-data` al `docker exec` del wrapper. El refresco del
  script se aplica solo al reabrir el panel (auto-instalación idempotente).

## Borrar proyecto (con opción de conservar la carpeta)

Botón **"Eliminar"** en el dashboard (cada tarjeta) y en la vista de proyecto,
para cualquier proyecto (no solo importaciones pendientes — eso ya lo cubría
"Cancelar").

- **Siempre borra todos los datos**: apaga + quita container/vhost + **`DROP
  DATABASE`** del esquema del proyecto en el servidor de DB compartido
  (`wordpress::drop_database`, nuevo — antes `delete_site` dejaba el esquema
  vivo). Tras el drop, `teardown_unused_shared` re-apaga el container de DB si
  ningún otro activo lo usa.
- **Modal de confirmación propio** (`DeleteProjectModal.svelte`, no el `confirm()`
  nativo que mostraba la URL de localhost como título): titula con el **nombre**
  del proyecto y trae un **checkbox** "Borrar también la carpeta del proyecto en
  disco" — una sola pantalla en vez de dos diálogos encadenados.
  - **Marcado** → `remove_dir_all` de la carpeta del proyecto.
  - **Sin marcar** → conserva la carpeta y solo elimina su `config.json`, así el
    panel la olvida (queda "desconectada"); `app/public`, `conf` y los dumps de
    `app/sql` siguen en disco para reconfigurarla más tarde. `stop_site` deja un
    dump fresco antes de apagar.
- **Consola con ventana de gracia**: al confirmar se abre la `OpConsole` (la misma
  de migración/import) con una cuenta atrás de **5 s** ("Preparando proceso de
  eliminación…") y un botón **«Cancelar borrado»**. Si se cancela a tiempo, no se
  toca nada. Pasados los 5 s desaparece el botón de cancelar y se procede; al
  terminar se habilita **«Cerrar»**. `delete_site` emite sus pasos (apagar, DROP
  de la DB, borrar/desconectar carpeta) por el canal `op-log`.
- **API**: `delete_site` pasa de `(id)` a `(id, deleteFolder)` (+ `AppHandle` para
  emitir progreso); `api.deleteSite` espejo. "Cancelar importación" sigue llamando
  con `deleteFolder=true` (borra todo, como antes).
- **Tests**: `e2e/delete-site.spec.ts` cubre las cuatro ramas (modal + cancelar,
  borrar solo datos, abortar en la gracia, borrar también la carpeta). Suite e2e
  completa: 16/16.

## Fase 4+ — Pendiente

Ver `PLAN.md`: Fase 5 IA (`agent.rs`).

## Diferido (fuera de fase)

- **Botones de la barra de título** no respetan la config de KDE — ver
  `docs/KNOWN_ISSUES.md`. Se revisará al finalizar todas las fases.
