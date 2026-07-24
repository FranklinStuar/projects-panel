# Runbook de importación, migración y recuperación

Este runbook cubre tres flujos de movimiento de proyectos:

1. **Migración entre sistemas**: `~/panel-wp/` copiado a otro equipo, o traído de vuelta al actual.
2. **Importación desde LocalWP**: mover sitios de LocalWP al panel.
3. **Re-importación de carpetas desconectadas**: proyectos borrados por el panel con la opción "no borrar la carpeta".

También cubre recuperación tras apagón sucio y tras una pérdida de datos.

## Matriz por superficie

| Operación | UI | CLI | MCP | D-Bus |
|---|---|---|---|---|
| Migrar y encender (pendiente) | botón ámbar en el dashboard / detalle | n/d | n/d | n/d |
| Listar importados de LocalWP | ruta `/import-localwp` | n/d | n/d | n/d |
| Importar sitio de LocalWP | botón "Importar" por sitio en `/import-localwp` | n/d | n/d | n/d |
| Listar desconectados | modal "Importar proyecto" en el dashboard | n/d | n/d | n/d |
| Importar desconectado | botón "Importar" en `ImportProjectModal` | n/d | n/d | n/d |
| Exportar DB manualmente | tab "Info" → "Exportar DB" | n/d | n/d | n/d |
| Reparar nginx | ruta `/settings` → Mantenimiento | n/d | n/d | n/d |
| Reparar php.ini de todos | ruta `/settings` → Mantenimiento | n/d | n/d | n/d |
| Reparar auto-login | tab "Plugins/Themes" del proyecto | n/d | n/d | n/d |
| Detección de huérfanos | automática al abrir el panel | n/d | n/d | n/d |

`wordpress-panel-cli` y el MCP no exponen estos flujos. Son operaciones de la UI o del D-Bus, salvo las consultas a `containers`/`logs`/`resources` que siguen disponibles para inspección.

## 1. Precondiciones universales

- `panel-net` existe y `dnsmasq` resuelve `*.test` (o la IP alterna del endpoint) a loopback.
- Si vas a importar de LocalWP: LocalWP debe haber exportado la DB a `app/sql/local.sql` (el importador no extrae del MySQL de LocalWP en vivo; ver `docs/KNOWN_ISSUES.md`).
- Si vas a migrar entre sistemas: la carpeta del proyecto debe incluir todo bajo `~/panel-wp/{slug}/`. Sin red de por medio, basta `cp -a` o un `tar` del proyecto entero.

## 2. Migración entre sistemas (Fase 4)

### Precondiciones

- La carpeta del proyecto original está en `~/panel-wp/{slug}/` con `config.json`, `app/public/`, `app/sql/db-*.sql`, `conf/php/php.ini`, `ssl/` (regenerable), `logs/`.
- El nuevo sistema tiene el panel instalado (ver `01-instalacion-y-primera-ejecucion.md`).
- Si el sistema original tenía `panel.json` con un endpoint en puerto alto, el sistema nuevo elegirá otro puerto alto propio; las URLs de los sitios cambian y `siteurl`/`home` se reescriben en la migración (`migrate::fix_site_url`).

### Procedimiento

1. Copia la carpeta al nuevo sistema. La forma exacta (rsync, scp, disco externo) es indiferente; mantén la estructura.
2. En el sistema nuevo, abre el panel. `config::load_all_sites` escanea `~/panel-wp/*/config.json`; los proyectos copiados aparecen en la lista con badge ámbar "pendiente de migración".
3. En el dashboard o en el detalle del proyecto, pulsa **Migrar y encender**. Se abre `OpConsole` con el progreso.

### Lo que ocurre (`migrate::run_migration`)

1. Verifica que `app/public/` exista.
2. Inyecta los mu-plugins del panel (`wordpress::sync_mu_plugins`) para garantizar `panel-mailpit.php` y `panel-autologin.php`.
3. `[1/6]`: `docker::ensure_db` + `wordpress::create_database` (idempotente: `CREATE DATABASE IF NOT EXISTS`).
4. `[2/6]`: si SSL, `ssl::generate` (mkcert con la CA del sistema nuevo, regenera cert/key).
5. `[3/6]`: `docker::start_site` (enciende php + vhost + recarga nginx). La 1ª vez puede tardar: construye la imagen `panel-php:{ver}-r3`.
6. `[4/6]`: `wordpress::wp_config_create` reescribe `wp-config.php` con las credenciales del nuevo sistema.
7. `[5/6]`: si hay un dump en `app/sql/`, lo importa. Si no, deja la DB vacía y avisa al usuario. La importación es `migrate::import_dump`:
   - Vía `docker exec -i` (excepción al "Docker solo vía bollard" documentada en `migrate.rs::import_dump`): el `exec_stdin` de bollard se cuelga con dumps grandes porque su stream de salida no emite `None` al terminar el proceso.
   - Acelera con pragmas: `SET autocommit=0; SET unique_checks=0; SET foreign_key_checks=0;` + `COMMIT;` final. Esto evita un fsync y la revalidación de índices/FK por statement.
   - Watchdog de 3 min: si **ni** el stdin avanza **ni** crece la DB (`information_schema.tables WHERE table_schema='{db}'`), mata el exec y llama a `wordpress::reset_database` (`DROP DATABASE` + recrea vacía). El error devuelto: "import cancelado: sin actividad por 3 min. La DB se restauró vacía; reintenta la migración para importar de nuevo."
   - Barra en vivo: `op-log` con prefijo `` (SOH) que `OpConsole.svelte` reescribe en sitio: `12/53 MB ━━━━━──── 1:23`.
