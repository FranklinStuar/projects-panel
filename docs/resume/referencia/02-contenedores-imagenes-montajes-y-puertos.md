# 02 · Contenedores, imágenes, montajes y puertos

> Referencia verificada contra el commit `373841c` (rama `main`, 2026-07-23).
> Cubre cada container que dockeriza el panel, sus imágenes, los volúmenes
> bind-montados, los puertos publicados al host, el endpoint configurable y
> los casos especiales de worktree/clone. Todo se cita a `ruta::símbolo`.

## 1. Reglas generales

- **Naming**: proyectos = `wp-{site-id}` (`config::SiteConfig::container_name`),
  compartidos = `panel-*` (`docker::NETWORK/NGINX/MAILPIT/MINIO/ADMINER`).
- **Publicación al host**: solo `panel-nginx` (HTTP/HTTPS) y los puertos UI
  de `panel-mailpit`, `panel-minio` y `panel-adminer`. **Ningún container de
  proyecto publica puertos** — se alcanzan por `panel-net` (bridge interno).
- **UID/GID**: el user `www-data` (por defecto `82:82` en Alpine) se remapea
  en arranque con `PUID`/`PGID` (host) — entrypoint `docker/php/entrypoint.sh`.
- **Endpoint**: `Endpoint` (`config::Endpoint`) decide la IP loopback y los
  puertos externos de `panel-nginx`. **El panel siempre cede 80/443 a LocalWP**
  y publica en puertos ≥ 8080 (HTTP) / 8443 (HTTPS) (`docker::autoselect_endpoint`,
  `docker.rs:583-597`). Si no hay conflicto, la primera vez persiste
  `Endpoint::default() = {127.0.0.1, 80, 443}` (`config::Endpoint::default`).
- **DB partagé**: el datadir se bindea a `config_dir/db-data/{container}` para
  sobrevivir al recreado del container y a un apagón (`docker::db_data_dir`).
- **MySQL/MariaDB sin alpine**: lo confirma `docker::DbType::image` — solo
  Postgres usa `postgres:{ver}-alpine` (`docker.rs:30-37`).
- **Tres excepciones al "Docker solo vía bollard"**:
  1. `docker build` (imagen php) — `php::ensure_php_image` (`php.rs:24-52`).
  2. `docker cp` (migración datadir DB legado) — `docker::migrate_db_to_volume`
     (`docker.rs:244-279`).
  3. `docker exec -i` (import dump) — `migrate::import_dump` (`migrate.rs:243-409`).

## 2. `panel-net` — la red interna

- Constante: `docker::NETWORK = "panel-net"` (`docker.rs:24`).
- Tipo: `bridge` (`docker::ensure_network`, `docker.rs:58-72`).
- Prerrequisito de TODO: `create_panel_network` (command IPC, `lib.rs:140-144`)
  o `system::status` lo declara faltante (no detiene la app).
- DNS wildcard externo (no es parte del container, pero está acoplado):
  `domain::wildcard_rule` (`domain.rs:18-21`) → `*.test → 127.0.0.1` instalado
  en `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` por
  `domain::install_wildcard` (`pkexec`).
- `docker::PANEL_PREFIXES = ["wp-", "panel-"]` se usa para detectar huérfanos
  (`docker::running_panel_containers`, `docker.rs:109-128`,
  marcado `#[allow(dead_code)]`).

## 3. Containers por proyecto

### 3.1 `wp-{site-id}` — php-fpm

| Atributo                         | Valor                                                                                                       |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Imagen**                       | `panel-php:{php_version}-r3` (`php::ensure_php_image`, `php.rs:21-52`). `IMAGE_REV = "r3"` (`php.rs:18`).     |
| **Construcción**                 | `docker build -t panel-php:{ver}-r3 --build-arg PHP_VERSION={ver} docker/php/`.              |
| **CMD / Entrypoint**             | `entrypoint.sh` → `php-fpm`. Entrypoint remapea `www-data` con `PUID`/`PGID`.                |
| **Red**                          | `panel-net` (`network_mode`).                                                                                |
| **Puertos al host**              | **ninguno** (`docker.rs:757-759` — comentario: "NO se publican puertos al host: solo panel-nginx le habla por panel-net"). |
| **Usuario**                      | `www-data` (remapeado).                                                                                       |
| **Env vars**                     | `PUID=`, `PGID=` (uid/gid del host — `docker::host_uid_gid`, `docker.rs:1024-1039`).                          |
| **Volúmenes bind** (caso normal) | `dnsmasq:host:port` (no aplica) — ver abajo.                                                                  |

