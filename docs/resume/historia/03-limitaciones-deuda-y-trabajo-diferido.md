# Limitaciones, deuda técnica y trabajo diferido

Este documento cataloga los huecos activos del proyecto: limitaciones operativas documentadas, deuda técnica observable en el código y trabajo fuera de fase que sigue pendiente. La regla es la misma que en los otros runbooks: **el código actual prevalece sobre la prosa histórica**. Si una limitación documentada ya no se reproduce, se anota explícitamente.

Fuentes principales: `docs/KNOWN_ISSUES.md`, `docs/CHANGELOG.md`, `docs/PLAN.md::Fases y Diferido`, `src-tauri/src/**` y `ideas-cambios.md` (no committeado en el repo; ver nota al final).

## 1. Limitaciones activas en uso

### L1. Import LocalWP requiere dump en disco

**Síntoma**: el importador (`localwp::import_site`) copia `app/public` y el dump `app/sql/local.sql` que LocalWP deja en disco. La DB no se extrae del MySQL de LocalWP en vivo: si `local.sql` no existe (o está desactualizado), el sitio se migra con la base de datos vacía.

**Mitigación documentada**: exportar la DB desde LocalWP antes de importar.

**Posible mejora**: añadir un modo que ejecute `mysqldump` en el host sobre el socket de LocalWP (`~/.config/Local/run/.../mysql` típicamente), pero requiere saber dónde está el socket de cada sitio de LocalWP. No implementado.

Citado: `docs/KNOWN_ISSUES.md::Import LocalWP: la DB requiere el dump en disco`.

### L2. Reconstructed (sin sidecar) es best-effort

**Síntoma**: `import_disconnected_site` re-importa sin pérdida las carpetas con `config.disconnected.json` (`preserved`). Para carpetas viejas sin ninguna config (`reconstructed`), la metadata se deduce best-effort: nombre = carpeta, dominio `{slug}.test`, `dbName` parseado de `wp-config.php` (o el slug), y versiones PHP/DB por defecto (`config::DEFAULT_PHP = "8.3"`, `config::DEFAULT_DB = "8.0"`).

**Mitigación documentada**: tras importar, revisa dominio y versiones en `/site/[id]` antes de "Migrar y encender".

**Riesgo residual**: el importador no resuelve colisión de dominio. Si la carpeta `reconstructed` tiene el mismo dominio que un proyecto vivo, el `Migrar y encender` posterior reescribirá `siteurl`/`home` con la URL del nuevo (puede pisar el sitio vivo). Documentar el orden de operaciones o renombrar antes de importar.

Citado: `docs/KNOWN_ISSUES.md::Importar proyecto: carpetas sin config (reconstructed) son best-effort`.

### L3. Botones de la barra de título no respetan la config de KDE

**Síntoma**: en KDE/Wayland los botones (cerrar/min/max) no aparecen donde el usuario los tiene configurados (este equipo: izquierda, `kwinrc` `ButtonsOnLeft=XAIH`).

**Intentos hechos**:
- `decorations: true` en `tauri.conf.json` (decoración activada) — no bastó.
- `GTK_CSD=0` en el arranque (`lib.rs`) — **no tuvo efecto**; revertido para no dejar cambios inertes.

**Hipótesis a probar**:
- tao/GTK en Wayland fuerza CSD y no consume el protocolo `xdg-decoration` (server-side) que ofrece KWin.
- Sincronizar `gtk-decoration-layout` en runtime.
- Probar en sesión X11 para aislar.
- Revisar webkit2gtk/tao upstream.

**Estado**: diferido por decisión del usuario hasta cerrar todas las fases.

> Convergencia: la prosa menciona `GTK_CSD=0` como revertido, pero el código actual (`lib.rs::run`) no aplica esa env var. El `tauri.conf.json` actual no incluye `GTK_CSD`. El síntoma persiste en Wayland; en X11 los botones sí respetan la config del usuario.

Citado: `docs/KNOWN_ISSUES.md::Botones de la barra de título no respetan la config de KDE`.

### L4. `try_files` cubre lectura web, no lectura por filesystem desde PHP (clones)

**Síntoma**: en un clone temporal, los archivos de uploads viejos del padre se sirven vía `try_files $uri @uploads_base` desde nginx, pero un plugin de PHP que escanee el directorio de uploads del clone solo ve los archivos nuevos.

**Caso típico afectado**: `wp media regenerate` no recupera thumbnails viejos (escanea el dir de uploads del clone).

