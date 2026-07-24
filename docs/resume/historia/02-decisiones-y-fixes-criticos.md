# Decisiones y fixes críticos

Este documento recoge las decisiones de arquitectura y los fixes críticos que el proyecto documenta en prosa, contrastados con el código actual. Es un índice razonado de "por qué pasó lo que pasó" para responder dudas tipo "¿por qué se hace así?" sin tener que reconstruir la historia desde el changelog.

Fuentes: `docs/CHANGELOG.md`, `docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/EXTENDING.md`, `docs/KNOWN_ISSUES.md`, `src-tauri/src/**`, `scripts/`, `docker/`.

## Decisiones de arquitectura

### D1. Solo php-fpm por proyecto, sin nginx propio

`docs/PLAN.md::Container por Proyecto` deja explícito: el container del proyecto es **solo php-fpm**. El reverse-proxy es `panel-nginx`, compartido y on-demand. Razones:

- `wordpress:*` ya trae el core de WP dentro y no respeta la versión PHP del usuario; usar `php-fpm-alpine` puro da control total y una imagen base de ~30 MB.
- Sin nginx propio, no hay un `docker-compose` por proyecto. El panel gestiona todo vía bollard.
- **No publica puertos al host**: solo `panel-nginx` le habla por `panel-net`. Asignar puertos por proyecto del plan original queda obsoleto: el endpoint del panel es global y se persist en `panel.json`.

Citado: `docs/PLAN.md::Arquitectura Docker` y `docs/ARCHITECTURE.md::Modelo de containers y recursos`.

### D2. Red única `panel-net`

Todos los containers del panel (compartidos y por proyecto) viven en un único bridge `panel-net`. Resolución por nombre:

- `panel-nginx` → `wp-{id}:9000` (FastCGI).
- `wp-{id}` → `panel-mysql-{ver}:3306`.

Sin esta red, ningún container puede comunicarse. Prerequisito de todo. Ver `docker::NETWORK`, `docker::ensure_network`.

### D3. Servicios compartidos, on-demand y con teardown

El principio rector (también en `docs/CLAUDE.md`) es:

1. Nada corre si no hace falta.
2. Compartir antes que duplicar.
3. Imágenes mínimas (alpine).

`docker::teardown_unused_shared` apaga `panel-nginx`, `panel-mailpit`, `panel-adminer` y el motor DB compartido cuando ningún proyecto activo los usa. Resultado: con cero proyectos encendidos, `docker ps` no muestra ningún `wp-*` ni `panel-*` (y `lib.rs::stop_all_sites` orquesta el apagado total).

### D4. Endpoint global y puertos altos por defecto

`config::Endpoint` (loopbackIp/httpPort/httpsPort) vive en `panel.json` y se elige **una vez** (autodetección). El plan original preveía tres ramas (loopback alterna, IP loopback alterna, fallback de puerto), pero el código actual `docker::autoselect_endpoint` siempre cede 80/443 y elige el primer par libre desde 8080/8443. Razón: coexistencia con LocalWP (que escucha en `0.0.0.0:80`) sin necesidad de retocar la IP del wildcard dnsmasq.

El puerto se mantiene estable porque WordPress guarda `siteurl` con puerto: cambiarlo después rompería los sitios ya instalados. `config::site_url` aplica el puerto solo si no es estándar.

> Convergencia: `docs/PLAN.md::Filosofía` menciona las tres ramas; `docker::autoselect_endpoint` solo implementa la tercera. La rama de IP loopback alterna existe en `netcheck::pick_loopback_ip` pero no se usa.

### D5. UID/GID host↔www-data en el container php

`www-data` en alpine es uid 82 ≠ host (1000). Bind-mounts de `app/public` rompen WordPress (no se puede escribir uploads/plugins) y los clones con `gh` heredan permisos incorrectos. Solución: el entrypoint (`docker/php/entrypoint.sh`) ajusta `usermod`/`groupmod` a `PUID`/`PGID` del host (`docker::host_uid_gid`).

Sin este paso, la regla "bind-mount con permisos del host" del principio rector no se cumple. Requisito de Fase 1, no optimización tardía.

### D6. Los mu-plugins del panel se inyectan, no se asume