#### Montajes (caso normal, `site.worktree_of = None`)

Ver `docker::create_php_container` (`docker.rs:711-777`):

```
{site.public_dir()}                 → /var/www/html
{site.php_ini()}                    → /usr/local/etc/php/conf.d/zz-project.ini:ro
{config_dir}/wp-cli.phar            → /usr/local/bin/wp:ro
```

`suffix:` `:ro` indica solo lectura; el resto es lectura/escritura (uploads del
sitio en `app/public/wp-content/uploads`).

#### Montajes (worktree-project, `site.worktree_of = Some(…)`)

```
{parent.public_dir()}               → /var/www/html                                    (parent's whole public)
{site.worktree_root()}/{basename}   → /var/www/html/{target_path}                       (overlay: el git worktree)
{site.worktree_wp_config()}         → /var/www/html/wp-config.php                       (own wp-config)
{site.php_ini()}                    → /usr/local/etc/php/conf.d/zz-project.ini:ro
{config_dir}/wp-cli.phar            → /usr/local/bin/wp:ro
```

El comentario en `docker.rs:716-739` advierte que Docker ordena los montajes
por profundidad del destino: el parent (raíz) se monta antes y los overrides
quedan encima.

#### Generación del vhost compartido

`nginx::write_vhost` (líneas 162-167) escribe
`~/.config/wordpress-panel/nginx/conf.d/{id}.conf` con contenido de
`nginx::render_vhost` (líneas 43-160) usando la plantilla `docker/nginx/vhost.conf.tmpl`. Se monta al container `panel-nginx` desde
`conf_d_dir()` (líneas 12-16). El vhost usa
`fastcgi_pass {container_name}:9000` (php-fpm socket interno).

### 3.2 `wp-{wt-id}` — variantes worktree-project

| Atributo particular                                                            | Detalle                                                                                       |
| ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `site.clone_of` no se usa en este container                                     | Solo cambia el vhost (clone → fallback en `nginx::render_vhost` bloques
  `^~ /wp-content/uploads/`, `nginx.rs:72-89`). |
| `site.worktree_of` activa la rama de montajes del padre                          | `docker::create_php_container` (líneas 719-750).                                              |
| `site.clone_of` o `site.worktree_of` poblados → `site.last_migrated_at` se respeta| `clone::create_clone` (líneas 73-105) y `worktree::create_worktree` (líneas 128-162).       |

### 3.3 Containers de proyecto NO expuestos al host

Confirmado en `docker.rs:755-757` (host_config no tiene `port_bindings`).

## 4. Containers compartidos

### 4.1 `panel-nginx` — reverse-proxy único

| Atributo          | Valor                                                                                                                          |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| **Imagen**        | `nginx:alpine` (`docker::ensure_nginx`, `docker.rs:493`).                                                                       |
| **Red**           | `panel-net`.                                                                                                                   |
| **Puertos host**  | `{loopback_ip}:{http_port}/tcp` y `{loopback_ip}:{https_port}/tcp` (`docker.rs:514-528`).                                      |
| **Endpoint**      | `select_endpoint` (usa persistido o `autoselect_endpoint`).                                                                   |
| **Montajes**      | `{conf_d_dir}` → `/etc/nginx/conf.d:ro` y `{projects_root}` → `/srv/projects:ro` (`docker.rs:530-533`).                          |
| **Reinicio**     | `docker::reload_nginx` (líneas 638-659) usa `nginx -s reload`; si falla (zombie tras apagón), recrea el container.               |
| **Tuning extra** | `00-panel-tuning.conf` con `server_names_hash_bucket_size 128;` (`nginx::ensure_tuning`).                                       |

#### Selección del endpoint

- `select_endpoint` (`docker.rs:570-578`): persistido `Endpoint` o autodetección.
- `autoselect_endpoint` (`docker.rs:583-597`): `pick_alt_port(8080)` y
  `pick_alt_port(8443)` (de `netcheck::pick_alt_port`, basadas en
  `/proc/net/tcp{,6}`). Si los dos caen en el mismo, salta a `hp + 1`.
- `preflight_endpoint` (`docker.rs:616-635`): chequea `PortStatus::free_for(ip)`
  para HTTP y HTTPS; si no, devuelve error con el nombre del proceso titular
  (`netcheck::holder_name`).