**Aceptación**: para los casos de uso documentados (validar update de theme, medir daño de mover archivos), el comportamiento actual es suficiente.

**Posible mejora**: overlayfs (`mount -t overlay`) que fusione el dir del padre (ro) y el del clone (rw) en un solo mountpoint para PHP. Requiere `pkexec`; no implementada.

Citado: `docs/CHANGELOG.md::Limitaciones conocidas` y `docs/plans/clones-temporales.md::Limitaciones conocidas`.

### L5. `wp db import` desde el wrapper no respeta el cert autofirmado de MySQL 8

**Síntoma**: el cliente `mariadb` (en la imagen php) intenta verificar el cert TLS de MySQL 8 y falla con `TLS/SSL error: self-signed certificate`.

**Mitigación actual**: el panel no usa `wp db import` desde la imagen php; importa directamente en el container DB con `docker exec -i ... mysql` (socket local, sin TLS). La imagen php aún instala `mariadb-client` para que `wp db` funcione desde el wrapper con DBs remotas bien configuradas.

**Posible mejora**: añadir `MYSQL_OPT_SSL_VERIFY_SERVER_CERT=0` o configurar `wp-config.php` con `MYSQL_CLIENT_FLAGS` apropiado. No se hace porque rompe la verificación de certs legítimos.

Citado: `docs/CHANGELOG.md::Fix — import/export de DB: hacerlo en el container DB (sin TLS)`.

### L6. Sin `search-replace` automático al migrar

**Síntoma**: `migrate::fix_site_url` ajusta `home`/`siteurl` con `wp option update`, pero no recorre el contenido (posts, options serializadas) en busca de URLs `*.local` u otros dominios del origen. Si el dump vino de LocalWP (`*.local`) o de otro dominio, las URLs embebidas en el contenido siguen apuntando al origen.

**Mitigación documentada**: ejecutar `wp search-replace` manualmente tras migrar.

**Riesgo residual**: en sitios de LocalWP con `*.local` por todos lados, el admin funciona pero las imágenes, los enlaces internos y los widgets no se ven correctos hasta que se aplique el search-replace.

Citado: `docs/KNOWN_ISSUES.md::Import LocalWP: la DB requiere el dump en disco`.

### L7. Multisite de LocalWP no se migra automáticamente

**Síntoma**: `localwp::import_site` detecta `multi_site != ""` y lo anota en `note`, pero `migrate::fix_site_url` no configura `wp-config.php` para multisite. El sitio arrancará como instalación simple.

**Mitigación**: tras migrar, añadir las constantes `MULTISITE` y `SUBDOMAIN_INSTALL`/`SUNRISE` a `wp-config.php` y reescribir el `.htaccess` (o el `try_files` del vhost) según la guía de WP multisite.

Citado: `docs/CHANGELOG.md::Importación desde LocalWP` y `docs/KNOWN_ISSUES.md` (no lo nombra explícitamente, pero la prosa lo deja entrever).

### L8. VPS no soportado

**Síntoma**: el panel asume Linux con `NetworkManager` (para el wildcard dnsmasq) y un solo usuario con `~/.local/bin` para los wrappers. No hay build release para macOS (Tauri 2 compila, pero el flujo de release actual solo produce AppImage/deb/rpm) ni instalador para Windows.

**Estado actual**: el plan menciona macOS como futuro (`docs/PLAN.md::Futuro: versión macOS`); la implementación se reduce a enumerar los puntos a adaptar (Tauri ya soporta macOS, dnsmasq vía Homebrew + `/etc/resolver/test`, socket Docker en VM, mkcert portable, `~/Library/Application Support/wordpress-panel`).

Citado: `docs/PLAN.md::Futuro: versión macOS`.

### L9. Importación desde LocalWP sin dump no avisa antes de migrar

**Síntoma**: el importador deja un `imported.sql` solo si LocalWP tenía `app/sql/local.sql`. Si no, el sitio queda pendiente con la DB vacía y la primera señal es al hacer "Migrar y encender": `migrate.rs::run_migration::None` emite `[5/6] No hay dump en app/sql/: el sitio arranca con la DB vacía`.

**Mitigación documentada**: `localwp::import_site` añade `note` con la advertencia, pero esa nota se pierde si el usuario ignora el resultado del importador.

**Posible mejora**: hacer que la UI pin la nota en la cabecera del proyecto importado hasta que complete la migración.

Citado: `docs/CHANGELOG.md::Importación desde LocalWP` (anota versiones ajustadas y ausencia de dump).

