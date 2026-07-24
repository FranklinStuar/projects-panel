# Runbook de diagnóstico y mantenimiento

Este runbook agrupa las verificaciones operativas que se hacen "cuando algo va mal": puertos, DNS, SSL, eventos, contenedores zombies, DB legada, dump al detener, ACL de Tauri, wrappers, versión de Phase, etc. También cubre el mantenimiento regular: rotación de dumps, limpieza del log de volcados, exclusiones de snapshot, reparación del endpoint y comandos administrativos.

## Matriz por síntoma

| Síntoma | Dónde mirar | Comando | Causa típica |
|---|---|---|---|
| Sitio no carga / 502 | vhost, nginx, php-fpm | `docker logs panel-nginx`; `wordpress-panel-cli logs nginx` | vhost huérfano (container `wp-{id}` caído); nginx reload falló |
| 404 en assets | location nginx, fastcgi | `wordpress-panel-cli logs nginx -n 100` | `try_files` mal armado o root path mal |
| 413 al subir theme | php.ini, nginx | revisar tab "Info" → "Tope de subida" y `nginx::ensure_tuning` | `nginx` con `client_max_body_size` por defecto o php.ini no actualizado |
| DB no conecta | `wait_db_ready`, container DB | `docker logs panel-mysql-80` | motor no listo; engine crashed |
| DB se perdió tras apagón | datadir | `ls ~/.config/wordpress-panel/db-data/panel-mysql-80/var/lib/mysql` | bind no durable; volumen anónimo huérfano |
| `op-log` no se ve en consola | capability Tauri | `cat src-tauri/capabilities/default.json` | falta `core:event:default` |
| `wp` no detecta el proyecto | wrapper | `cat ~/.local/bin/wp`; `wordpress-panel-cli detect-project "$PWD"` | wrapper desactualizado, sin `--user www-data` |
| CLI/MCP no responde | D-Bus | `gdbus introspect --session --dest com.goldmediatech.WordpressPanel --object-path /com/goldmediatech/WordpressPanel` | panel cerrado o D-Bus no disponible |
| Endpoint en puerto alterno | panel.json | `cat ~/.config/wordpress-panel/panel.json`; `/settings → Reasignar` | coexistencia con LocalWP o servicio en 80/443 |
| Sitio no abre por dominio | dnsmasq | `getent ahostsv4 panel-probe.test`; `systemctl status NetworkManager` | dnsmasq no resuelve (post-upgrade, post-reinicio) |
| PHP 413 antes de `upload_max_filesize` | nginx | `cat ~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf` | tuning global ausente |
| Imposible abrir wp-admin con un click | mu-plugin | tab "Plugins/Themes" → "Reparar auto-login" | `oneClickAdmin` no activado o mu-plugin ausente |
| Grupo de proyectos "perdido" tras upgrade | groups.json | `cat ~/.config/wordpress-panel/groups.json` | reescritura manual; usar `set_site_group` |
| Docker no se conecta | socket | `docker info`, `ls -la /var/run/docker.sock` | el usuario no está en el grupo `docker` |
| Auto-dump no se genera | engine DB | `docker logs panel-mysql-80 --tail 50`; `docker exec panel-mysql-80 mysql -uroot -ppanel -e "SHOW GLOBAL STATUS LIKE 'Innodb_rows%'"` | motor caído; watch no enganchado |
| VPS no detecta proyectos tras formatear | config | `ls ~/panel-wp/*/config.json` | copia sin `config.json`; usar **Importar proyecto** |
| `OpConsole` se queda vacía al migrar | capability | ver `02-desarrollo-pruebas-build-y-empaquetado.md` §3 | reproduzco el fix de capability de `docs/CHANGELOG.md::Fix — consola vacía` |
| Plasmoid no lista proyectos | qdbus6 | `qdbus6 com.goldmediatech.WordpressPanel /com/goldmediatech/WordpressPanel com.goldmediatech.WordpressPanel.Manager.GetRunningSites` | D-Bus o binario del plasmoid ausente |

## 1. Precondiciones universales

- `panel-net` existe.
- El panel está abierto (D-Bus vivo).
- `docker` responde sin sudo.
- `~/.local/bin` está en `PATH`.
- `mkcert -CAROOT/rootCA.pem` existe.

## 2. Puertos del host y del endpoint

