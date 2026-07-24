# Docker, red y servicios

## Topología

Todo runtime comparte el bridge Docker `panel-net`.

```text
Host loopback
  127.0.0.1:8080+ ─┐
  127.0.0.1:8443+ ─┴─ panel-nginx:80/443
                           │ FastCGI
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
      wp-{idA}:9000     wp-{idB}:9000     wp-{idN}:9000
         │                 │
         ├──── panel-mysql-80:3306 / panel-mariadb-* / postgres-*
         ├──── panel-mailpit:1025
         └──── panel-minio:9000 (si se activa)

UIs loopback: Mailpit :8025, MinIO :9100/:9101, Adminer :8088
```

Los containers PHP **no publican puertos al host**. Nginx es la única entrada HTTP/HTTPS de sitios; UIs auxiliares tienen bindings explícitos solo en `127.0.0.1`.

## Endpoint

`DockerManager::autoselect_endpoint` cede 80/443 y siempre busca el primer par libre desde 8080/8443. Se persiste en `panel.json`; `ensure_nginx` hace preflight y, si el endpoint guardado dejó de ser bindeable, selecciona otro par alto. El comentario histórico de `Endpoint` sobre modo normal 80/443 no describe la selección actual.

Nginx monta `conf.d` read-only y `~/panel-wp` como `/srv/projects:ro`. Los vhosts enrutan por nombre a `wp-{id}:9000`, sirven estáticos y certificados desde el árbol montado. `reload_nginx` intenta `nginx -s reload`; ante un container zombie lo elimina y recrea.

## Servicios y ciclo de vida

| Recurso | Multiplicidad | Demanda | Persistencia |
|---|---:|---|---|
| `wp-{id}` | uno/proyecto | proyecto activo | código por bind |
| `panel-nginx` | uno | algún proyecto activo | vhosts host |
| DB `panel-*-{ver}` | uno/motor+versión | activo compatible | bind `db-data/` |
| `panel-mailpit` | uno | algún activo | efímero |
| `panel-minio` | uno | activo con `minio` | bind `minio-data/` |
| `panel-adminer` | uno | apertura de visor | sin datos propios |

`DockerManager::ensure_*` reutiliza containers parados cuando es seguro. DB espera disponibilidad TCP hasta 60 s, evitando la fase MySQL `--skip-networking`. `teardown_unused_shared` calcula PHP activos reales: apaga la DB ya no compartida, MinIO sin consumidores y nginx/Mailpit/Adminer cuando no queda ningún activo.

## PHP e imágenes

`php::ensure_php_image` produce `panel-php:{version}-r3` desde `docker/php/Dockerfile`. El entrypoint remapea `www-data` a UID/GID host recibidos como `PUID/PGID`. Se montan:

- `app/public` en `/var/www/html`;
- `php.ini` del proyecto read-only;
- un `wp-cli.phar` global read-only.

Al cambiar el tag/revisión, `start_site` elimina el container con imagen antigua y lo recrea. Alpine se usa donde existe; las imágenes oficiales MySQL/MariaDB no son Alpine.

## Worktree-projects

Para un worktree, `DockerManager::create_php_container` monta el `public` del padre y luego sobrepone `wt/{basename}` en el destino del repo y el `wp-config.php` propio. Nginx también debe resolver rutas estáticas con conocimiento del padre. Es un aislamiento por montaje y URL, no una copia completa.

## Datos y excepciones al API

Bollard maneja red, pulls, containers, exec y logs. Tres excepciones usan el CLI Docker: build PHP; import de dumps grandes por stdin; migración única del datadir con `docker cp`. Estas excepciones responden a limitaciones prácticas documentadas, no a un segundo orquestador general.

## Invariantes

- Nunca recrear DB sin bind durable.
- Nunca publicar PHP al host.
- Nunca dejar compartidos activos sin consumidores.
- Nombres `wp-`/`panel-` permiten detectar recursos gestionados.
- Credenciales y DNS se consideran entorno local, no producción.

## Deuda observable

Las imágenes de servicios usan tags flotantes como `latest` en Mailpit/MinIO. Los puertos de UIs auxiliares son fijos y pueden colisionar. `ensure_mailpit`/`ensure_minio` se invocan best-effort desde el arranque, por lo que el sitio puede iniciar sin ellos. El plugin WordPress para usar MinIO como S3 sigue diferido.

## Fuentes primarias

- `src-tauri/src/docker.rs::DockerManager::ensure_nginx`, `autoselect_endpoint`
- `src-tauri/src/docker.rs::DockerManager::start_site`, `create_php_container`, `teardown_unused_shared`
- `src-tauri/src/docker.rs::ensure_db`, `db_data_dir`, `host_port_map`
- `src-tauri/src/php.rs::ensure_php_image`, `wp_cli_phar_path`
- `src-tauri/src/nginx.rs::render_vhost`
- `docker/php/Dockerfile`, `docker/php/entrypoint.sh`