- `ensure_endpoint_dns` (`docker.rs:601-612`): si la IP no es `DEFAULT_IP`,
  instala la regla wildcard dnsmasq con `pkexec`.

### 4.2 `panel-mysql-{ver}`, `panel-mariadb-{ver}`, `panel-postgres-{ver}` — DB compartido

| Atributo                            | Valor                                                                                                          |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Imágenes**                        | `mysql:{ver}` / `mariadb:{ver}` / `postgres:{ver}-alpine` (`docker::DbType::image`).                            |
| **Nombre del container**            | `format!("{prefix}-{ver_sin_puntos}", DbType::service_prefix())` (`docker::db_container_name`).                |
| **Red**                             | `panel-net`.                                                                                                   |
| **Puertos al host**                 | **ninguno** — los proyectos hablan por `panel-net` (`docker.rs:182-186`).                                       |
| **Datadir durable**                 | `config_dir/db-data/{container}` ↔ `DbType::datadir()` (`docker.rs:161-185`).                                   |
| **Env vars** (`docker::db_env`)     | MySQL/MariaDB: `MYSQL_ROOT_PASSWORD=panel`, `MYSQL_ROOT_HOST=%`. Postgres: `POSTGRES_PASSWORD=panel`, `POSTGRES_USER=panel`. |
| **Root password**                   | `panel` (literal único del sistema).                                                                            |
| **Credenciales para php-fpm**       | `root`/`panel` (cliente que monta `wp-config.php` con `--dbuser=root --dbpass=panel`).                          |
| **Ready TCP**                       | `wait_db_ready` (gatea con `mysql -h127.0.0.1` o `pg_isready`) — timeout 60 s (`docker.rs:286-302`).             |

#### Versiones (puertas cerradas pero no enumeradas aquí)

`{ver}` es la `services.db.version` de los proyectos. La UI no cuenta cuántos
containers DB hay — el panel solo enciende los que necesita
(`docker::ensure_db`).

### 4.3 `panel-mailpit` — SMTP catcher

| Atributo           | Valor                                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------------------------ |
| **Imagen**         | `axllent/mailpit:latest` (`docker.rs:316`).                                                             |
| **Red**            | `panel-net`.                                                                                            |
| **SMTP interno**   | `panel-mailpit:1025/tcp` (expuesto, no bind al host).                                                   |
| **UI web**         | `127.0.0.1:8025` (`docker::MAILPIT_UI_PORT = 8025`, líneas 31-34).                                      |
| **Arranque**       | `ensure_mailpit` (líneas 306-349). No-op si ya corre.                                                   |
| **Encendido**      | Automático en cada `start_site` (`.ok()` para no fallar la carga si no arranca — `docker.rs:668`).       |
| **Apagado**        | `teardown_unused_shared` (`docker.rs:802-862`) lo apaga si no queda proyecto activo.                    |

### 4.4 `panel-minio` — S3 local (on-demand)

| Atributo           | Valor                                                                                                                   |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| **Imagen**         | `minio/minio:latest` (`docker.rs:363`).                                                                                 |
| **Red**            | `panel-net`.                                                                                                            |
| **Cmd**            | `server /data --console-address :9001` (`docker.rs:385-390`).                                                            |
| **API**            | `127.0.0.1:9100` (← `9000/tcp`, `docker::MINIO_API_PORT = 9100`).                                                       |
| **Console**        | `127.0.0.1:9101` (← `9001/tcp`, `docker::MINIO_CONSOLE_PORT = 9101`).                                                   |
| **Data**           | `config_dir/minio-data/` ↔ `/data` (`docker.rs:380`). Persistente (no se borra).                                         |
| **Credenciales**   | `MINIO_ROOT_USER=panel`, `MINIO_ROOT_PASSWORD=panel-secret` (`docker.rs:391-394`).                                       |
| **Arranque**       | Solo si un proyecto tiene `minio: true` (`docker::start_site`, `docker.rs:669-671`).                                     |
| **Apagado**        | `teardown_unused_shared` si nadie más lo pide (`docker.rs:842-848`).                                                      |

### 4.5 `panel-adminer` — visor de DB (on-demand)