El panel siempre elige puertos en el rango 8080+ para coexistir con LocalWP u otros servicios en 80/443 (`docker::autoselect_endpoint`):

- `netcheck::port_status` lee `/proc/net/tcp{,6}` y devuelve `Free`/`Wildcard`/`Specific`.
- Si 80/443 están en `Wildcard` (LocalWP en `0.0.0.0:80`), `autoselect_endpoint` elige el primer par libre desde 8080/8443.
- El endpoint se persiste en `~/.config/wordpress-panel/panel.json` (`config::save_endpoint`); al abrir el panel se reusa, no se vuelve a autodetectar.
- `reset_endpoint` (botón en **Configuración**) lo borra; el siguiente arranque elige de nuevo.

### Diagnóstico

```bash
# Endpoint persistido
cat ~/.config/wordpress-panel/panel.json
# Ver el panel abierto
gdbus introspect --session \
  --dest com.goldmediatech.WordpressPanel \
  --object-path /com/goldmediatech/WordpressPanel >/dev/null \
  && echo 'D-Bus OK'

# Ver qué proceso tiene el puerto 80/443 (best-effort, /proc/<pid>/fd)
ss -ltnp 'sport = :80 or sport = :443 or sport = :8080 or sport = :8443'
```

Si el panel marca `preflight_endpoint` con error "el puerto 8080 ya está ocupado (lo usa: localwp)" o similar, la UI lo muestra. Decisión:

- **No borres `panel.json` "a ciegas"**: perderás la asignación estable. Primero apaga todos los proyectos (`Apagar todo` o `StopAll`), pulsa **Reasignar puerto** en **Configuración**, y vuelve a encender.
- Si el ocupante es LocalWP y quieres coexistir, el panel ya está en puertos altos. Si quieres portar a 80/443, apaga LocalWP primero (o configura LocalWP en otro puerto) y reasigna.

### Cambio esperado

- `docker ps` muestra `panel-nginx` con los puertos del `panel.json`.
- `curl -I http://127.0.0.1:8080/ | head -1` (o `:8443` con SSL) devuelve `301` (o `200` si no hay SSL) o `502` si no hay proyectos encendidos.

### Abortar y recuperar

- Apagar todos los proyectos antes de reasignar para no perder acceso a sitios ya configurados.
- Si reasignaste y la URL del sitio cambió, enciende cada proyecto y vuelve a regenerar SSL. Los sitios en `siteurl`/`home` apuntan al puerto persistido en la WP-config; si reasignaste, el navegador puede no llegar. Regenera el cert y verifica.

## 3. DNS wildcard `*.test`

`domain::wildcard_active` consulta `panel-probe.test`. Si devuelve `false`, `dnsmasq` no resuelve. Causas típicas:

- `NetworkManager` no tiene `dns=dnsmasq` activado (`/etc/NetworkManager/conf.d/dns.conf`).
- El snippet `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` no está o tiene IP incorrecta.
- El sistema no usa `NetworkManager` (usa `systemd-resolved` u otro): este panel asume NetworkManager.

### Diagnóstico

```bash
getent ahostsv4 panel-probe.test
# 127.0.0.1 indica OK; NXDOMAIN o silencio indica dnsmasq inactivo.

# Reconfigurar manualmente si first-run.sh falló
cat /etc/NetworkManager/dnsmasq.d/wordpress-panel.conf
# Esperado: address=/test/127.0.0.1

systemctl status NetworkManager
systemctl reload NetworkManager
```

`docker::ensure_endpoint_dns` instala el snippet con `pkexec` si la IP loopback del endpoint no es la default (`127.0.0.1`). Requiere contraseña en el diálogo gráfico.

### Cambio esperado

- `getent ahostsv4 panel-probe.test` devuelve `127.0.0.1`.
- Abrir `https://demo.test` en el navegador llega al sitio.

### Abortar y recuperar

- Revertir manualmente el snippet o el `dns=dnsmasq` y reiniciar NetworkManager.
- Si el sistema usa otro resolvedor, documenta: este runbook asume NetworkManager; otros casos requieren reescribir `domain.rs::ensure_wildcard`.

## 4. SSL con mkcert

`ssl::generate` ejecuta `mkcert -cert-file … -key-file … dominio.test`. La CA local (`mkcert -install`) se hace una vez con `scripts/first-run.sh::paso 3`. Si la CA no está, los navegadores muestran aviso.