`wordpress::sync_mu_plugins` reescribe `panel-mailpit.php` y `panel-autologin.php` en cada `create_site`, `migrate_site` y al re-importar. Razón: un proyecto importado de LocalWP no los trae (lo cubre `docs/CHANGELOG.md::Fix — Auto-login en proyectos importados de LocalWP`); una copia entre sistemas puede traerlos desfasados. `repair_autologin` lo invoca para proyectos antiguos que no tengan el mu-plugin.

### D7. WP-CLI corre como `www-data` siempre

`wpcli::run` usa `exec_as(c, ..., Some("www-data"))`. Razones:

- WP-CLI rechaza root (`YIKES: ...`).
- Los archivos quedan con el dueño del host vía el remapeo de uid del entrypoint.

`scripts/wp-wrapper.sh` también usa `--user www-data` (después del fix de `docs/CHANGELOG.md::Fix — el wrapper wp de terminal corría como root`).

### D8. DB legada → datadir durable vía `docker cp`

Los containers DB creados antes de la Fase 4 no tenían bind: los datos estaban en la capa de escritura o en un volumen anónimo declarado por la imagen (`VOLUME /var/lib/mysql`). El volumen sobrevive al reinicio del container pero **se queda huérfano al recrearlo** (p. ej. al subir `IMAGE_REV`); el resultado es la pérdida que motivó el feature.

`docker::db_has_volume` exige `source == host_dir` además del destino, para no confundir el bind real con un volumen anónimo. Si no, `migrate_db_to_volume` usa `docker cp` (excepción al "Docker solo vía bollard" documentada en el propio módulo) para copiar el datadir al host, y recrea con el bind. Una sola vez por container; idempotente.

Citado: `docs/CHANGELOG.md::DB durable + auto-dump`.

### D9. Auto-dump con `Innodb_rows_*` + hash del dump

El export-al-detener solo deja un dump fresco cuando el usuario para el proyecto ordenadamente. Si la máquina se apaga de golpe, ese dump nunca se genera. El watcher (`autodump::watch`) hace un gate barato con `SHOW GLOBAL STATUS WHERE Variable_name IN ('Innodb_rows_inserted','Innodb_rows_updated','Innodb_rows_deleted')` para evitar volcar la DB ociosa, y solo cuando la suma de ins/upd/del cambió desde el último sondeo hace `backup::dump_bytes` y compara el hash con el último dump persistido. Si difiere, escribe `db-{stamp}.sql` y rota dejando los 3 más recientes.

Fixes posteriores:
- `db_has_volume` exige `source == host_dir` (no solo el destino).
- `mysqldump --skip-dump-date` para que la línea `Dump completed on <fecha>` no rompa el dedup por hash.
- La línea base se siembra desde disco (`autodump::latest_dump_hash`): una edición con el panel cerrado o al arrancar se detecta en el primer sondeo.

### D10. Conexión única D-Bus ↔ CLI/MCP/UI

El panel expone el servicio `com.goldmediatech.WordpressPanel` por D-Bus de sesión (`dbus.rs::serve`). El CLI (`scripts/wordpress-panel-cli.sh`) y el MCP (`mcp/server.mjs`) son envoltorios finos: cada comando lanza el CLI, que habla con el panel por D-Bus. **No reimplementan lógica** ni guardan estado. La UI no usa D-Bus directamente (usa IPC de Tauri); pero los métodos D-Bus que mutan proyectos (`StartSite`/`StopSite`/`StopAll`/`CreateWorktree`/`RemoveWorktree`/`CreateClone`) emiten el evento `sites-changed` que la UI escucha para recargarse sola (`+page.svelte::listen('sites-changed', () => load())`).

Citado: `docs/ARCHITECTURE.md::D-Bus` y `mcp/README.md`.

### D11. Zbus usa el runtime Tokio

`zbus = { ..., features = ["tokio"] }` en `src-tauri/Cargo.toml`. Sin la feature `tokio`, el executor de zbus corre sobre `async-io` y los handlers que tocan Docker (bollard) panican con "no reactor running" porque bollard espera el runtime tokio del panel. Documentado en el commit `fix(dbus): usar el runtime Tokio en zbus (feature tokio)`.

### D12. WP-CLI timeout 120 s