| Atributo          | Valor                                                                                                              |
| ----------------- | ------------------------------------------------------------------------------------------------------------------ |
| **Imagen**        | `adminer:4` (`docker.rs:431`).                                                                                      |
| **Red**           | `panel-net`.                                                                                                       |
| **UI web**        | `127.0.0.1:8088` (`docker::ADMINER_UI_PORT = 8088`, líneas 34-35).                                                  |
| **Montaje**       | `docker/adminer/autologin.php` → `/var/www/html/plugins-enabled/autologin.php:ro` (`docker.rs:442-447`).            |
| **Arranque**      | `open_adminer` (Tauri command) invoca `docker::ensure_adminer` cuando la UI lo pide (`lib.rs:642-669`).            |
| **Auto-login**    | Pasa `?{driver}={server}&username={user}&db={dbname}` (`lib.rs:657-668`). `MySQL/MariaDB` usan `server=…`; `Postgres` usa `pgsql=…`. |

### 4.6 `panel-php` — *NO existe como container*

El container php por proyecto es `wp-{site-id}` (sección 3.1). La **imagen**
compartida por todos los proyectos es `panel-php:{ver}-r3`, construida por
`php::ensure_php_image`.

## 5. Tabla maestra de imágenes

| Imagen                          | Origen                                                              | Tag concreto                              |
| ------------------------------- | ------------------------------------------------------------------- | ----------------------------------------- |
| `panel-php:{ver}-r3`            | `docker/php/Dockerfile` (alpine base)                               | `panel-php:8.3-r3`, `panel-php:8.4-r3`… |
| `nginx:alpine`                  | Docker Hub                                                          | `nginx:alpine`                            |
| `mysql:{ver}`                   | Docker Hub (no alpine)                                              | `mysql:8.0`, `mysql:8.4`…                |
| `mariadb:{ver}`                 | Docker Hub (no alpine)                                              | `mariadb:10.11`, `mariadb:11.0`…          |
| `postgres:{ver}-alpine`         | Docker Hub                                                          | `postgres:16-alpine`, `postgres:17-alpine`… |
| `axllent/mailpit:latest`        | Docker Hub                                                          | `axllent/mailpit:latest`                  |
| `minio/minio:latest`            | Docker Hub                                                          | `minio/minio:latest`                      |
| `adminer:4`                     | Docker Hub                                                          | `adminer:4`                               |

## 6. Convención "compartido vs. proyecto"

| Container                          | Arranque                                     | Apagado por `stop_site`                                  |
| ---------------------------------- | -------------------------------------------- | --------------------------------------------------------- |
| `panel-nginx`                      | `ensure_nginx` en `start_site` (idempotente) | `teardown_unused_shared` solo si no quedan activos.        |
| `panel-mailpit`                    | `ensure_mailpit` en `start_site`             | `teardown_unused_shared` si no quedan activos.             |
| `panel-minio`                      | `ensure_minio` si `site.minio = true`        | `teardown_unused_shared` si nadie más lo pide.             |
| `panel-adminer`                    | `ensure_adminer` solo al ejecutar `open_adminer` | Si no quedan proyectos activos, `teardown_unused_shared` lo apaga. |
| `panel-{db-type}-{ver}`            | `ensure_db` en `start_site` (on-demand)      | `teardown_unused_shared` solo si nadie más usa esa combinación. |
| `wp-{site-id}`                     | `create_php_container` + `start_container`   | `stop_site` exporta dump → `stop_container → remove_vhost`. |

## 7. Resumen de puertos publicados al host

| Container           | Puerto host (loopback)       | Puerto container                      | Cómo se publica                                                                                              |
| ------------------- | ---------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `panel-nginx`       | `Endpoint.http_port`         | `80/tcp`                              | `port_bindings` (docker.rs:514-521). Default loopback `127.0.0.1`, rango `8080+` si conflicto.               |
| `panel-nginx`       | `Endpoint.https_port`        | `443/tcp`                             | `port_bindings` (docker.rs:522-528). Default `443`, rango `8443+` si conflicto.                              |
| `panel-mailpit`     | `127.0.0.1:8025` (`MAILPIT_UI_PORT`) | `8025/tcp` (UI)                | `host_port_map` (docker.rs:1043-1055).                                                                        |
| `panel-minio`       | `127.0.0.1:9100` (`MINIO_API_PORT`)  | `9000/tcp` (API)               | `host_port_map`.                                                                                              |
| `panel-minio`       | `127.0.0.1:9101` (`MINIO_CONSOLE_PORT`) | `9001/tcp` (console)        | `host_port_map`.                                                                                              |
| `panel-adminer`     | `127.0.0.1:8088` (`ADMINER_UI_PORT`) | `8080/tcp` (UI)               | `host_port_map`. Solo se monta, no se crea hasta la primera petición.                                         |
| `wp-{site-id}`      | (ninguno)                    | `9000/tcp` (php-fpm)                   | Sin `port_bindings`. Alcanzable solo por `panel-net`.                                                          |