### Diagnóstico

```bash
mkcert -CAROOT
test -f "$(mkcert -CAROOT)/rootCA.pem" && echo 'CA OK'
test -f ~/panel-wp/mi-sitio/ssl/cert.pem && echo 'cert OK'

# Si el panel marcó un proyecto con SSL pero la navegación muestra error:
docker exec panel-nginx ls /srv/projects/mi-sitio/ssl/
docker exec panel-nginx cat /etc/nginx/conf.d/mi-sitio.test.conf | head -20
```

### Cambio esperado

- El navegador muestra el candado verde en `https://{slug}.test` (mkcert es local-CA confiable).
- `openssl s_client -connect 127.0.0.1:8443 -servername demo.test` muestra cadena válida (con `-CAfile "$(mkcert -CAROOT)/rootCA.pem"`).

### Abortar y recuperar

- Si la CA no está: `mkcert -install` con sudo (instala en el almacén del sistema y en NSS para Firefox).
- Si `ssl::generate` falla tras importar el proyecto: ejecutar **Regenerar SSL** en el menú "···" del proyecto.

## 5. nginx: vhosts huérfanos, reload, autocura

`docker::reload_nginx` ejecuta `nginx -s reload` dentro de `panel-nginx`. Si el container está zombie (setns/nsexec falla), `ensure_nginx` lo recrea (`docs/CHANGELOG.md::Fix — panel-nginx zombie tras apagón sucio`).

`docker::ensure_nginx` también llama `prune_orphan_vhosts` antes de bindear: borra los `{id}.conf` cuyo `wp-{id}` no está corriendo. Esto evita el error "host not found in upstream" al arrancar nginx.

El comando `repair_nginx` (en `lib.rs::repair_nginx` y expuesto como botón en **Configuración → Mantenimiento**) ejecuta manualmente la poda + recrea el container.

### Diagnóstico

```bash
# Estado de nginx
docker ps -a --format '{{.Names}}\t{{.Status}}' | grep -E '^(wp-|panel-)'

# Logs de nginx
wordpress-panel-cli logs nginx -n 100

# Vhosts actuales
ls ~/.config/wordpress-panel/nginx/conf.d/
```

Si ves un `{id}.conf` cuyo `wp-{id}` no existe o no está corriendo, es un huérfano: pulsa **Reparar nginx** o enciende el proyecto. El panel se autodefiende al reiniciar nginx.

### Cambio esperado

- Tras **Reparar nginx**: `docker ps` muestra `panel-nginx`; `docker logs panel-nginx --tail 50` no muestra "host not found in upstream".
- Los proyectos encendidos cargan sus vhosts; los apagados no aparecen en `curl http://127.0.0.1:8080`.

### Abortar y recuperar

- Si nginx se recrea pero un proyecto sigue caído, enciéndelo de nuevo; el `docker::start_site` regenera el vhost.
- Si un huérfano persistió tras reparación, el archivo en `conf.d/` se quedó: edítalo a mano o reescribe con `nginx::write_vhost(site)` desde la consola de Rust (`cargo test` con un test que llame a `write_vhost`).

## 6. PHP y uploads (413)

`nginx::ensure_tuning` escribe `00-panel-tuning.conf` con `server_names_hash_bucket_size 128` y `client_max_body_size 0`. El primero es necesario porque los worktrees con slugs largos desbordan el bucket por defecto; el segundo significa "nginx no pone límite, el límite lo pone PHP".

`php::upload_max_filesize` y `post_max_size` por defecto en `docker/php.ini.tmpl` son 64M. El comando `set_php_upload_limit` (`lib.rs::set_php_upload_limit`, CLI `php upload <MB>`, MCP `set_php_upload_limit`, D-Bus `SetUploadLimit`) sobreescribe el `php.ini` del proyecto y recarga `php-fpm` en caliente (`kill -USR2 1`).

### Diagnóstico

```bash
# Tuning global
cat ~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf
# Esperado: server_names_hash_bucket_size 128; client_max_body_size 0

# PHP del proyecto (proyecto encendido)
docker exec wp-{id} cat /usr/local/etc/php/conf.d/zz-project.ini
# upload_max_filesize y post_max_size del proyecto, con la línea de override
```

### Cambio esperado

- Una subida de 80 MB funciona (con `set_php_upload_limit 96`).
- nginx devuelve 413 solo si `php-fpm` lo rechaza; en la práctica, el límite PHP manda.