8. `[6/6]`: si importó dump, `migrate::fix_site_url` ejecuta `wp option update home {url}` y `siteurl {url}` con `--skip-plugins --skip-themes` (un plugin/mu-plugin que se cuelgue al cargar no debería romper la migración). URL = `endpoint::site_url(dominio, ssl)`.
9. Marca `migration_pending = false`, `last_migrated_at = now()`.
10. Devuelve la `SiteConfig` actualizada al frontend. Cualquier error se emite también por `op-log` con un `✗ La migración falló: ...` (ver `migrate::migrate_site`).

### Cambio esperado y evidencia

- El proyecto pasa de "pendiente" a "corriendo".
- `config.json` del proyecto queda con `migrationPending: false` y `lastMigratedAt` poblado.
- `app/sql/db-*.sql` es la nueva foto tras la migración; los dumps anteriores del origen se conservan en la misma carpeta (importar no los borra).

### Abortar

- La UI no expone un botón de abortar para la migración en curso; usa la "ventana de gracia" de OpConsole para cerrarla tras el `✗`. Si ya está importando, la única forma es matar el `docker exec` (lo hace `migrate::import_dump` con su watchdog o al fallar).
- Si el panel se cierra a mitad, la DB puede haber sido creada vacía (paso [1/6]); reintenta la migración.

### Recuperar

- Si `[5/6]` se cancela por watchdog, la DB queda vacía y reintentar la migración vuelve a empezar de cero (los pasos 1–4 son idempotentes).
- Si la URL quedó con un dominio antiguo (porque el dump traía `*.local` o similar), `fix_site_url` lo corrige, pero no hace `wp search-replace` del contenido: URLs embebidas en posts siguen apuntando al dominio origen. Corrige manualmente con `wp search-replace` desde el wrapper.
- Si el cert SSL no regeneró: `nginx -s reload` falla. Vuelve a **Regenerar SSL** en el menú "···" del proyecto.

### Limitaciones documentadas

- Solo MySQL/MariaDB: la importación vía `docker exec -i ... mysql` aplica a motores compatibles con `mysql -uroot -ppanel`. Para Postgres, `migrate::import_dump` aún no está adaptado; el flujo de Fase 4 está cableado al cliente `mysql` (ver `backup.rs` y `migrate.rs`).
- Sin `search-replace` automático: documentado en `docs/KNOWN_ISSUES.md`.
- `multisite` de LocalWP: el importador lo detecta y avisa, pero la migración no configura wp-config para multisite.

### Riesgos

- `docker exec` con stdin adjunto desde bollard fue retirado a propósito (ver `docker.rs::import_dump` eliminado y la nota en el módulo); el CLI es la única vía soportada.
- El watchdog termina el `docker exec` con `start_kill`; si el mysql todavía está aplicando el último statement, queda cancelado de forma sucia pero la DB queda vacía por el `reset_database`.
- El fix de URL usa `--skip-plugins --skip-themes`: válido para el `option update` pero no debe extrapolarse a otras operaciones.

## 3. Importación desde LocalWP

### Precondiciones

- LocalWP instalado y con al menos un sitio en `~/Local Sites/{site}/`.
- `~/.config/Local/sites.json` presente y legible.
- El sitio tiene dump `app/sql/local.sql` (ver `docs/KNOWN_ISSUES.md`).
- Versiones de PHP/MySQL del sitio: si caen fuera de las soportadas por el panel, el importador las ajusta a la más reciente y avisa (`localwp::pick_supported`).

### Procedimiento

1. Ve a `/import-localwp`. La UI lista los sitios de LocalWP marcados como "ya importado" si el dominio `.test` colisiona o el nombre coincide con uno del panel (`localwp::list_sites`).
2. Pulsa **Importar** en el sitio deseado. Se abre `OpConsole` con título "Importar desde LocalWP".
3. Al terminar, el sitio aparece en el dashboard como **pendiente de migración**. Enciéndalo con el flujo de §2 (migrar importa el dump y fija las URLs).