## 2. Deuda técnica observable en el código

### D1. Estado del proyecto queda con SiteConfig parcial si `create_site` aborta

**Detalle**: `wordpress::create_site` (pasos 1–10) es secuencial. Si el paso 5 (descarga del tarball) o el 8 (`wp core install`) fallan, el SiteConfig ya se escribió en `config.json` y el contenedor php puede haber quedado creado. El resultado: un proyecto "medio creado" que el panel reconoce pero que no funciona del todo.

**Mitigación parcial**: el botón "Cancelar" en proyectos `migrationPending` llama `delete_site` con `deleteFolder=true`, lo mismo aplicaría aquí. Pero la UI no expone un "limpiar" para `create_site` abortado.

**Posible mejora**: ejecutar todo `create_site` dentro de un `tauri::async_runtime::spawn` con rollback similar al de `worktree::run_create::catch`. O confirmar el nombre del proyecto antes de iniciar la creación.

Citado: `src-tauri/src/wordpress.rs::create_site`, sin cobertura de tests de integración explícita para el path de fallo.

### D2. Capacidad de los eventos no se valida en build

**Detalle**: `src-tauri/capabilities/default.json` declara los permisos, pero `pnpm build` no verifica que la capability exista. Si alguien borra el archivo por error, la consola sale vacía y el `listen('op-log')` falla silenciosamente.

**Mitigación**: los tests e2e no cubren este caso porque usan IPC mockeado. El fix histórico (`docs/CHANGELOG.md::Fix — consola de progreso vacía`) añadió el archivo pero no hay test que verifique su presencia.

**Posible mejora**: añadir un test de integración que compruebe que el capability concede `core:event:default` y que `OpConsole` recibe líneas al hacer `app.emit('op-log', ...)`.

### D3. `Autodump::start` se engancha aunque el proyecto esté parado

**Detalle**: `lib.rs::start_site` engancha el watcher de auto-dump tras `docker::start_site`. Si el proyecto se apaga después (`stop_site` llama `AutoDump::stop`), el watcher termina. Pero si el proyecto nunca llegó a encender (p. ej. `start_site` falló en el último paso, pero el `start` se ejecutó parcialmente), el watcher queda enganchado contra un container que no existe.

**Mitigación parcial**: `autodump::watch` chequea `is_running(&db_container)` cada 20 s y `continue` si no está. La consecuencia es ruido, no error: el watcher se queda en bucle infinito haciendo `SHOW GLOBAL STATUS` contra un container inexistente, que fallará y se reintentará.

**Posible mejora**: si `start_site` falla, `lib.rs::start_site` debería llamar `AutoDump::stop(&id)` en el path de error.

Citado: `src-tauri/src/autodump.rs::watch`, sin cobertura de tests.

### D4. El CLI bash depende de `python3` para desenvolver la salida de `gdbus`

**Detalle**: `scripts/wordpress-panel-cli.sh::dbus_json` con `gdbus` usa `python3 -c 'import sys,ast; print(ast.literal_eval(sys.stdin.read())[0])'` para desenvolver la tupla `('json',)`. Si `python3` no está en el PATH o el `literal_eval` falla, la salida se devuelve cruda y el `jq` posterior puede fallar.

**Mitigación**: la rama `qdbus6` no necesita `python3`. El wrapper cae a `qdbus6` si está disponible.

**Posible mejora**: pre-procesar con `gdbus` directamente (`gdbus call --dest ... --object-path ... --method ... | head -c -2 | tail -c +2` o similar) para evitar la dependencia de `python3`.

Citado: `scripts/wordpress-panel-cli.sh::dbus_json`.

### D5. El wrapper `wp` re-descubre el proyecto en cada llamada

**Detalle**: `scripts/wp-wrapper.sh` ejecuta `wordpress-panel-cli detect-project "$PWD"` en cada invocación. Si el CWD es `/srv/projects/{parent}/app/public/wp-content/themes/mi-theme`, la búsqueda recorre todos los `config.json` y parsea el `path` con `sed` por cada uno. Para instalaciones con muchos proyectos (cientos), esto es lento.

**Mitigación**: el comando `start` del CLI usa el mismo patrón y es proporcional al número de proyectos. No hay test de rendimiento documentado.

**Posible mejora**: cachear el mapeo `path → id` en `~/.local/share/panel-wp/path-cache` con invalidación al detectar cambio de mtime de `config.json`.

Citado: `scripts/wordpress-panel-cli.sh::project_for`.