### Abortar y recuperar

- Si el tuning global no existe, ejecuta **Configuración → Mantenimiento → "Aplicar php.ini a todos"** (no restaura el tuning, pero `set_php_upload_limit` reescribe el `php.ini`).
- Si nginx devuelve 413 antes de PHP, revisa `client_max_body_size 0` en el tuning.

## 7. Eventos Tauri y la consola de progreso vacía

`OpConsole.svelte` escucha `op-log` con `listen()`. El listener está gateado por la capability `core:event:default` en `src-tauri/capabilities/default.json`. Sin esa capability, `listen` queda bloqueado por el ACL y la consola sale vacía aunque el backend emita (`docs/CHANGELOG.md::Fix — consola de progreso vacía`).

### Diagnóstico

```bash
cat src-tauri/capabilities/default.json
# Esperado: permissions: ["core:default", "core:event:default"], windows: ["main"]
```

La capability se autodescubre; no hace falta tocar `tauri.conf.json`.

### Cambio esperado

- Una migración, import o deploy muestra líneas en vivo en `OpConsole`.
- Los eventos `op-log` aparecen en la consola del navegador con prefijo `[mock-ipc] activo` (en modo mock) o vía el canal nativo en la app real.

### Abortar y recuperar

- Si la capability falta tras una edición manual: reescribe el archivo (Tauri autodescubre) o añade `tauri::generate_handler!` el comando correspondiente.
- Si el listener se engancha en `onMount` (`OpConsole.svelte` actual): abrir la consola con un retardo puede perder líneas tempranas; en la versión actual esto está corregido.

## 8. DB: datadir durable, contenedores legados, post-apagón

`docker::ensure_db` bindea el datadir del motor a `~/.config/wordpress-panel/db-data/{container}/`. Containers legados sin bind (creados antes del bind o por un volumen anónimo de la imagen) se migran automáticamente: `db_has_volume` exige `source == host_dir` para considerarlo durable; si no, `migrate_db_to_volume` copia el datadir de la capa de escritura con `docker cp` y recrea con el bind.

`autodump::watch` engancha en `start_site` y vigila cada 20 s. Si la DB tuvo escrituras (`Innodb_rows_*`), vuelca y compara el hash. La línea base se siembra desde el último dump en disco.

### Diagnóstico

```bash
# Bind durable
docker inspect panel-mysql-80 --format '{{json .Mounts}}' | jq '.[] | select(.Destination=="/var/lib/mysql") | .Source'
# Esperado: /home/<user>/.config/wordpress-panel/db-data/panel-mysql-80

# Auto-dump activo
ls -la ~/panel-wp/mi-sitio/app/sql/

# Estado del watcher
docker exec panel-mysql-80 mysql -uroot -ppanel -e "SHOW GLOBAL STATUS LIKE 'Innodb_rows%'"
```

Si `db-data/panel-mysql-80/var/lib/mysql/` está vacío tras un `ensure_db`, el datadir del host no se bindeó. Causa típica: container preexistente con `Mounts` anónimos. Solución: borrar el container (`docker rm -f panel-mysql-80`), encender cualquier proyecto que use MySQL 8.0; el panel lo recrea con el bind.

### Cambio esperado

- `db-data/panel-mysql-80/var/lib/mysql/` poblado (carpetas `mysql/`, `sys/`, etc.).
- `Innodb_rows_inserted + updated + deleted` varía con la actividad.
- `app/sql/db-*.sql` aparece tras actividad real.

### Abortar y recuperar

- Si la DB está vacía tras un reinicio: el bind no se aplicó. Ver `migrate_db_to_volume` (`docker.rs::244`).
- Si el auto-dump no se genera: comprueba que el watcher esté enganchado (al `start_site`). Tras `pnpm tauri dev`, el `setup` arranca watchers para los `wp-{id}` ya activos.

## 9. WP-CLI: timeout, root, plugin colgante

`wpcli::run` ejecuta `wp` con `--user www-data` y un timeout de 120 s. WP-CLI arranca WordPress entero, así que un mu-plugin que haga una llamada HTTP al cargar (p. ej. update-check de UpdraftPlus) puede colgarlo. El timeout evita que la migración se quede colgada.

### Diagnóstico

