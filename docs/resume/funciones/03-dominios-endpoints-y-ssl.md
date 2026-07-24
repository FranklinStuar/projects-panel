# 03 — Dominios, endpoints y SSL

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para la resolución de dominios `.test`
> vía dnsmasq wildcard, la publicación en `127.0.0.1` (con coexistencia con
> LocalWP mediante puertos altos) y la firma SSL local con mkcert.

## Resultado para el usuario

Todos los proyectos del panel resuelven `*.test` en `127.0.0.1` sin editar
`/etc/hosts` por proyecto. El panel publica en `127.0.0.1:80/443` si están
libres; si no (típicamente porque LocalWP escucha en `0.0.0.0:80`), elige
automáticamente un par de puertos altos (≥8080/8443) y los persiste para que
las URLs de los sitios no cambien en cada arranque. Si el usuario quiere
forzar otra IP loopback, el panel instala la regla dnsmasq extra vía
`pkexec`. Cada proyecto con SSL activado tiene un cert mkcert firmado por la
CA local.

## Precondiciones

- **dnsmasq wildcard** instalado por `scripts/first-run.sh` (privilegios):
  copia `address=/test/127.0.0.1` a `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf`
  y recarga `NetworkManager`. Ver `domain::install_wildcard` para reglas
  alternativas.
- **mkcert** + CA local (`mkcert -install`) — necesario para que el navegador
  confíe en los certs sin warnings. Detectado por `system::mkcert_ca_installed`
  (`rootCA.pem` en `$(mkcert -CAROOT)`).
- Daemon de Docker accesible (depende de Fase 1, ciclo de vida).
- Endpoint persistido en `~/.config/wordpress-panel/panel.json` (`PanelConfig.endpoint`).
- `panel-nginx` levantado (`docker::ensure_nginx`).

## Flujo feliz

### A. Resolución de dominios (`*.test`)

1. `domain::wildcard_active` sondea `("panel-probe.test", 0).to_socket_addrs()`
   y comprueba que alguna addr sea loopback. Si lo es, NO hace nada más.
2. Si no está activo, `domain::ensure_wildcard` escribe el snippet en
   `config_dir/dnsmasq-panel.conf` (sin privilegios). La instalación en
   `/etc/NetworkManager/dnsmasq.d/…` se hace en `first-run.sh`.
3. `docker::start_site` invoca `domain::ensure_wildcard` después de levantar
   el proyecto (idempotente).
4. Si el endpoint usa una IP loopback **alterna** (no `127.0.0.1`), se
   llama `domain::install_wildcard(ip)` que:
   - `pkexec sh -c "install -d /etc/NetworkManager/dnsmasq.d && printf '%s' 'address=/test/{ip}' > /etc/NetworkManager/dnsmasq.d/wordpress-panel.conf && systemctl reload NetworkManager"`.
   - Idempotente.
5. `domain::resolves_to(ip)` confirma que la IP alternativa está respondiendo
   (mismo `to_socket_addrs` con `panel-probe.test`).

### B. Selección del endpoint (punto de publicación)

1. `docker::ensure_nginx` llama `DockerManager::select_endpoint`:
   - Si `config::load_endpoint` devuelve `Some`, úsalo.
   - Si no, `DockerManager::autoselect_endpoint`:
     - `http_port = netcheck::pick_alt_port(8080).unwrap_or(8080)`.
     - `https_port = netcheck::pick_alt_port(8443).unwrap_or(8443)`. Si
       coincide con `http_port`, salta al siguiente libre.
     - `loopback_ip = domain::DEFAULT_IP` (`127.0.0.1`).
   - Persiste con `config::save_endpoint`.
2. `docker::ensure_endpoint_dns(ep)`: si `loopback_ip != 127.0.0.1` y no
   resuelve, llama `domain::install_wildcard(loopback_ip)`.
3. `docker::preflight_endpoint(ep)`:
   - Parsea `loopback_ip` como `Ipv4Addr`.
   - Para `http_port` y `https_port`, `netcheck::port_status(port).free_for(ip)`.
   - Si NO está libre, devuelve error con el holder: `netcheck::holder_name(port)`
     (lee `/proc/<pid>/fd` para encontrar al dueño).
   - Mensaje: "el puerto 80 (HTTP) ya está ocupado en 127.0.0.1 (lo usa:
     <comm>). Apaga ese servicio (¿LocalWP?) o borra
     ~/.config/wordpress-panel/panel.json para reasignar puerto."