> IMPORTANTE: `MAILPIT_UI_PORT` = 8025 y `ADMINER_UI_PORT` = 8088 son puertos
> internos del panel — **distintos** de los de Mailpit/Adminer en Docker
> (`8025` y `8080` respectivamente). El map solo cambia el origen (host).

## 8. Tabla maestra de montajes (bind-mounts)

| Container         | Host path                                              | Path en container                          | Flags        | Fuente                          |
| ----------------- | ------------------------------------------------------ | ------------------------------------------ | ------------ | ------------------------------- |
| `wp-{site-id}`    | `{site.public_dir()}`                                  | `/var/www/html`                            | rw (default) | `docker::create_php_container` (línea 743). |
| `wp-{site-id}`    | `{site.php_ini()}`                                     | `/usr/local/etc/php/conf.d/zz-project.ini` | `:ro`        | líneas 745-748.                 |
| `wp-{site-id}`    | `{config_dir}/wp-cli.phar`                             | `/usr/local/bin/wp`                        | `:ro`        | línea 748.                       |
| `wp-{wt-id}` (worktree) | `{parent.public_dir()}`                          | `/var/www/html`                            | rw (default) | líneas 728-739.                 |
| `wp-{wt-id}` (worktree) | `{site.worktree_root()}/{basename}`              | `/var/www/html/{target_path}`              | rw (default) | línea 729.                      |
| `wp-{wt-id}` (worktree) | `{site.worktree_wp_config()}`                    | `/var/www/html/wp-config.php`              | rw (default) | línea 731.                       |
| `panel-nginx`     | `{conf_d_dir}`                                         | `/etc/nginx/conf.d`                        | `:ro`        | `docker::ensure_nginx` (línea 530). |
| `panel-nginx`     | `{projects_root}`                                      | `/srv/projects`                            | `:ro`        | línea 531.                       |
| `panel-mysql*`    | `{config_dir}/db-data/{container}`                     | `/var/lib/mysql`                           | rw (default) | `docker::ensure_db` (línea 185). |
| `panel-postgres*` | `{config_dir}/db-data/{container}`                     | `/var/lib/postgresql/data`                 | rw (default) | línea 185.                       |
| `panel-minio`     | `{config_dir}/minio-data`                              | `/data`                                    | rw (default) | `docker::ensure_minio` (línea 380). |
| `panel-adminer`   | `docker/adminer/autologin.php` (asset del build)       | `/var/www/html/plugins-enabled/autologin.php` | `:ro`      | `docker::ensure_adminer` (líneas 442-446). |

## 9. Reglas de teardown (ciclo de vida)

`docker::teardown_unused_shared` (`docker.rs:802-862`):

1. Recorre los **otros** proyectos activos. Construye:
   - `any_active` (`bool`).
   - `any_minio` (`bool`).
   - `active_dbs` (`Vec<String>` con los nombres `panel-{prefix}-{ver}`).
2. **DB compartida**: si `{stopped.db}.{prefix}-{ver}` no está en `active_dbs`
   y está corriendo → `stop_container`.
3. **MinIO**: si nadie activo lo pide y está corriendo → `stop_container`.
4. **nginx + mailpit + adminer**: si `!any_active` → `stop_container` para los tres.

`stop_site` (`docker.rs:781-800`):

1. Si `wp-{id}` corre → `backup::export_db` (`source = "stop"`) → `rotate_dumps(3)`
   → `stop_container`.
2. `nginx::remove_vhost` → `reload_nginx` (best-effort).
3. `teardown_unused_shared`.

`start_site` (`docker.rs:664-709`):

1. `ensure_network` (idempotente).
2. `ensure_db` (motor y conexión TCP listos).
3. `ensure_mailpit` (best-effort).
4. `ensure_minio` si `site.minio`.
5. `php::ensure_php_image` (build si no existe).
6. Si container existe con otra imagen → `remove_container(force=true)`.
7. Si no existe → `create_php_container`.
8. `start_container` si no está corriendo.
9. `nginx::write_vhost` → `ensure_nginx` → `reload_nginx`.
10. `domain::ensure_wildcard` (best-effort).

## 10. Creación del container php — orden de argumentos

`docker::create_php_container` (líneas 711-777) — observación técnica:

- Orden de `binds`: parent → worktree override → wp-config override → ini → wp.
  (Docker resuelve la precedencia por profundidad del destino, no por orden de
  declaración del bind.).