`wpcli::WPCLI_TIMEOUT = 120 s`. WP-CLI arranca WordPress entero; un mu-plugin que haga una llamada HTTP al cargar (update-check de UpdraftPlus, llamada de licencia) puede colgar el comando. El timeout defensivo se aplica solo al wrapper interno `wpcli::run`; el wrapper de terminal `scripts/wp-wrapper.sh` no lo aplica (es una llamada a `docker exec` directa). Documentado en `docs/CHANGELOG.md::Fix — migración se colgaba importando dumps grandes`.

### D13. Vhost con `try_files` y `^~` para clones

Los clones temporales no copian uploads: el nginx sirve los archivos del `wp-content/uploads` del padre vía `try_files $uri @uploads_base` (`nginx::render_vhost` cuando `site.clone_of.is_some()`). El modificador `^~` da precedencia sobre la location regex genérica de estáticos.

Limitación documentada: cubre lectura web, no lectura por filesystem desde PHP (regenerar thumbnails, p. ej.).

### D14. Tuning global nginx con `client_max_body_size 0`

`nginx::ensure_tuning` escribe `00-panel-tuning.conf` con `server_names_hash_bucket_size 128` (worktrees con slugs largos desbordan el bucket por defecto) y `client_max_body_size 0` (sin límite en nginx; el tope lo pone PHP). Sin esto, nginx corta con 413 los uploads >1M aunque PHP los acepte.

### D15. Eventos Tauri requieren capability

`app.emit`/`listen` usan el plugin `core:event`, que **sí** está gateado por ACL en Tauri 2. `src-tauri/capabilities/default.json` concede `core:default` + `core:event:default` a la ventana `main`. Sin esa capability, `listen('op-log')` queda bloqueado y `OpConsole.svelte` sale vacío. Los tests e2e usan IPC mockeado, así que este caso no lo detectaban.

Citado: `docs/CHANGELOG.md::Fix — consola de progreso vacía`.

## Decisiones de UI

### U1. Master-detail estilo LocalWP

`+page.svelte` y `+layout.svelte` sustituyen las páginas-ruta sueltas por:

- Riel de íconos angosto a la izquierda (Proyectos, Dominios, Servicios, Configuración, CLI, "+" nuevo proyecto, ruta a `/dumps`).
- Lista de proyectos agrupada con `groups.json` + `config.group` fusionados; poder/estado como íconos; **drag&drop** nativo HTML5 para asignar grupo; **grupos plegables** con estado en `localStorage`; sección fija "En ejecución" con los proyectos `running` al inicio.
- Detalle embebido (`ProjectDetail.svelte`) por `selectedId` (no se navega); `ProjectDetail` se reutiliza también en el wrapper `/site/[id]` (deep-link).

Citado: `docs/CHANGELOG.md::Rediseño UI — master-detail estilo LocalWP`.

### U2. Acciones primarias vs. menú "···"

`ProjectDetail` separa la acción primaria (Encender/Detener) de las acciones secundarias (Punto de guardado, Regenerar SSL, Eliminar) en un menú "···". Esto descongestiona la cabecera y deja la acción más usada a un click.

### U3. Consola de progreso modal con ventana de gracia

`OpConsole.svelte` es un modal que escucha `op-log` y muestra los pasos en vivo. El listener se engancha en `onMount` (no al abrir) para no perder las primeras líneas; el botón "Cerrar" está deshabilitado mientras corre. En el borrado de proyecto, una cuenta atrás de 5 s con botón "Cancelar borrado" cubre la operación destructiva.

Citado: `docs/CHANGELOG.md::Borrar proyecto (con opción de conservar la carpeta)`.

### U4. Tema "DevFlow Dark Blue" como paleta única