4. Si el endpoint persistido quedó inservible, `ensure_nginx` re-llama
   `autoselect_endpoint` y re-persiste.
5. Las URLs resultantes se construyen con `Endpoint::site_url(domain, ssl)`:
   - `ssl && https_port == 443` → `https://{domain}`.
   - `ssl && https_port != 443` → `https://{domain}:{https_port}`.
   - `!ssl && http_port == 80` → `http://{domain}`.
   - `!ssl && http_port != 80` → `http://{domain}:{http_port}`.

### C. Reverse-proxy (panel-nginx)

1. `docker::ensure_nginx` (si NO corriendo):
   - Borra cualquier container `panel-nginx` zombie (`remove_container
     force:true`).
   - Crea la imagen `nginx:alpine` (pull si falta).
   - `nginx::ensure_tuning` escribe `00-panel-tuning.conf` con
     `server_names_hash_bucket_size 128;` (dominios largos de los
     worktrees desbordan el default 64).
   - Bind mounts: `config_dir/nginx/conf.d:/etc/nginx/conf.d:ro` y
     `~/panel-wp:/srv/projects:ro`.
   - HostConfig: `network_mode = panel-net`, `port_bindings` =
     `{80/tcp: <loopback_ip>:<http_port>, 443/tcp: <loopback_ip>:<https_port>}`.
2. `docker::ensure_nginx` (corriendo): `reload_nginx` o recreate-and-start:
   - `exec("nginx", "-s", "reload")` si el container responde.
   - Si falla (zombie tras setns/nsexec), `remove_container force:true` +
     `ensure_nginx` (un start limpio relee conf.d, equivale al reload).
3. `docker::reload_nginx` se llama tras `write_vhost` en `start_site` y
   tras `remove_vhost` en `stop_site`.

### D. Vhost por proyecto

1. `nginx::write_vhost(site)` escribe `config_dir/nginx/conf.d/{id}.conf`
   con `nginx::render_vhost(site)`:
   - `server_name {domain}`.
   - `listen 80; return 301 https://…` (si SSL) o `listen 80` (si no).
   - `listen 443 ssl; http2 on;` y `ssl_certificate`/`ssl_certificate_key`
     apuntan a `~/panel-wp/{slug}/ssl/{cert,key}.pem`.
   - `root /srv/projects/{slug}/app/public` (o, para clones/worktrees,
     ver abajo).
   - `location / { try_files $uri $uri/ /index.php?$args; }` (WP pretty
     permalinks).
   - `location ~ \.php$ { fastcgi_pass {container_name}:9000; … }` con
     `SCRIPT_FILENAME /var/www/html$fastcgi_script_name` (vista del
     container, no del nginx).
   - Si `clone_of`:
     - `location ^~ /wp-content/uploads/ { root /srv/projects/{slug}/app/public;
       try_files $uri @uploads_base; }`
     - `location @uploads_base { root /srv/projects/{parent}/app/public;
       try_files $uri =404; }`
   - Si `worktree_of`:
     - `root /srv/projects/{parent}/app/public` (la raíz son los
       estáticos del PADRE).
     - `location ~ ^/{target_path}/(.+\.(…))$ { alias /srv/projects/{wt}/wt/{basename}/$1; … }`
       (location ANTES del static genérico, sirve los assets del repo
       objetivo desde su `git worktree`).
2. `nginx::remove_vhost(site)` borra el archivo.
3. `docker::reload_nginx` para que nginx relea `conf.d`.

### E. SSL (mkcert)

1. `ssl::generate(site)`:
   - Asegura `~/panel-wp/{slug}/ssl/`.
   - `mkcert -cert-file $ssl/cert.pem -key-file $ssl/key.pem {site.domain}`.
   - Devuelve error si mkcert no está instalado (`"ejecutando mkcert (¿instalado?
     ver first-run.sh)"`).