### Lo que ocurre (`localwp::import_site`)

1. Lee `~/.config/Local/sites.json` (deserialización tolerante).
2. Copia `app/public/` con `cp -a` (preserva atributos, más rápido que Rust directo).
3. Copia `app/sql/local.sql` como `imported.sql` y registra el tamaño en el log.
4. Mapea versiones PHP/MySQL a las soportadas (si ajusta, lo anota en `note`).
5. Escribe `config.json` con `migrationPending: true`, grupo `"LocalWP"`.
6. Si la versión no está soportada, lo avisa en `note`.

### Cambio esperado y evidencia

- `~/panel-wp/{slug}/app/public/` contiene los archivos de LocalWP.
- `app/sql/imported.sql` con el dump original.
- El sitio aparece en el dashboard con badge ámbar.

### Abortar y recuperar

- El importador es secuencial: si falla, el directorio puede quedar parcial. Elimínalo con el flujo de §6 del runbook anterior (`delete_site` con `deleteFolder=true`) y vuelve a importar.
- Si el ajuste de versión es bloqueante (p. ej. PHP 5.6 → 8.4), importa de todos modos y migra: el WP no se encenderá, pero el import queda preservado.

### Limitaciones

- DB requiere dump en disco (no extrae del MySQL de LocalWP en vivo).
- No hace `wp search-replace` del contenido.

### Riesgos

- Copia la totalidad de `app/public/`, que puede ser muy grande (tema, plugins, uploads). Asegúrate de tener espacio.
- El `imported.sql` no se rota; se queda para siempre en `app/sql/`.

## 4. Re-importar proyectos desconectados

### Precondiciones

- Existe una carpeta bajo `~/panel-wp/` sin `config.json` pero con:
  - `config.disconnected.json` (proyecto borrado por el panel con `deleteFolder=false`), o
  - `app/public/wp-config.php` (carpeta traída de fuera, sin metadata).

### Procedimiento

1. En el dashboard, pulsa **Importar proyecto**. Se abre `ImportProjectModal`.
2. La lista muestra las carpetas desconectadas con badge `config conservada` (preserved) o `reconstruido` (reconstructed), PHP/DB, `con dump`/`sin dump`.
3. Pulsa **Importar** en la que quieras. Se abre `OpConsole` con título "Importar proyecto".
4. Al terminar, el sitio aparece como **pendiente de migración**. Enciéndalo con §2.

### Lo que ocurre (`lib.rs::import_disconnected`)

1. Resuelve la ruta bajo `~/panel-wp/{folder}` y valida que tenga `app/public/`.
2. Si hay `config.disconnected.json`: lo lee y actualiza la `path` al directorio actual (por si moviste la carpeta).
3. Si no: `reconstruct_config` deduce nombre = carpeta, dominio = `{slug}.test`, `dbName` = `parse_db_name(wp-config.php)` o `{slug}_db`. Versiones por defecto PHP 8.3 / MySQL 8.0 (`config::DEFAULT_PHP`/`DEFAULT_DB`).
4. Si el `id` choca con un proyecto vivo, genera uuid nuevo.
5. `migrationPending = true`, `lastMigratedAt = None`.
6. Escribe `config.json` y borra el sidecar `config.disconnected.json`.
7. Devuelve `ImportResult { site, note: None }`.

### Cambio esperado y evidencia

- `~/panel-wp/{folder}/config.json` aparece.
- `config.disconnected.json` desaparece.
- El sitio aparece en la lista maestra como pendiente.

### Abortar y recuperar

- El importador es atómico: si falla, no se escribe `config.json`. No deja proyecto fantasma.
- Si reconstruyó versiones equivocadas (caso reconstructed sin sidecar), ajusta manualmente `config.json` antes de migrar.

### Limitaciones

- La rama `reconstructed` (carpetas sin sidecar) es best-effort; revisa dominio y versiones en `/site/[id]` antes de **Migrar y encender**. Las carpetas desconectadas por el propio panel siempre son `preserved`.

### Riesgos

- Un `reconstructed` con dominio colisionando con un proyecto vivo: el importador no resuelve esa colisión; la migración posterior puede pisar URLs. Renombra el proyecto antes de importar o reordena dominios tras la migración.

## 5. Recuperación tras apagón sucio

### Síntomas

- Un proyecto encendido ya no responde; el container tiene `state.running = true` pero `exec` falla con `setns/nsexec`.
- `panel-nginx` no carga ningún sitio, logs con `host not found in upstream`.
- El dump-al-detener no se generó (apagón con proyectos activos).

### Procedimiento general