### D6. `migrate::import_dump` no mide el progreso por bytes aplicados

**Detalle**: el watchdog usa el **tamaño real de la DB** (`information_schema.tables WHERE table_schema='{db}'`) como señal de vida. Si el motor es Postgres, el gate no aplica (`autodump::write_counter` devuelve `None` para Postgres). En ese caso, la única señal de actividad es el avance del stdin, que es propensa a falsos timeouts.

**Mitigación**: la importación funciona en Postgres (mismo flujo), pero el watchdog puede matar dumps grandes que apliquen en silencio.

**Posible mejora**: añadir un gate de "bytes aplicados" para Postgres usando `pg_stat_database` o el progreso de `pg_restore`.

Citado: `src-tauri/src/autodump.rs::write_counter` y `src-tauri/src/migrate.rs::watchdog` (sin gate para Postgres).

### D7. La `pick_alt_port` siempre arranca en 8080

**Detalle**: `docker::autoselect_endpoint` busca el primer par libre desde 8080/8443. Si el usuario tiene otros servicios comunes en 8080 (un IDE o dev server), el panel le quita ese puerto silenciosamente.

**Mitigación**: `preflight_endpoint` reporta el error antes de `ensure_nginx`, pero `autoselect_endpoint` ya guardó el endpoint en `panel.json` antes de ese chequeo. Reintenta con otro par, pero el orden es determinista.

**Posible mejora**: priorizar pares configurables (`panel.json::preferredPorts`) o detectar rangos "evitar" (8080, 8443, 9000, 9100, 9101, 8025, 8088) y excluirlos.

Citado: `src-tauri/src/docker.rs::autoselect_endpoint`.

### D8. `set_php_upload_limit` no actualiza la `config.json` del Worktree-project

**Detalle**: el comando `lib.rs::set_php_upload_limit` modifica `site.services.php.uploadMaxMb` y reescribe el `php.ini` del proyecto, pero la lógica no considera el caso del worktree-project, donde el `php.ini` se monta en el container como cualquier otro proyecto. El worktree hereda el `php.ini` del padre porque no tiene `php.ini` propio (su `php_ini()` devuelve `{path}/conf/php/php.ini` del worktree, que existe).

**Mitigación**: el flujo actual funciona: el worktree tiene su propia carpeta `conf/php/php.ini` (creada por `wordpress::create_dirs` en `run_create` paso [1/7]). El comando opera sobre el SiteConfig del worktree, así que el `php.ini` correcto se reescribe.

**Riesgo residual**: si el usuario quiere que el worktree herede el `uploadMaxMb` del padre, no hay forma de declararlo; siempre se reescribe el `php.ini` del worktree con el valor del worktree. No documentado como limitación.

Citado: `src-tauri/src/lib.rs::set_php_upload_limit` (no filtra por `worktree_of`).

### D9. `list_disconnected_sites` no contempla colisión de dominio

**Detalle**: `import_disconnected_site` detecta colisión de `id` (regenera uuid), pero no detecta colisión de `domain` con un proyecto vivo. Si importas una carpeta con `config.disconnected.json` que tiene un dominio ya usado por un proyecto vivo, el `domain` se queda como está. La migración posterior reescribirá `siteurl`/`home` con el dominio del "nuevo" (que en realidad es el del vivo).

**Mitigación**: en la UI, `ImportProjectModal.svelte` muestra el dominio antes de importar, pero no compara con proyectos vivos.

**Posible mejora**: en `lib.rs::import_disconnected`, si `site.domain` ya está en un proyecto vivo, devolver error con sugerencia de renombrar.

Citado: `src-tauri/src/lib.rs::import_disconnected`.

### D10. `delete_site` con `deleteFolder=true` no espera al teardown de compartidos

**Detalle**: tras `remove_dir_all(site.path)`, el código retorna inmediatamente. `teardown_unused_shared` se llama antes, pero si el borrado dejó otros proyectos activos que compartían la misma DB, el motor no se apaga (espera al último stop). Sin embargo, el `remove_dir_all` no es transaccional: si la UI llama `delete_site` en paralelo con un `start_site` del mismo id (race), puede dejar un SiteConfig vivo apuntando a un directorio inexistente.

**Mitigación**: el bloqueo viene del patrón de uso de la UI (no expone borrado y encendido simultáneo). El CLI/MCP no exponen `delete_site` directamente.

**Posible mejora**: usar un `Mutex` o un lock por id en `lib.rs::delete_site` para serializar.