2. `mkcert -install` (CA local) se hace una sola vez en
   `scripts/first-run.sh`. Verificación: `system::mkcert_ca_installed` →
   `(mkcert -CAROOT)/rootCA.pem` debe existir.
3. `regenerate_ssl(id)` (en `lib.rs`) llama `ssl::generate` + `docker::reload_nginx`
   para regenerar el cert tras renovar la CA.

### F. Detección de puertos ocupados (`netcheck`)

- `netcheck::port_status(port)` lee `/proc/net/tcp` y `/proc/net/tcp6`
  (little-endian, estado `0A` = LISTEN) y combina IPv4 + IPv6:
  - `Free` (sin listener).
  - `Wildcard` (`0.0.0.0:port` o `:::port`).
  - `Specific(Vec<Ipv4Addr>)` (lista de IPs concretas).
- `port_status(port).free_for(ip)`:
  - `Free` → true.
  - `Wildcard` → false (un `0.0.0.0:80` bloquea CUALQUIER bind).
  - `Specific(ips)` → `!ips.contains(ip)`.
- `netcheck::pick_alt_port(start)` primer puerto libre en `127.0.0.1` desde
  `start`.
- `netcheck::holder_name(port)` busca el comm del proceso cuyo fd apunta al
  socket (mejor esfuerzo; puede no encontrar si no tenemos acceso a `/proc/<pid>/fd`).

## Variantes

- **Otra IP loopback**: `domain::DEFAULT_IP = 127.0.0.1`. Si el panel
  detecta que `80/443` están tomados por IPs concretas (no wildcard), elige
  `127.0.0.2`–`127.0.0.254` y `domain::install_wildcard(ip)` regla ese
  dominio vía `pkexec`. Las IPs loopback concretas no chocan entre sí
  (`127.0.0.1:80` no bloquea `127.0.0.2:80`).
- **URLs limpias (80/443) vs puerto alterno**: UI muestra tag
  "URLs limpias" / "puerto alterno" en `/settings` según
  `endpoint.httpPort == 80 && endpoint.httpsPort == 443`.
- **HTTP sin SSL**: `NginxService.ssl = false`. El vhost solo tiene `listen 80`
  (sin redirect). El cert NO se genera.
- **Clones temporales** (`config::clone_of`): uploads viejos del padre
  accesibles ro vía fallback nginx (ver ficha 06).
- **Worktree-project** (`config::worktree_of`): root del vhost apunta al
  PADRE y los assets del repo objetivo se sirven por `alias` desde el
  worktree.

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `~/.config/wordpress-panel/panel.json` (campo `endpoint`) | `config::load_endpoint`, `endpoint_or_default` | `save_endpoint`, `clear_endpoint` |
| `~/.config/wordpress-panel/nginx/conf.d/{id}.conf` | `panel-nginx` (bind ro) | `nginx::write_vhost` / `remove_vhost` |
| `~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf` | `panel-nginx` | `nginx::ensure_tuning` (idempotente) |
| `~/panel-wp/{slug}/ssl/cert.pem` + `key.pem` | `panel-nginx` (vhost) | `ssl::generate` (mkcert) |
| `~/panel-wp` | `panel-nginx` (bind ro `/srv/projects`) | — |
| `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` | NetworkManager/dnsmasq | `domain::install_wildcard` (vía `pkexec`) |
| `~/.config/wordpress-panel/dnsmasq-panel.conf` | (no usado por dnsmasq) | `domain::ensure_wildcard` (sin root) |
| `$(mkcert -CAROOT)/rootCA.pem` | navegador | `mkcert -install` (first-run.sh) |

## Containers / servicios

- `panel-nginx` — reverse-proxy compartido on-demand.
- `panel-adminer` — adminer 4 (ver ficha 05). El autologin aprovecha la
  `?server=…&username=…&db=…` del `open_adminer` (`docker::ADMINER_UI_PORT`).
- `panel-{db}-{ver}` — DB compartida. La conexión entre WP y MySQL pasa
  por `panel-net` (no host). El cert autofirmado de MySQL 8 no afecta
  porque el cliente es `mysql` dentro del container de DB (socket local).

## Fallos y compensaciones

