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

## Fase 2 — Funcionalidades completas (en curso)

### Hecho
- **Logs en vivo** (`logs.rs`): stream `follow` de los logs del container →
  eventos `log:{id}`; tab "Logs" en la vista de proyecto con autoscroll del
  buffer (últimas 500 líneas). Start/stop atado al tab + estado `LogStreams`.
- **Auto-login one-click** (`autologin.rs` + mu-plugin `panel-autologin.php`):
  token efímero de un solo uso (transient WP 60s) → botón "Abrir admin" abre el
  navegador logueado. Inyectado en `create_site` si `oneClickAdmin`.
- **Listado de plugins/themes** (`list_plugins`/`list_themes` vía WP-CLI JSON);
  tab "Plugins / Themes" en la vista de proyecto.
- **GitHub vía `gh`** (`github.rs` + mu… no, host): clonar/pull/quitar theme y
  plugins en el host (bind-mount → cambios al instante), estado de `gh`,
  registro en config.json; tab "GitHub" con UI de theme/plugins y "Pull todo".
- **Vista de proyecto con tabs** (Info / Logs / Plugins-Themes / GitHub) + start/stop.

### Pendiente de Fase 2
- D-Bus (`dbus.rs`) + plasmoid KDE.
- SSL con mkcert.
- Grupos de proyectos.

## Fase 3+ — Pendiente

Ver `PLAN.md`: MinIO, MariaDB/Postgres, headless, migración, import LocalWP; y
Fase 5 IA (`agent.rs`).

## Diferido (fuera de fase)

- **Botones de la barra de título** no respetan la config de KDE — ver
  `docs/KNOWN_ISSUES.md`. Se revisará al finalizar todas las fases.