Citado: `src-tauri/src/lib.rs::delete_site` (sin lock por id).

### D11. `create_clone` no clona `mu-plugins` del padre

**Detalle**: `clone::run` inyecta los mu-plugins del panel (`panel-mailpit.php` + `panel-autologin.php`) en el paso 5. Si el padre tiene mu-plugins custom (p. ej. un plugin del usuario que envía webhooks), no se copian al clone.

**Mitigación**: el `tar --zstd -xf` en el paso 2 extrae `code.tar.zst` del snapshot, que sí incluye `wp-content/mu-plugins/` completo (excepto lo excluido por el snapshot, que son `uploads`/`cache`/`wp-config`/`*.log` y los `snapshot_excludes` del proyecto).

**Verificación**: la prosa dice "extrae el código del snapshot" y luego "sincroniza plugins del panel" — el segundo paso reescribe solo los del panel, no los custom. Los custom vienen del tar del snapshot.

**No es bug**, es comportamiento esperado. Documentado para evitar confusión.

Citado: `src-tauri/src/clone.rs::run::step 2 y step 5`.

### D12. La ruta al phar de WP-CLI se detecta con `docker inspect` o se descarga a `config_dir`

**Detalle**: `php::wp_cli_phar_path` descarga `wp-cli.phar` a `~/.config/wordpress-panel/wp-cli.phar` y lo monta como `/usr/local/bin/wp` en el container. Si la descarga falla (sin red), no hay fallback. El comando `wp` no existe en la imagen php sin ese phar.

**Mitigación**: la imagen php sí instala `php` (intérprete) y los binarios de `mariadb-client`. `wp` se monta desde el host.

**Riesgo**: si el host pierde acceso a `raw.githubusercontent.com` después de la primera instalación, no se puede actualizar `wp-cli.phar`. No hay versión fija.

**Posible mejora**: cachear en `config_dir` con verificación de versión y reintento; alternativa, incluir el phar en la imagen php via Dockerfile.

Citado: `src-tauri/src/php.rs::wp_cli_phar_path`.

## 3. Diferidos fuera de fase

### F1. Fase 5 — Asistente IA (`agent.rs`)

**Estado**: no implementado. `docs/PLAN.md::Agentes de IA` describe un chat contextual con herramientas de lectura (read_config, read_logs, list_plugins, list_themes, get_container_stats) y de escritura (write_config, write_wpconfig, exec_wpcli, restart_service) con aprobación humana. Proveedores: Anthropic, OpenAI, DeepSeek, Minimax. API key en libsecret/Keychain.

**Riesgo de divergencia**: `docs/PLAN.md::Agentes de IA` lista modelos `claude-sonnet-4-6`, `claude-opus-4-8`, `claude-haiku-4-5`, `gpt-4o`, `gpt-4o-mini`, `o3-mini`, `deepseek-chat`, `deepseek-reasoner`, `MiniMax-Text-01`, `abab6.5s`. Varios de esos modelos están deprecados o no existen en los catálogos actuales. Antes de implementar, contrastar contra los modelos vigentes en la API del proveedor.

**Citado**: `docs/PLAN.md::Fase 5 — Asistente IA (opcional, expansible)`, `docs/CHANGELOG.md::Diferido (fuera de fase)`.

### F2. Headless: container de frontend separado

**Estado**: el formulario de "Nuevo Proyecto" acepta `headless: true` + `frontendFramework` y los guarda en `SiteConfig`, pero no aprovisiona el container de frontend. El usuario debe configurar el frontend por su cuenta.

**Aceptación documentada**: el feature se ofrece (los flags existen); la integración queda al usuario.

Citado: `docs/CHANGELOG.md::Fase 3 — Hecho` y `docs/PLAN.md::Headless WordPress + frontend (Next.js, etc.)`.

### F3. Plugin S3 (MinIO ↔ WP)

**Estado**: MinIO se ofrece como servicio compartido on-demand. No hay un plugin que conecte WP a MinIO para media offload.

**Aceptación documentada**: el servicio se ofrece, la integración WP queda al usuario.

Citado: `docs/CHANGELOG.md::Fase 3 — Hecho` y `docs/PLAN.md::MinIO container + file browser UI`.

### F4. Botones de la barra de título en KDE/Wayland

**Estado**: diferido. Ver L3.

### F5. macOS como plataforma soportada

**Estado**: Tauri 2 compila para macOS, pero el flujo de release solo produce AppImage/deb/rpm. Faltan el instalador, el .app, la adaptación de rutas (`~/Library/Application Support`), y la configuración de dnsmasq vía Homebrew.