- **Puerto en uso al arrancar**: `preflight_endpoint` falla con
  `holder_name` (el nombre del proceso). Si el endpoint persistido quedó
  inservible, `ensure_nginx` lo re-elige.
- **dnsmasq no resuelve**: `domain::wildcard_active` devuelve false. La
  UI muestra el ítem rojo en `/settings` y el usuario debe ejecutar
  `bash scripts/first-run.sh`.
- **mkcert no instalado**: `ssl::generate` falla con mensaje sobre
  `first-run.sh`. El proyecto se queda sin HTTPS (es un parámetro
  opcional en `create_site`).
- **mkcert CA no instalada**: los `.test` cargan con warning en el
  navegador. La UI pinta "CA de mkcert" rojo en `/settings`.
- **Panel-nginx zombie tras apagón sucio**: `docker::reload_nginx` detecta
  `exec` roto (setns/nsexec) y recrea el container.
- **Dominio muy largo (worktree)**: `00-panel-tuning.conf` con
  `server_names_hash_bucket_size 128` evita que nginx no arranque.
- **Cert caducado / nuevo host**: `regenerate_ssl(id)` re-firma el cert
  y recarga nginx.
- **El usuario quiere reasignar puerto**: `reset_endpoint` olvida el
  endpoint persistido; el siguiente `ensure_nginx` lo auto-elige. CUIDADO:
  los proyectos ya instalados guardan `siteurl` con el puerto viejo.

## UI / IPC / CLI / MCP disponibles

### IPC (`lib.rs`)

- `panel_endpoint()` — devuelve el `Endpoint` actual.
- `system_status()` — docker, red, dnsmasq (`wildcard_active`), mkcert
  (`mkcert_ca_installed`), wrappers, plasmoid, endpoint, rutas.
- `create_panel_network()` — `docker::ensure_network`.
- `reset_endpoint()` — `config::clear_endpoint`.
- `regenerate_ssl(id)` — `ssl::generate` + `reload_nginx`.

### UI (`src/routes/`)

- `/` — los badges de estado de los proyectos muestran la URL efectiva
  (`siteUrl(ep, domain, ssl)`) cuando están corriendo.
- `/domains` (`src/routes/domains/+page.svelte`) — tabla de dominios
  con esquema (HTTP/HTTPS).
- `/settings` (`src/routes/settings/+page.svelte`) — checklist del
  sistema (Docker, red, dnsmasq, mkcert, wrappers, plasmoid) + bloque
  "Punto de publicación" con `loopbackIp:httpPort/httpsPort` y tag
  URLs limpias/puerto alterno. Botón "Reasignar puerto" →
  `reset_endpoint()`.
- `/services` (`src/routes/services/+page.svelte`) — botones para abrir
  Mailpit, MinIO, instalar wrappers.

### CLI (`scripts/wordpress-panel-cli.sh`)

- `wordpress-panel-cli list` — muestra estado y dominio.
- `wordpress-panel-cli open site` — abre la URL pública.
- `wordpress-panel-cli open admin` — auto-login al admin.

### MCP (`mcp/server.mjs`)

- `list_projects` (estado, dominio).
- `open_project { what: site | admin }`.

## Tests

- `config::tests::site_url_cuatro_ramas` — 4 ramas de `Endpoint::site_url`.
- `config::tests::endpoint_serializa_en_camelcase`.
- `netcheck::tests::v4_little_endian` — `parse_v4("0100007F") ==
  127.0.0.1`, `parse_v4("00000000") == 0.0.0.0`, `parse_v4("0200007F") ==
  127.0.0.2`.
- `netcheck::tests::listen_addr_matches_port_and_state` — filtro de
  estado LISTEN (`0A`) y puerto.
- `netcheck::tests::free_for_semantics` — Free/Wildcard/Specific.
- `nginx::tests::vhost_normal_sin_uploads_block`,
  `vhost_clone_incluye_uploads_fallback_http` / `_ssl`,
  `vhost_worktree_root_padre_y_alias_objetivo`.

## Limitaciones

- Si LocalWP escucha en `0.0.0.0:80` (wildcard), el panel NO puede tomar
  80/443 aunque tenga otra IP loopback (los kernels bloquean binds
  específicos cuando hay un wildcard). Solución: puertos altos.
