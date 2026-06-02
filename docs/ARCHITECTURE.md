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
| `docker.rs` | `DockerManager` (bollard): red, ensure_db/ensure_nginx, start/stop_site, teardown, `exec`/`exec_as` (fija usuario; chequea exit code), helpers uid/gid e imagen-context. `wait_db_ready` (gatea sobre TCP antes de usar la DB). Selección de endpoint (`select_endpoint`/`autoselect_endpoint`/`preflight_endpoint`) con autodetección de puerto libre. |
| `nginx.rs` | Render/escritura/borrado de vhosts en `~/.config/wordpress-panel/nginx/conf.d/`. |
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
| `autologin.rs` | `open_admin`: token efímero (transient WP, 60s, un solo uso) + abre navegador; el mu-plugin `panel-autologin.php` valida y loguea al admin. El mu-plugin lo inyecta `wordpress::sync_mu_plugins` al crear/migrar; `repair_autologin` lo reinyecta en proyectos viejos (import LocalWP) que no lo traían. |
| `github.rs` | `gh`/`git` en el HOST (no container, los archivos están bind-montados): `status`, `clone`, `pull`, `remove_dir`, `propose_path`. Sin auth propia. |
| `ssl.rs` | `generate`: cert/key por dominio con mkcert en `ssl/` del proyecto. La CA local (`mkcert -install`) se hace una vez en `first-run.sh`. |
| `dbus.rs` | Servidor D-Bus (zbus) para el plasmoid KDE; arranca en el `setup` de Tauri. Ver sección D-Bus. |
| `backup.rs` | `export_db`: `wp db export` → `app/sql/db-{timestamp}.sql` (dump en la raíz pública montada, luego movido fuera de la raíz servida). `rotate_dumps`: deja solo los N `db-*.sql` más recientes. `stop_site` los invoca para exportar-al-detener. |
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
| `open_admin` | `id` | `()` | Abre el admin en el navegador (auto-login si está activo). |
| `repair_autologin` | `id` | `SiteConfig` | Activa `oneClickAdmin` y reinyecta los mu-plugins del panel (auto-login + mailpit). Para proyectos importados de LocalWP sin el plugin. No requiere proyecto encendido. |
| `stream_logs` | `id` | `()` | Inicia el stream de logs → eventos `log:{id}`. |
| `stop_logs` | `id` | `()` | Detiene el stream de logs. |
| `list_plugins` | `id` | `String` (JSON) | `wp plugin list`. |
| `list_themes` | `id` | `String` (JSON) | `wp theme list`. |
| `gh_status` | — | `GhStatus` | gh instalado/autenticado + usuario. |
| `gh_clone` | `id, kind, repo, branch` | `SiteConfig` | Clona theme/plugin + registra en config.json. |
| `gh_pull` | `id, path, branch` | `String` | `git pull` de una carpeta. |
| `gh_pull_all` | `id` | `String` | Pull de theme + todos los plugins. |
| `gh_remove` | `id, kind, path` | `SiteConfig` | Borra carpeta + desregistra. |
| `regenerate_ssl` | `id` | `()` | Regenera cert mkcert + reload nginx. |
| `set_site_group` | `id, group?` | `SiteConfig` | Asigna/quita grupo del proyecto. |
| `set_site_minio` | `id, enabled` | `SiteConfig` | Activa/desactiva MinIO; arranca el servicio si el proyecto corre. |
| `export_db` | `id` | `String` (ruta) | Dump de la DB a `app/sql/`. |
| `install_cli_wrapper` | — | `String` | Instala `wp`/`wordpress-panel-cli` en `~/.local/bin`. También se ejecuta solo al arrancar el panel. |
| `open_terminal` | `id` | `()` | Instala el wrapper (idempotente) y abre un emulador de terminal con cwd en la carpeta del proyecto; dentro funciona `wp`. |
| `open_mailpit` | — | `()` | Abre la UI de Mailpit. |
| `open_minio` | — | `()` | Abre la consola de MinIO. |
| `open_adminer` | `id` | `()` | Arranca `panel-adminer` y abre el navegador en la DB del proyecto (requiere proyecto corriendo). |
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
├── panel.json                     estado global del panel (Endpoint elegido)
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
  fallback `index.html`. El routing (incl. `/site/[id]`) es 100% cliente.
- `lib/api.ts` envuelve `invoke`. `lib/types.ts` = espejo de los modelos serde
  (incl. `Endpoint` + helper `siteUrl`).
- Componentes (`lib/components/`): `OpConsole.svelte` — consola modal que escucha
  `op-log` y muestra los pasos en vivo (botón «Cerrar» bloqueado mientras corre;
  botón «Cancelar borrado» opcional). `DeleteProjectModal.svelte` — borrado de un
  proyecto: modal de confirmación (titulado con el nombre + checkbox para borrar
  también la carpeta) y, al confirmar, `OpConsole` con la ventana de gracia de 5 s
  y `delete_site`. Se usa en el dashboard y en `/site/[id]` (`bind:site`).
  `ImportProjectModal.svelte` — modal del dashboard (botón «Importar proyecto»)
  que lista las carpetas desconectadas (`list_disconnected_sites`) y re-importa
  la elegida (`import_disconnected_site`) mostrando el progreso en `OpConsole`.
- Tailwind (`darkMode: 'class'`, clase `dark` en `<html>`). Tema dark-only navy
  **"DevFlow Dark Blue"** (`DESIGN.md`): la escala `zinc` está remapeada a navy
  en `tailwind.config.js`, así los `dark:bg-zinc-*` existentes heredan el tema
  sin tocarse; token `primary` (#4d8eff). Estilos base de inputs (fondo navy,
  texto claro, foco azul) globales en `app.css` (`@layer base`/`components`).