Citado: `docs/PLAN.md::Futuro: versión macOS`.

### F6. Visual del plasmoid en sesión Plasma real

**Estado**: los tests del plasmoid son headless (no hay sesión Plasma). El flujo de release lo instala con `kpackagetool6 --upgrade`, pero la verificación visual (cómo se ve en la barra, cómo se actualiza) no se automatiza.

Citado: `docs/CHANGELOG.md::Fase 2 — Pendiente de verificación`.

## 4. Ideas registradas en `ideas-cambios.md` (no committeado)

El archivo `ideas-cambios.md` (no commiteado, no presente en este worktree) recoge tres ideas del autor en prosa. Ninguna está implementada ni tiene plan formal. Se mencionan aquí solo para no perderlas:

### I1. Refactorización del proyecto

> "tengo la sensación que hay que optimizar, analiza el proyecto e indícame qué podríamos mejorar y haz un plan de refactorización, el proyecto creció más de lo previsto en un inicio por lo que no tengo duda que necesite una refactorización y posiblemente un cambio de arquitectura."

No hay plan; no es parte de Fase 5. Cita el propio autor.

### I2. Deploy multi-entorno (cPanel, AWS bitnami)

> "hay que buscar una manera de hacer deploy de los proyectos de git que cuando se necesite subir los cambios, como son varios proyectos, algunos trabajan con cpanel y otros usan bitnami-wordpress de aws necesito un método para hacer deploy desde el panel."

> "para el resto que no tengo acceso al ssh podríamos crear un plugin para que yo envíe desde el panel al plugin la señal y se actualice haciendo un empaquetado del proyecto antes de enviar y en producción desempaquetar haciendo la instalación completa como lo haría wordpress de manera automática."

Estado: no implementado. El `gh_deploy` actual (en `github.rs::deploy`) es por repo desde el host, asumiendo acceso SSH al server. No hay plugin WP para "pull" en servidores sin SSH.

### I3. Panel WP para VPS (subset)

> "el proyecto ya está listo para usarlo en local, ahora necesito que hacer otro similar para agregarlo en vps. primero necesito que me des una lista de todas funciones que se le agregaron a este proyecto, voy a escoger las que necesito que tenga el vps ya que hay parte como el clonar, github-worktree o la del autologin que no quiero que estén en producción."

> "el objetivo de este nuevo proyecto es dar la oportunidad de tener un extremo para hacer deploy de una manera sencilla."

Estado: no implementado. Es un proyecto nuevo (no un flag en este). El subset excluye explícitamente: clones, worktrees, autologin, snapshots.

> **Convergencia**: las ideas I1–I3 quedan fuera del alcance de este runbook; no se documentan como deuda activa porque no están planificadas. Si se retoman, conviene crear un plan dedicado en `docs/plans/`.

Citado: `ideas-cambios.md` (en la raíz del repo, no committeado).

## 5. Diagnóstico de convergencia prosa ↔ código

Lista de contradicciones resueltas y a vigilar al actualizar la documentación:

| Proposito | Plan/Changelog | Código actual | Estado |
|---|---|---|---|
| "Fase 1 en curso" | `docs/CHANGELOG.md` | Fases 2–4 completas; `lib.rs::run` instala wrappers; `create_site` provisiona WP | **Resuelto**: los runbooks reflejan el código real |
| `GTK_CSD=0` en `lib.rs::run` | CHANGELOG "revertido" | No aparece en el código actual | **Resuelto**: no se aplica |
| Endpoint con tres ramas (loopback/IP/puerto) | `docs/PLAN.md` | Solo `autoselect_endpoint` con puertos altos | **Resuelto**: documentado en `02-decisiones-y-fixes-criticos.md::D4` |
| Fases 5 IA (agent.rs) | `docs/PLAN.md` | No existe | **Pendiente**: la prosa lo deja como opcional |
| Plugin S3 MinIO↔WP | `docs/PLAN.md` y CHANGELOG "fuera de fase" | No existe | **Pendiente** |
| Headless frontend container | `docs/PLAN.md` y CHANGELOG "fuera de fase" | No existe (flags guardados) | **Pendiente** |
| macOS | `docs/PLAN.md` | Sin release target | **Pendiente** |
| Botones KDE/Wayland | `docs/KNOWN_ISSUES.md` y CHANGELOG | Sin fix en el código | **Pendiente** |

Volver al índice principal de `docs/resume/`.