- `Wildcard` (`free_for`) considera `::` IPv6 como bloqueo (en la práctica
  `IPV6_V6ONLY` no está activo, así que `::` también cubre IPv4).
- `mkcert` solo firma `*.test` (no es un CA real). HTTPS en otros TLDs
  requiere cambiar la regla dnsmasq.
- `selector de tema claro/oscuro` no está en `/settings` (decisión de
  diseño, tema oscuro fijo).
- `domain::DEFAULT_IP = 127.0.0.1` por defecto, pero `wp siteurl` se
  fija al dominio (con puerto en `migrate::fix_site_url`).
- `panel-nginx` se PUBLICA en una sola IP loopback (`loopback_ip`); no
  escucha en `0.0.0.0`. Para escuchar en varias IPs, re-elegir endpoint.

## Invariantes a NO romper

- **Endpoint se elige UNA vez** y se persiste; cambiar el endpoint
  rompe los `siteurl` de los sitios ya instalados.
- **Cede 80/443 a LocalWP**: el panel SIEMPRE publica en puertos altos
  si `Wildcard` está ocupado. Las URLs del panel llevan el puerto.
- **Una sola regla dnsmasq wildcard** (`address=/test/127.0.0.1` /
  `address=/test/<ip_alterna>`). No se edita `/etc/hosts` por proyecto.
- **`server_names_hash_bucket_size 128`** siempre presente en
  `00-panel-tuning.conf` (los worktrees generan dominios largos).
- **`ssl_certificate` siempre apunta a `~/panel-wp/{slug}/ssl/`** — el
  cert se regenera con mkcert al migrar de sistema.
- **`fastcgi_param SCRIPT_FILENAME /var/www/html$fastcgi_script_name`**
  usa la vista del container php, no la de nginx.

## Recomendaciones breves (rebuild)

- `Endpoint::site_url` debe ser SIEMPRE la fuente de URLs (no `format!`
  ad-hoc en la UI).
- El endpoint se persiste por sesión — NO recalcular en cada comando.
- al cambiar `IMAGE_REV` o la versión de mkcert, regenerar el `panel-nginx`
  (no solo `reload`).
- En `docker::ensure_nginx`, ANTES de bindear, hacer
  `preflight_endpoint` (falla con mensaje claro en vez del 500 opaco).

## Fuentes primarias

- `src-tauri/src/config.rs` — `Endpoint`, `site_url`, `is_default`,
  `load_endpoint`, `save_endpoint`, `clear_endpoint`, `load_panel_config`,
  `save_panel_config`, `panel_config_path`.
- `src-tauri/src/domain.rs` — `DEFAULT_IP`, `wildcard_active`,
  `resolves_to`, `ensure_wildcard`, `install_wildcard`, `snippet_path`,
  `install_target`.
- `src-tauri/src/netcheck.rs` — `port_status`, `pick_alt_port`,
  `pick_loopback_ip`, `holder_name`, `free_for`, `is_wildcard`.
- `src-tauri/src/docker.rs` — `ensure_nginx`, `select_endpoint`,
  `autoselect_endpoint`, `ensure_endpoint_dns`, `preflight_endpoint`,
  `reload_nginx`, `remove_container`, `DockerManager::ensure_network`.
- `src-tauri/src/nginx.rs` — `render_vhost`, `write_vhost`, `remove_vhost`,
  `ensure_tuning`, `conf_d_dir`, `project_dirname`.
- `src-tauri/src/ssl.rs` — `generate`, `has_cert`, `ssl_dir`.
- `src-tauri/src/system.rs` — `status`, `mkcert_ca_installed`,
  `wrapper_installed`, `plasmoid_installed`.
- `src/lib/types.ts` — `Endpoint`, `siteUrl`.
- `src/lib/api.ts` — `panelEndpoint`, `systemStatus`, `createPanelNetwork`,
  `resetEndpoint`, `regenerateSsl`.
- `src/routes/settings/+page.svelte` — checklist + endpoint.
- `src/routes/domains/+page.svelte` — listado de dominios.
- `scripts/first-run.sh` — dnsmasq + mkcert + wrappers.
- `mcp/server.mjs` — `list_projects`, `open_project`.