- `HostConfig.network_mode = Some(NETWORK.to_string())`.
- `HostConfig.binds = Some(binds)`.
- `HostConfig` NO tiene `port_bindings`.
- `Config.env` solo lleva `PUID`/`PGID`.
- `Config.image` = el tag resuelto por `ensure_php_image`.

## 11. Vhost — caso especial `clone_of`

`nginx::render_vhost` cuando `site.clone_of = Some(…)` (líneas 72-88) genera:

```nginx
location ^~ /wp-content/uploads/ {
    root /srv/projects/{clone_dirname}/app/public;
    try_files $uri @uploads_base;
}
location @uploads_base {
    root /srv/projects/{parent_dirname}/app/public;
    try_files $uri =404;
}
```

Cubierto por tests `nginx.rs:217-247`.

## 12. Vhost — caso especial `worktree_of`

`nginx::render_vhost` cuando `site.worktree_of = Some(…)` (líneas 53-69) usa
`root = /srv/projects/{parent_dirname}/app/public` (no el del worktree) y añade
un bloque `location ~ ^/{target_path}/(.+\.(css|js|...))` que sirve el archivo
desde `/srv/projects/{worktree_dirname}/wt/{basename}/{rest}` con `alias`.

Cubierto por test `nginx.rs:249-274`.

## 13. Resumen ejecutivo (lo que cuenta)

- **6 containers** puede levantar el panel (nginx + 3 DB + mailpit + minio +
  adminer) + **N containers php** (`wp-{site-id}`).
- **3 imágenes** son del proyecto (`panel-php`), todas las demás son Docker Hub.
- **2 endpoints UI** viven en loopback: `127.0.0.1:8088` (adminer) y
  `127.0.0.1:8025` (mailpit).
- **Puertos altos por defecto** (8080/8443+): el panel nunca asume 80/443 los
  libere LocalWP.
- **Cinco rutas persistentes** (`config_dir/nginx/conf.d`, `…/db-data/{c}`,
  `…/minio-data`, `…/wp-cli.phar`, `…/wp-versions.json`).
- **Tres llamadas a `docker` CLI** que escapan a bollard (`docker build`,
  `docker cp`, `docker exec -i`), todas con justificación en código.

## 14. Estado de deuda / Diferido

- `docker::running_panel_containers` (`docker.rs:109-128`) está marcado
  `#[allow(dead_code)]` por una nota **"detección de huérfanos (shutdown.rs) en Fase 2"**.
  No se invoca; no existe `shutdown.rs`.
- `docker::remove_container` (`docker.rs:875-889`) se usa solo desde
  `lib::delete_site` y desde `worktree::remove_worktree` (limpieza tras error).
- **Port forwarding a IPs loopback alternas** (`netcheck::pick_loopback_ip`):
  existe la función pero `autoselect_endpoint` ya siempre cede 80/443; en
  práctica se queda en `127.0.0.1` con puertos altos. Conservado como fallback.
- **Pluggable DB engines**: `DbType` admite `mysql | mariadb | postgres` pero
  la UI no ofrece Postgres en producción (no aparece en tests ni en
  `KNOWN_ISSUES.md`; sería DEFERRED).

## Fuentes primarias

- `src-tauri/src/docker.rs` (orquestación completa, constantes, ensure_*).
- `src-tauri/src/nginx.rs` (vhost, tuning).
- `src-tauri/src/php.rs` (imagen, `IMAGE_REV`, `wp_cli_phar_path`).
- `src-tauri/src/config.rs` (`DbType`, `Endpoint`, `SiteConfig::container_name`).
- `src-tauri/src/netcheck.rs` (`PortStatus`, `pick_alt_port`, `holder_name`).
- `src-tauri/src/ssl.rs` (`mkcert`).
- `src-tauri/src/domain.rs` (dnsmasq wildcard).
- `src-tauri/src/migrate.rs` (`import_dump` excepción).
- `src-tauri/src/system.rs` (`SystemStatus`).
- `docker/php/Dockerfile`, `docker/php/entrypoint.sh`, `docker/adminer/autologin.php`,
  `docker/mu-plugins/panel-mailpit.php`, `docker/mu-plugins/panel-autologin.php`.
- `src-tauri/capabilities/default.json` (capability de `core:event` para `op-log`/`log:*`/`sites-changed`).
- `docs/CHANGELOG.md`, `docs/ARCHITECTURE.md`.
