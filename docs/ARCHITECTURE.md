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
                   │ bollard (socket Docker) + CLI docker (solo build img)
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
| `panel-mailpit` | `axllent/mailpit` | (planificado) captura de correo. |
| `panel-minio` | `minio/minio` | (planificado) S3 sim. |

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
| `config.rs` | Modelos (`SiteConfig`, `Services`, `DbType`, `SiteStatus`, `SiteState`), rutas (`config_dir`, `projects_root`), persistencia (`load_all_sites`, `read/write_site_config`, `find_site`). |
| `docker.rs` | `DockerManager` (bollard): red, ensure_db/ensure_nginx, start/stop_site, teardown, exec, helpers uid/gid e imagen-context. |
| `nginx.rs` | Render/escritura/borrado de vhosts en `~/.config/wordpress-panel/nginx/conf.d/`. |
| `php.rs` | `ensure_php_image` (docker build por versión), `wp_cli_phar_path` (descarga el phar). |
| `domain.rs` | dnsmasq wildcard `*.test`: snippet + detección de resolución. |
| `wordpress.rs` | `create_site` end-to-end, `download_core` (tarball), `fetch_versions` (API wp.org, cache 24h), DB/wp-config/install vía WP-CLI, mu-plugin mailpit. |
| `wpcli.rs` | `run()` WP-CLI dentro del container del proyecto. |
| `logs.rs` | `spawn_stream`: sigue (`follow`) los logs del container y los emite como evento `log:{id}`. Cancelable vía `JoinHandle::abort()`. |
| `autologin.rs` | `open_admin`: token efímero (transient WP, 60s, un solo uso) + abre navegador; el mu-plugin `panel-autologin.php` valida y loguea al admin. |
| `github.rs` | `gh`/`git` en el HOST (no container, los archivos están bind-montados): `status`, `clone`, `pull`, `remove_dir`, `propose_path`. Sin auth propia. |

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
| `open_admin` | `id` | `()` | Abre el admin en el navegador (auto-login si está activo). |
| `stream_logs` | `id` | `()` | Inicia el stream de logs → eventos `log:{id}`. |
| `stop_logs` | `id` | `()` | Detiene el stream de logs. |
| `list_plugins` | `id` | `String` (JSON) | `wp plugin list`. |
| `list_themes` | `id` | `String` (JSON) | `wp theme list`. |
| `gh_status` | — | `GhStatus` | gh instalado/autenticado + usuario. |
| `gh_clone` | `id, kind, repo, branch` | `SiteConfig` | Clona theme/plugin + registra en config.json. |
| `gh_pull` | `id, path, branch` | `String` | `git pull` de una carpeta. |
| `gh_pull_all` | `id` | `String` | Pull de theme + todos los plugins. |
| `gh_remove` | `id, kind, path` | `SiteConfig` | Borra carpeta + desregistra. |

**Eventos** (backend → frontend, vía `app.emit`): `log:{id}` — una línea de log por
evento. El frontend se suscribe con `listen()` de `@tauri-apps/api/event`. Estado
de los streams activos en `LogStreams` (managed state, `Mutex<HashMap>`).

## Rutas en disco

```
~/.config/wordpress-panel/        (config_dir)
├── nginx/conf.d/{site-id}.conf    vhosts montados ro en panel-nginx
├── wp-cli.phar                    montado ro en cada wp-{id}
├── wp-versions.json               cache 24h de versiones WP
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
```

## Frontend

- SvelteKit + Svelte 5 (runes: `$state`, `$derived`, `$effect`, `$props`).
- `adapter-static` modo SPA: `+layout.ts` con `ssr=false`, `prerender=false`,
  fallback `index.html`. El routing (incl. `/site/[id]`) es 100% cliente.
- `lib/api.ts` envuelve `invoke`. `lib/types.ts` = espejo de los modelos serde.
- Tailwind (`darkMode: 'class'`, clase `dark` en `<html>`).