`DESIGN.md::Tema` (electric blue + deep navy) implementado remapeando la escala `zinc` de Tailwind (`tailwind.config.js`). Los componentes con `dark:bg-zinc-*` heredan el tema sin tocarse. Token `primary` (#4d8eff). `app.css` define `.input` global con fondo navy y texto claro (antes vivían en `<style>` locales y se quedaban en blanco).

## Decisiones sobre el CLI

### C1. Autodetección por CWD

`scripts/wordpress-panel-cli.sh::project_for` lee todos los `~/panel-wp/*/config.json` y matchea por prefijo de path. `resolve_pid` permite identificar el proyecto por nombre o id cuando no se está en su CWD. Sin flag, todos los subcomandos usan el CWD. Sin mecanismo de "proyecto por defecto" global.

### C2. Resolución de proyecto en MCP

`mcp/server.mjs::resolveProject` carga `~/panel-wp/*/config.json` y matchea por id exacto o por subcadena del nombre (case-insensitive). Si hay 0 → error; si hay >1 → error con la lista. `needProject: true` en cada herramienta que toca un proyecto; `runCli` ejecuta con `cwd = resolveProject(args.project).path` para que el CLI detecte el proyecto por CWD.

### C3. ENV override

`PANEL_WP_ROOT` y `WORDPRESS_PANEL_CLI` permiten mover la raíz de proyectos o el binario del CLI sin recompilar. Default: `~/panel-wp` y `~/.local/bin/wordpress-panel-cli` (o el script del repo como fallback).

## Decisiones de mantenimiento y operación

### M1. Export-al-detener no bloquea

`docker::stop_site` ejecuta `backup::export_db` y `rotate_dumps` best-effort: si fallan, el stop continúa igualmente. La razón es que un stop fallido no debe impedir apagar el container. El auto-dump y el log de volcados cubren los huecos entre stops exitosos.

### M2. Import-al-migrar es atómico y reintentable

`migrate::import_dump` se divide en 6 pasos numerados. Si `[5/6]` se cuelga o se aborta, los pasos 1–4 son idempotentes y la DB queda vacía (gracias a `reset_database`). Reintentar la migración reanuda: el import vuelve a empezar de cero (no se puede retomar un dump SQL a mitad de statement, sería unsafe).

### M3. LaOpConsole muestra una línea "viva" durante el import

`progress::log_progress` usa prefijo SOH (``); `OpConsole.svelte` la reescribe en sitio. Formato `12/53 MB ━━━━━──── 1:23`. Evita que el contador inunde la consola si tickea cada 2 s.

### M4. `autodump::watch` siembra desde disco

Una edición hecha con el panel cerrado (o justo al arrancar) se detecta en el primer sondeo, no se absorbe silenciosamente. Esto es importante: si el dump-al-detener no se generó por un crash, la línea base sembrada desde disco permite que el primer sondeo del watcher detecte el cambio y vuelque.

### M5. `mysql -N -B` para queries programáticas

`autodump::write_counter` y `migrate::query_db_size` usan `-N -B` para que la salida sea solo el valor numérico, sin cabecera ni formateo. Esto evita parseos frágiles cuando se mezcla stdout+stderr (mysql avisa por stderr al pasar la contraseña en CLI).

### M6. `tar` con tolerancia a código 1

`snapshot::run` considera el código 1 de `tar` como "avisos no fatales" (file changed as we read it), típico en un WP activo con cache/logs mutando. Código 2+ aborta. La línea de stderr se loguea como `⚠`.

## Decisiones de versionado y release

### R1. `NO_STRIP=1` en `pnpm tauri build`

El `strip` bundleado de linuxdeploy (que Tauri usa para AppImage) no soporta la sección `.relr.dyn` de las libs modernas de Manjaro/Arch. Sin la env var, el build falla al final. Documentado en `chore(deploy): fix AppImage install on Manjaro and add .desktop creation` y en `.claude/commands/deploy.md::/deploy`.

### R2. `WEBKIT_DISABLE_DMABUF_RENDERER=1` en Wayland

tauri dev puede renderizar la ventana en blanco en Wayland; el `.desktop` generado en el flujo de release incluye la env var para evitarlo. Documentado en README, KNOWN_ISSUES y `.claude/commands/deploy.md`.

### R3. Tauri autodescubre `capabilities/*.json`

No hace falta tocar `tauri.conf.json` al añadir capabilities nuevas: Tauri 2 autodescubre el directorio. Por eso `src-tauri/capabilities/default.json` se aplica sin `capabilities: [...]` explícito en el config.

## Fixes críticos (línea de tiempo y causa raíz)

### F1. Conflictos de puerto 80/443 (Fase 4+)

**Síntoma**: Docker fallaba con "failed to bind host port 127.0.0.1:80/tcp: address already in use" al crear el primer proyecto.

**Causa**: LocalWP escucha en `0.0.0.0:80` (wildcard). El kernel rechaza bindear *cualquier* `127.0.0.x:80` mientras exista un listener wildcard en ese puerto.

**Fix**: `netcheck.rs` (lee `/proc/net/tcp{,6}`, decodifica little-endian IPv4, distingue Free/Wildcard/Specific). `autoselect_endpoint` cede 80/443 a LocalWP y elige el primer par libre desde 8080/8443. `preflight_endpoint` da un error legible con `holder_name`.

Citado: `docs/CHANGELOG.md::Fix — Conflicto de puerto 80/443 del host (coexistencia con LocalWP)`.

### F2. Instalación de WordPress fallaba en silencio

**Síntoma**: El proyecto se creaba pero WP no se instalaba, y `create_site` devolvía Ok.

**Causas encadenadas**:
- `docker::exec` tragaba el exit code.
- WP-CLI corría como root (rechaza root con `YIKES`).
- `ensure_db` no esperaba readiness: la imagen oficial de MySQL acepta el socket local antes de abrir el puerto TCP, y `create_database`/`wp config create` corrían en esa ventana.

**Fix**: `inspect_exec` chequea el exit code; `exec_as(user)` para `wpcli::run` y `backup`; `wait_db_ready` con timeout 60 s gateando sobre TCP.

### F3. Migración se colgaba con dumps grandes

**Síntoma**: Migrar un sitio real (p. ej. desde LocalWP) se quedaba clavado tras "Generando certificado SSL" minutos hasta matar la app.

**Causa**: `docker::exec_stdin` con stdin adjunto: el stream de salida de bollard no emite `None` al terminar el proceso, así que el lector quedaba esperando. Dumps chicos (~1 MB) colaban de chiripa; uno de 7 MB colgaba.

**Fix**: `migrate::import_dump` ahora usa `docker exec -i … mysql` (CLI, excepción justificada). Importa 7 MB en ~15 s.

### F4. Consola de progreso vacía

**Síntoma**: La consola (`OpConsole`) salía vacía: solo se veía el ícono verde al terminar, sin líneas de progreso ni el error si fallaba.

**Causa**: El proyecto no tenía ninguna capability de Tauri 2. Los comandos propios no pasan por el ACL pero `listen('op-log')` usa el plugin `core:event`, que sí está gateado. Los tests e2e usan IPC mockeado y no lo detectaban.

**Fix**: `src-tauri/capabilities/default.json` (nuevo) concede `core:default` + `core:event:default` a la ventana `main`. Tauri autodescubre el directorio.

### F5. Import del dump: timeout con rollback, aceleración, barra en vivo

**Síntomas**: dumps grandes quedaban "clavados" sin forma de cancelar, la DB quedaba a medio importar si se mataba la app, y no había señal de avance.

**Causa real de la lentitud**: cada statement hacía fsync y revalidación de índices/FK.

**Fix**:
- **Aceleración**: pragmas de sesión antepuestos (`SET foreign_key_checks=0; unique_checks=0; autocommit=0; … COMMIT;`).
- **Watchdog con rollback + resume**: chunks de 1 MiB; cancela `docker exec` si ni el stdin avanza ni crece la DB durante 3 min (`IMPORT_IDLE_TIMEOUT`); `reset_database` (nuevo) hace `DROP DATABASE` + recrea vacía. Reintentar reanuda.
- **Indicador de vida correcto**: medir solo bytes-por-stdin daba falsos timeouts (el pipe del OS es de ~64 KB; tras el primer chunk `write_all` se bloquea hasta que mysql consume stdin, y mysql consume tan rápido como aplica el SQL; durante un statement grande no fluye ni un byte aunque el import avance). El watchdog usa además el **tamaño real de la DB** (`information_schema.tables WHERE table_schema='{db}'`, vía `query_db_size`).
- **Barra de progreso en sitio**: `progress::log_progress` con prefijo SOH; `OpConsole.svelte` la reescribe en sitio.

### F6. Auto-login no funcionaba en proyectos importados de LocalWP

**Síntoma**: El botón "Abrir admin" no auto-logueaba proyectos traídos de LocalWP.

**Causa**: `localwp::import_site` y `migrate` no inyectaban el mu-plugin `panel-autologin.php` (solo `create_site` lo hacía en el paso 6).

**Fix**: `wordpress::sync_mu_plugins(site)` reinyecta mailpit (siempre) + auto-login (si `oneClickAdmin`). `create_site` ahora lo usa; `migrate` lo llama tras verificar la carpeta. `repair_autologin` (nuevo comando IPC) repara proyectos ya importados.

### F7. Wrapper `wp` de terminal corría como root

**Síntoma**: `wp cli info` funcionaba pero `wp plugin list` no (WP-CLI bootea WP y rechaza root con `YIKES`).

**Causa**: `scripts/wp-wrapper.sh` hacía `docker exec` sin `--user www-data`. El comando in-app `exec_wpcli` sí usaba www-data; el wrapper se quedó sin paridad.

**Fix**: añadido `--user www-data` al `docker exec` del wrapper. El refresco del script se aplica solo al reabrir el panel (auto-instalación idempotente).

### F8. Panel-nginx zombie tras apagón sucio

**Síntoma**: Tras cortar la corriente sin apagar Docker (p. ej. batería agotada), el daemon volvía marcando el container "running" pero con sus namespaces muertos; el exec de `nginx -s reload` fallaba con `setns/nsexec` y no se podía encender ningún proyecto.

**Fix**: `docker::reload_nginx`, si el reload falla, fuerza `remove_container` + `ensure_nginx` (recrear = arranque fresco que relee todo `conf.d`, equivale al reload).

### F9. `git worktree add` con rama ya registrada

**Síntoma**: Un intento anterior fallido a mitad dejaba el repo del padre con el dest "missing but already registered"; `git worktree add` se negaba.

**Fix**: `worktree::add_worktree` ejecuta `git worktree prune` antes de `add` (idempotente, no toca worktrees vivos).

### F10. `git worktree` con `server_names_hash_bucket_size` insuficiente

**Síntoma**: Worktrees con slugs largos (p. ej. `pgnyc-dev-feature-…-test`) provocaban que nginx no arrancara.

**Fix**: `nginx::ensure_tuning` escribe `server_names_hash_bucket_size 128;` en `00-panel-tuning.conf` (el default 64 era insuficiente).

### F11. VSCode abría `app/public` vacío para worktree-projects

**Síntoma**: `open_vscode` para un worktree-project abría el `app/public` del padre, que estaba tapado por el mount del worktree y no mostraba el código.

**Fix**: `github::ensure_workspace` para worktree-projects (`site.worktree_of.is_some()`) apunta el workspace a `wt/{basename}` (donde está el `git worktree`), no a `app/public`.

### F12. Validador de rama de worktree con pegado accidental

**Síntoma**: `git worktree create "git checkout -b feature/x"` fallaba sin pista útil: el nombre de la rama contenía el comando entero y espacios.

**Fix**: `worktree::invalid_branch_reason` rechaza ramas con espacios, `..`, `^~:?*[\]`, que empiecen por `-`. `guess_branch` extrae la última token que parece una rama y la sugiere en el error.

## Convergencias y divergencias entre prosa y código

- `docs/PLAN.md::Fase 1 — Pendiente de Fase 1` lista "WP-CLI wrapper instalado en `~/.local/bin/` + binario `wordpress-panel-cli`" y "Verificación end-to-end de una provisión completa de WordPress real" como pendientes, pero el `docs/CHANGELOG.md` marca la Fase 1 como "en curso" sin reflejar que la instalación de wrappers ocurre en el `setup` de Tauri. La prosa quedó atrás del código.
- `docs/PLAN.md::agent.rs` describe Fase 5 (agentes IA con Anthropic, OpenAI, DeepSeek y Minimax) con herramientas de escritura que requieren aprobación. El módulo no existe en el código. `docs/CHANGELOG.md::Fase 4+ — Pendiente` confirma que sigue pendiente.
- `docs/PLAN.md::Ports / Endpoints` propone tres ramas (loopback alterna, IP alterna, fallback de puerto). El código implementa solo la tercera (`autoselect_endpoint`); las funciones de soporte existen (`netcheck::pick_loopback_ip`) pero no se llaman.
- `docs/PLAN.md::Agent (en keysring)` propone guardar la API key en libsecret/Keychain; no hay `agent.rs` ni keyring todavía.
- `ideas-cambios.md` (no committeado en este repo) menciona ideas no implementadas: refactor del proyecto, deploy para varios entornos (cPanel, AWS bitnami), plugin para deploy "pull-only" en servidores sin SSH, y un proyecto paralelo tipo Panel WP para VPS con subset de funciones. Estas ideas siguen abiertas; no hay código que las implemente.

Volver al índice principal de `docs/resume/`.