1. Abre la UI y ve a **Configuración** → **Mantenimiento** → **Reparar nginx**.
2. Esto ejecuta `docker::repair_nginx`:
   - `prune_orphan_vhosts`: borra `~/.config/wordpress-panel/nginx/conf.d/{id}.conf` cuyo container `wp-{id}` no esté corriendo.
   - `remove_container` (force) sobre `panel-nginx`.
   - `ensure_nginx` recrea el container.
3. Enciende los proyectos que aún quieras (los contenedores se recrean con la imagen actual; `IMAGE_REV=r3`).
4. El auto-dump, que ya estaba enganchado, se vuelve a enganchar en `start_site`. Si el proyecto tenía cambios desde el último `db-*.sql` válido, el watcher lo detecta en el primer sondeo (sembrado desde disco, no desde DB viva).

### Cambio esperado y evidencia

- `docker ps` muestra `panel-nginx` y los `wp-{id}` que se volvieron a encender.
- `nginx -T` (dentro del container `panel-nginx`) lista los vhosts activos.

### Limitaciones

- El prune de huérfanos se ejecuta también automáticamente al llamar `ensure_nginx` (`docker::ensure_nginx` lo invoca antes de bindear). El botón **Reparar nginx** es para forzarlo sin tener que apagar/encender proyectos.
- La autocura también recrea `panel-nginx` si su `reload_nginx` falla (porque el container está "running" pero con namespaces muertos): el `remove_container` + `ensure_nginx` equivale a un reload fresco.

### Riesgos

- `prune_orphan_vhosts` borra vhosts de proyectos que quizás estaban "apagados a propósito". Verifica primero la lista de proyectos; si hay alguno que quieres conservar, enciéndelo antes de la poda.

## 6. Pérdida de datos (DB corrupta o sin datadir durable)

### Síntomas

- Tras un reinicio, la DB no contiene los datos esperados.
- `ls -la ~/.config/wordpress-panel/db-data/{container}/` está vacío.
- `docker inspect {container}` muestra un volumen anónimo de Docker, no el bind `config_dir/db-data/{container}`.

### Procedimiento

1. **No** borres el container todavía: el datadir de la capa de escritura puede contener los datos.
2. En la UI, abre el proyecto y comprueba `app/sql/db-*.sql`. El auto-dump debería haber dejado un volcado reciente.
3. Si hay dump: enciende el proyecto y deja que `migrate::fix_site_url` reapunte las URLs si fuera necesario. La DB se recrea vacía en el motor nuevo y se importa el dump con el flujo de §2 (puedes re-importar sin necesidad de "Migrar y encender" ejecutando `mysql -uroot -ppanel db < dump.sql` dentro del container).
4. Si no hay dump: la DB realmente se perdió. Reconstruir desde cero.
5. Como medida paliativa, `docker::ensure_db` detecta containers legados sin bind (`docker::db_has_volume`) y los migra con `docker cp` (excepción al "Docker solo vía bollard", `docker.rs::migrate_db_to_volume`). Este paso es **automático al primer `ensure_db` del motor**, no manual: si ya tenías datos en la capa de escritura y `db_data_dir` estaba vacío, la copia lossless ocurre. Después, el container se recrea con el bind.

### Cambio esperado y evidencia

- `~/.config/wordpress-panel/db-data/panel-mysql-80/var/lib/mysql/` poblado tras la primera ejecución de un proyecto con ese motor.
- `docker inspect panel-mysql-80` muestra `Mounts` con `Source` apuntando a esa ruta.

### Limitaciones

- Si el datadir del host se pobló por error con archivos de otro motor, `migrate_db_to_volume` no copia (chequea que `host_dir` esté vacío antes de `docker cp`). En ese caso la copia queda abortada y hay que revisar manualmente.
- El volumen anónimo de Docker que crea la imagen `mysql` (`VOLUME /var/lib/mysql`) **sobrevive al reinicio** del container pero **se queda huérfano al recrear** (lo recreas al cambiar `IMAGE_REV`, por ejemplo). El migrador a bind protege contra esa pérdida futura.

### Riesgos

- `docker cp` es la única salida al stream tar de bollard para mover un directorio entre containers y host (ver nota en `docker.rs::migrate_db_to_volume`). El riesgo es que un fallo a mitad deja el datadir del host parcialmente poblado; la próxima ejecución detecta "no vacío" y no copia, preservando el estado.

## 7. Criterio de salida

- El sitio abre en el navegador, la DB tiene los datos esperados y los dumps existen.
- `dump-log.jsonl` lista los volcados coherentes con la historia.
- **Configuración** muestra el sistema en verde.
- Si hubo pérdida: documenta qué dump era la última foto buena y re-corrige el flujo de auto-dump si fue la causa.

Los runbooks siguientes (`05`, `06`, `07`) cubren snapshots/clones/worktrees en profundidad, Git/CLI/MCP y diagnóstico/mantenimiento.