```bash
# Desde el wrapper
wp cli info
wp option get home --format=json

# Si un comando cuelga, prueba con skip-plugins/skip-themes
wp --skip-plugins --skip-themes option get home
```

### Cambio esperado

- Los comandos devuelven rápido.
- `wp search-replace` puede tardar (no hay timeout razonable).

### Abortar y recuperar

- `Ctrl+C` mata el wrapper; el container sigue encendido. No hay daño.
- Si un comando colgado bloquea una migración: ejecuta `fix_site_url` con `--skip-plugins --skip-themes` (ya lo hace `migrate::fix_site_url`).

## 10. Auto-login: mu-plugin ausente, selector de usuario

`autologin::open_admin` escribe un transient de WP (60 s, un solo uso). El mu-plugin `panel-autologin.php` lo valida y aplica `wp_set_auth_cookie`. Si el mu-plugin no está (proyectos importados de LocalWP antes del fix de `docs/CHANGELOG.md::Fix — Auto-login en proyectos importados de LocalWP`), el botón "Abrir admin" no auto-loguea.

`repair_autologin` (`lib.rs::repair_autologin`) activa `oneClickAdmin` y reinyecta los mu-plugins. No requiere proyecto encendido.

El selector de usuario (lista de `wp user list --format=json`) solo aparece si `oneClickAdmin=true` y el proyecto está encendido. Persiste la selección en `localStorage` (`wp-panel:autologin:<id>`).

### Diagnóstico

```bash
docker exec wp-{id} ls /var/www/html/wp-content/mu-plugins/
# Esperado: panel-mailpit.php y panel-autologin.php (si oneClickAdmin)
```

### Cambio esperado

- Click en "Abrir admin" entra logueado.
- La redirección va a `/wp-admin/` si el usuario tiene `manage_options`, o a `/` si no.

### Abortar y recuperar

- Si el mu-plugin no está: pulsa **Reparar auto-login** en el tab "Plugins/Themes".
- Si el selector de usuario no carga: el proyecto está apagado o `oneClickAdmin=false`.

## 11. Wrapper WP-CLI: PATH, version, root

`wp` se instala en `~/.local/bin/wp`. Su contenido es `scripts/wp-wrapper.sh`: detecta el proyecto por CWD, ejecuta `docker exec -i --user www-data "wp-${PROJECT_ID}" php /usr/local/bin/wp --path=/var/www/html "$@"`.

### Diagnóstico

```bash
ls -la ~/.local/bin/wp ~/.local/bin/wordpress-panel-cli
cat ~/.local/bin/wp

# Si wp no detecta el proyecto
cd ~/panel-wp/mi-sitio
wordpress-panel-cli detect-project "$PWD"
# Esperado: id del proyecto
```

### Cambio esperado

- `which wp` apunta a `~/.local/bin/wp`.
- `wp` desde la carpeta del proyecto detecta el id.

### Abortar y recuperar

- Si el wrapper se quedó antiguo (pre-fix `wp` como root): `cli::install_cli_wrapper` lo reinstala (botón **Configuración → Servicios → "Solo instalar wrapper `wp`"**).
- Si el PATH no incluye `~/.local/bin`: añádelo a `~/.bashrc` o equivalente.

## 12. Endpoint y panel.json

`config::panel.json` persiste el `Endpoint` (IP loopback + httpPort + httpsPort). Se elige una vez (`docker::autoselect_endpoint`) y se mantiene estable porque WordPress guarda el `siteurl` con el puerto.

### Diagnóstico

```bash
cat ~/.config/wordpress-panel/panel.json | jq .
```

### Cambio esperado

- El endpoint es razonable: 127.0.0.1:80/443 si libre, 127.0.0.1:8080/8443 si LocalWP u otro ocupa 80/443.

### Abortar y recuperar

- **Reasignar** en **Configuración** llama `reset_endpoint` (borra el archivo). El próximo `ensure_nginx` re-elige. **Importante**: hacerlo cambia el puerto; los sitios ya creados siguen apuntando al antiguo (su `wp-config` y `siteurl`/DB no se actualizan). Si reasignaste, enciende cada proyecto y considera regenerar SSL.
- `clear_endpoint` no es transaccional. No lo borres manualmente en medio de una operación.

## 13. Grupos persistentes

`config::groups.json` mantiene el orden y la lista durable de grupos. `set_site_group` (drag&drop) lo actualiza en `groups.json` al asignar a un grupo nuevo (`groups::create`).

