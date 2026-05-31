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

## Fase 4+ — Pendiente

Ver `PLAN.md`: migración entre sistemas, import LocalWP, settings completo,
empaquetado del plasmoid; y Fase 5 IA (`agent.rs`).

## Diferido (fuera de fase)

- **Botones de la barra de título** no respetan la config de KDE — ver
  `docs/KNOWN_ISSUES.md`. Se revisará al finalizar todas las fases.