### Diagnóstico

```bash
cat ~/.config/wordpress-panel/groups.json
```

### Cambio esperado

- `groups` es un array con los nombres en orden.
- `reorder` sobrescribe el array completo.

### Abortar y recuperar

- `groups::create` es idempotente. Si reordenas y eliminas grupos accidentalmente, los proyectos se quedan sin `site.group`. Reasigna desde la UI.

## 14. Log de volcados y limpieza

`dumplog::read_all` lista las entradas (más nuevas primero). `clean(before?, dbName?)` borra por fecha y/o nombre de DB; sin filtros borra todo. La limpieza solo toca el JSONL; los `.sql` quedan en `app/sql/`.

### Diagnóstico

```bash
tail -f ~/.config/wordpress-panel/dump-log.jsonl
# Total de líneas:
wc -l ~/.config/wordpress-panel/dump-log.jsonl
```

### Cambio esperado

- Las entradas tienen `timestamp` (ISO Z), `siteId`, `siteName`, `dbName`, `file`, `bytes`, `source ∈ {auto, stop, manual}`.
- Limpieza reduce la lista y reescribe el archivo.

### Abortar y recuperar

- Si borras accidentalmente todo: restaura desde un backup del JSONL; las entradas no se regeneran automáticamente.
- `rotate_dumps` (en `backup::rotate_dumps(site, 3)`) borra los `db-*.sql` viejos, no así el log.

## 15. Snapshot, clone, worktree (chequeos rápidos)

- **Snapshot no aparece en la lista**: `meta.json` no se escribió o el directorio está corrupto. `ls ~/panel-wp/{slug}/snapshots/`.
- **Clone sin uploads del padre**: revisa `nginx::render_vhost` cuando `clone_of` está poblado; la location `^~ /wp-content/uploads/` con `try_files` debe estar presente.
- **Worktree con assets rotos**: la location regex con `alias` al `wt/{basename}/$1` debe ganarle al match estático genérico. `nginx.rs::render_vhost` la inserta antes de la location genérica.

## 16. Phase 5 (IA) — estado actual

`src-tauri/src/agent.rs` no existe. Las herramientas de Fase 5 (chat contextual, providers, keyring) están en `PLAN.md::Fase 5` pero no implementadas. **No inventes endpoints de IA**; los runbooks previos no los cubren.

## 17. Macros de issues conocidos

- `docs/KNOWN_ISSUES.md` enumera tres limitaciones activas:
  1. **Import LocalWP requiere dump en disco** (no extrae del MySQL de LocalWP). Mitigación: exportar la DB desde LocalWP antes.
  2. **Reconstructed (sin sidecar) es best-effort**: versiones PHP/DB por defecto. Revisa antes de "Migrar y encender".
  3. **Barra de título no respeta config de KDE**: la posición de los botones (izq/der) sigue la del sistema, no la del usuario. Diferido hasta cerrar todas las fases.

## 18. Mantenimiento regular

### Diario

- `docker ps --format '{{.Names}}\t{{.Status}}' | grep -E '^(wp-|panel-)'`: con cero proyectos encendidos, no debe haber `wp-*` ni `panel-*` corriendo.
- `du -sh ~/panel-wp/ ~/.config/wordpress-panel/`: vigilar crecimiento.

### Semanal

- Revisar `dump-log.jsonl`: que las entradas tengan fuentes razonables. Si casi todas son `auto`, los proyectos están activos (normal).
- Verificar que `~/.config/wordpress-panel/db-data/{container}/` está poblado.
- Revisar `/dumps` y, si el log está largo, limpiar con `Borrar por fecha` o `Borrar por base`.

### Mensual

- `bash scripts/first-run.sh` es idempotente: reejecutar asegura dnsmasq/mkcert/NetworkManager.
- `cargo update && pnpm update`: solo tras validar el comportamiento.
- `bash scripts/package-plasmoid.sh && kpackagetool6 --type Plasma/Applet --upgrade dist/wordpress-panel.plasmoid`.

## 19. Criterio de salida

- El síntoma reportado ya no ocurre o tiene una causa documentada con mitigación.
- Si toca un runbook operativo, el flujo siguiente queda habilitado.
- Si toca código o documentación, regenera los runbooks impactados.

Volver al índice principal de `docs/resume/operacion/`.
