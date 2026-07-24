# 04 — Bases de datos y migración

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para la gestión de la base de datos
> compartida (MySQL/MariaDB/Postgres), el bind-mount durable, la creación
> del esquema por proyecto, la importación de dumps vía `docker exec -i`
> con watchdog y rollback, y la migración de un proyecto traído de otro
> sistema.

## Resultado para el usuario

Cada proyecto del panel tiene su propia base de datos (esquema aislado) en
un container DB compartido por versión (1 container para todos los
proyectos con MySQL 8.0, por ejemplo). Los datos **sobreviven a la
recreación del container** y a apagar la máquina, porque el datadir está
bindeado a `config_dir/db-data/{container}` en el host. La contraseña es
`panel` (root, MySQL/MariaDB) o `panel` (user `panel`, Postgres). El
arranque del motor DB espera a que TCP acepte (evita la ventana de
`--skip-networking` de MySQL 8). Importar un dump grande no se cuelga:
tiene watchdog que mide el avance real por `information_schema` y rollback
a DB vacía si se cancela por inactividad.

## Precondiciones

- Daemon Docker accesible (`docker_ok`).
- Red `panel-net` (se crea con `create_panel_network` la primera vez).
- Imagen oficial disponible (`docker::ensure_image` la pull si no).
- `mysqldump` y `mysql` DENTRO del container DB (los da la imagen oficial).
- Versiones soportadas: `MySQL 8.0, 8.4`; `MariaDB 10.6, 10.11, 11.4`;
  `Postgres 15, 16, 17`. Otras versiones se ajustan en `localwp::pick_supported`.

## Flujo feliz

### A. Asegurar el motor DB compartido (`docker::ensure_db`)

1. `docker::ensure_db(db)` calcula `db_container_name(db)`:
   `panel-{mysql|mariadb|postgres}-{version_sin_puntos}` → `panel-mysql-80`.
2. Si `is_running(container)` → retorna el nombre (no-op).
3. `ensure_image(image)` si la imagen no está local.
4. `db_data_dir(db)` = `config_dir/db-data/{container}/` (lo crea).
5. Caso A — el container ya existe:
   - `db_has_volume(container, host_dir, datadir_in)` comprueba que el
     `mount.source == host_dir` (no un volume anónimo de la imagen).
   - Si migrado: `start_container` + `wait_db_ready`.
   - Si NO migrado: `migrate_db_to_volume` (ver "Migración legacy"
     en fallos).
6. Caso B — el container NO existe:
   - `db_env(db)`: `MYSQL_ROOT_PASSWORD=panel`, `MYSQL_ROOT_HOST=%` para
     MySQL/MariaDB; `POSTGRES_PASSWORD=panel`, `POSTGRES_USER=panel` para
     Postgres.
   - `HostConfig { network_mode: panel-net, binds: ["{host_dir}:{datadir_in}"] }`.
   - `create_container` + `start_container` + `wait_db_ready`.
7. `wait_db_ready(container, db)`:
   - Sondea `mysql -h127.0.0.1 -uroot -ppanel -e "SELECT 1"` (MySQL/MariaDB)
     o `pg_isready -h 127.0.0.1 -U panel` (Postgres) cada 500 ms hasta 60 s.
   - **Importante**: gate con `-h127.0.0.1` para forzar TCP (no socket
     local; en el primer arranque MySQL está en `--skip-networking` y
     el socket ya responde mientras TCP no).

### B. Crear / reiniciar / borrar esquema del proyecto

- `wordpress::create_database(docker, db_container, site)`:
  - MySQL/MariaDB: `mysql -uroot -ppanel -e "CREATE DATABASE IF NOT EXISTS
    \`{db_name}\` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"`.
  - Postgres: `psql -U panel -c "CREATE DATABASE \"{db_name}\";"`.
  - Idempotente (reusa en migración).
- `wordpress::reset_database(docker, db_container, site)`:
  - DROP DATABASE + `create_database` (deja la DB vacía).
  - Usado por `migrate::import_dump` para limpiar tras un import cancelado.
- `wordpress::drop_database(docker, db_container, site)`:
  - DROP DATABASE (no recrea).
  - Usado por `delete_site` y `worktree::remove_worktree` (worktree con
    DB copiada).

### C. Export de la DB (al detectar, al detener, manual)

- `backup::dump_bytes(docker, site)`: `mysqldump -uroot -ppanel
  --single-transaction --no-tablespaces --skip-dump-date {db_name}` ejecutado
  en el container DB, stdout capturado (no `wp db export` desde el container
  php porque el cliente mariadb falla la verificación del cert autofirmado
  de MySQL 8).
  - `--skip-dump-date` para que el dump no varíe entre volcados (el
    auto-dump compara por hash).
- `backup::export_db_to(docker, site, dest)`: dump a un path arbitrario
  (usado por `snapshot::create_snapshot` con `dest = snapshots/{id}/db.sql`).
- `backup::export_db(docker, site)`: dump a `app/sql/db-{stamp}.sql`
  (usado por auto-dump, stop export, manual `export_db`).
- `backup::rotate_dumps(site, keep)`: mantiene solo los `keep` `db-*.sql`
  más recientes; ignora `imported.sql`, `local.sql`, etc. Solo borra
  archivos `db-*.sql`.

### D. Import de un dump grande (`migrate::import_dump`)

1. **CLI**: `docker exec -i {db_container} mysql -uroot -ppanel {db_name}`
   con stdin/stdout/stderr piped. Esta es una excepción justificada al
   "Docker solo vía bollard": el `exec_stdin` de bollard se cuelga con
   dumps grandes (su stream de salida no emite `None`).
2. **Pragmas de sesión** preparados al inicio del stdin:
   `SET autocommit=0; SET unique_checks=0; SET foreign_key_checks=0;`.
   - `autocommit=0` agrupa en una sola transacción (sin `COMMIT` por
     statement).
   - `unique_checks=0` y `foreign_key_checks=0` evitan revalidar índices y
     llaves por cada INSERT.
   - Epílogo: `\nCOMMIT;\n`.
3. **Writer** (task asíncrono): vuelca el dump por chunks de 1 MiB
   (`IMPORT_CHUNK = 1 << 20`). Si `write_all` falla, el task sale
   silenciosamente (el exec ya murió).
4. **Drenaje** de stdout/stderr en tareas separadas (no se hace parse del
   contenido, solo drenar para que el pipe no se bloquee).
5. **Watchdog** cada 2 s (`IMPORT_TICK`):
   - Sondea `SELECT COALESCE(SUM(data_length+index_length),0) FROM
     information_schema.tables WHERE table_schema='{db_name}'` vía
     `docker::exec` (consulta pequeña).
   - Si el tamaño creció, marca `last_activity = now`.
   - **Indicador de vida = DB real, no stdin**: medir solo `written_bytes`
     daba falsos positivos — el pipe del OS es de ~64 KB, así que tras
     el primer chunk `write_all` se bloquea hasta que mysql consume stdin.
   - Emite por `op-log` una línea "viva" (`progress_bar(sent, total, 24)`
     + bytes enviados + tiempo) que el frontend reescribe en sitio
     (`PROGRESS_PREFIX`).
   - Si `idle_ms >= IMPORT_IDLE_TIMEOUT (180 s)`, devuelve control →
     cancel.
6. **Cancel**:
   - `child.start_kill()` + `child.wait()`.
   - `writer.abort()`, `out_task.abort()`, `err_task.abort()`.
   - `wordpress::reset_database(docker, db_container, site)` → DB vacía.
   - Devuelve error: "import cancelado: sin actividad por 3 min. La DB
     se restauró vacía; reintenta la migración para importar de nuevo."
7. **Éxito**: `child.wait()` retorna 0, todas las tareas se cierran.

### E. Migración de un proyecto al sistema actual (`migrate_site`)

1. Verifica `site.public_dir().exists()`.
2. `wordpress::sync_mu_plugins(site)` (re-inyecta mailpit + auto-login).
3. `[1/6]` `docker::ensure_db` + `wordpress::create_database` (idempotente).
4. `[2/6]` `ssl::generate(site)` si `site.services.nginx.ssl`.
5. `[3/6]` `docker::start_site(site)` (enciende container php + vhost +
   nginx + dnsmasq wildcard).
6. `[4/6]` `wordpress::wp_config_create(docker, site, db_container)` —
   regenera `wp-config.php` con las credenciales del panel.
7. `[5/6]` Si `latest_dump(site)` (último *.sql en `app/sql/` por mtime):
   - `migrate::import_dump(app, docker, site, db_container, dump)`.
   - `migrate::fix_site_url(docker, site)` (con `--skip-plugins
     --skip-themes` para no depender de plugins).

   Si no hay dump: `note = "No había dump en app/sql/: el sitio arranca
   con la base de datos vacía."`.
8. `[6/6]` `fix_site_url` (si se importó la DB): `wp option update home`
   + `wp option update siteurl` con la URL efectiva del panel.
9. Actualiza `migration_pending = false`, `last_migrated_at = now`.
10. `Migration { site: updated, note }`.

### F. Migración legacy del container DB (una vez por container)

- `docker::migrate_db_to_volume(container, host_dir, datadir_in)`:
  1. Si `host_dir` está vacío, `docker cp {container}:{datadir_in}/.
     {host_dir}` (CLI, no bollard — extraer un dir por tar stream es
     complejo).
  2. `remove_container(container, force:true)`.
  3. `ensure_db` recrea el container con bind al `host_dir`.
- Esta migración es **una-sola-vez** por container. Después,
  `db_has_volume` corta por la rama "ya migrado".

## Variantes

- **Postgres (sin gate barato)**: `autodump::write_counter` devuelve `None`
  para Postgres → siempre se hace dump + hash (no hay
  `Innodb_rows_*`).
- **Worktree con DB copiada**: `worktree::create_worktree` vlca el padre
  con `dump_bytes` + escribe `from-parent-{stamp}.sql` + `import_dump`.
- **Worktree con DB compartida**: NO crea esquema, NO copia. Fija
  `WP_HOME`/`WP_SITEURL` con `wp config set ... --type=constant` en el
  wp-config del worktree (la DB del padre intacta).
- **Snapshot / clone**: `snapshot::create_snapshot` usa `export_db_to`
  (path arbitrario). `clone::create_clone` usa `import_dump` para restaurar
  el dump del snapshot.
- **DROP DATABASE no permitido por permisos**: usar `mysql -uroot` (root
  del container) — el container DB arranca con `MYSQL_ROOT_PASSWORD=panel`
  y `MYSQL_ROOT_HOST=%`.
- **Migración con dump de LocalWP (.local)**: las URLs en el dump apuntan
  al dominio viejo. `fix_site_url` las repunta al panel tras el import.
- **Sidecar imports** (`imported.sql`): la migración los pilla igual
  (`latest_dump` busca cualquier `*.sql`, no solo `db-*.sql`).

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `~/.config/wordpress-panel/db-data/{container}/` | bind ro en `panel-{db}-{ver}` (`/var/lib/mysql` o `/var/lib/postgresql/data`) | `docker cp` (migración legacy) |
| `~/panel-wp/{slug}/app/sql/db-*.sql` | listado por mtime | `backup::export_db`, `backup::export_db_to` |
| `~/panel-wp/{slug}/app/sql/imported.sql` | `latest_dump` (cualquier `*.sql`) | `localwp::import_site` copia desde `app/sql/local.sql` |
| `~/panel-wp/{slug}/app/sql/from-parent-{ts}.sql` | `latest_dump` (worktree) | `worktree::create_worktree` |
| `~/panel-wp/{slug}/app/public/wp-config.php` | bind ro en `wp-{id}` | `wp_config_create` (migración/worktree/clone) |
| `~/panel-wp/{slug}/app/public/wp-options` (home/siteurl) | lectura | `wp option update home/siteurl` (migración/clone) |
| MySQL/MariaDB `*_db` schema | (en runtime) | `CREATE DATABASE`, `DROP DATABASE`, `mysqldump`, `mysql` (PRAGMA + dump) |
| Postgres `*_db` schema | (en runtime) | `CREATE DATABASE`, `DROP DATABASE`, `pg_dump`, `psql` |

## Containers / servicios

- `panel-mysql-{ver}` / `panel-mariadb-{ver}` / `panel-postgres-{ver}`:
  un container compartido por (motor, versión). Todos los proyectos con
  ese par usan el mismo container, con esquemas separados.
- `wp-{site-id}`: php-fpm. Habla con la DB por `panel-net`:
  `--dbhost=panel-{db}-{ver}` (puerto 3306/5432).
- `panel-nginx`: publica el sitio (vhost con `fastcgi_pass {container}:9000`).

## Fallos y compensaciones

- **MySQL 8 init no abrió TCP**: `wait_db_ready` con `mysql -h127.0.0.1`
  hasta 60 s (`120 × 500 ms`).
- **Container DB legado sin nuestro bind**: `db_has_volume` distingue
  bind por `host_dir` vs. volume anónimo. `migrate_db_to_volume` +
  `docker cp` migra los datos.
- **dump corrupto en `/var/lib/mysql`** (heredado de LocalWP): entra en
  `migrate_db_to_volume` igual; el `docker cp` es lossless.
- **DB lista pero read-only**: `wait_db_ready` usaría `SELECT 1` que pasa
  — verificar la salud es responsabilidad de `mysqldump`.
- **Import colgado**: `IMPORT_IDLE_TIMEOUT = 180 s` + watchdog por
  `information_schema`. `reset_database` deja la DB vacía tras cancelar.
- **Pipe OS de ~64 KB bloquea el writer**: `write_all` se bloquea
  después del primer chunk. Solución: medir el tamaño real de la DB
  (no `written_bytes`) como indicador de vida.
- **`mysql` mete un warning por stderr al pasar la password en CLI**:
  `query_db_size` busca la línea que sea un entero, no el primer renglón.
- **Plugin que se cuelga al cargar durante la migración**: `fix_site_url`
  añade `--skip-plugins --skip-themes`.
- **`docker exec` no se cierra** (bollard): por eso `import_dump` usa
  `docker exec -i` por CLI.
- **Dump vacío**: `backup::dump_bytes` falla con `mysqldump no produjo
  salida para {dbname}`.
- **`wp core install` en migración**: NO se corre (la DB ya tiene datos).
  Solo `wp_config_create` + `fix_site_url`.

## UI / IPC / CLI / MCP disponibles

### IPC (`lib.rs`)

- `migrate_site(id)` → `Migration` (config actualizada + nota opcional).
- `export_db(id)` → `String` (path del dump). Registra en `dumplog` con
  `source = "manual"`.
- `repair_autologin(id)` → `SiteConfig` (re-inyecta mu-plugins).
- `system_status()` → `SystemStatus` (red, docker, etc.).

### UI (`src/lib/components/ProjectDetail.svelte`, `src/routes/`)

- Tab `svc` del proyecto: "Ver base de datos (Adminer)" (`open_adminer`),
  "Exportar base de datos" (`export_db`).
- `/` master-detail: el icono de cada proyecto muestra el estado
  (running/stopped/migrationPending); las cards "Migrar y encender"
  disparan `migrate_site` con la `OpConsole` viendo el progreso
  (`op-log`).
- `/import-localwp` y modal `ImportProjectModal.svelte` dejan proyectos
  `migration_pending` para migración posterior.
- `/dumps` (ver ficha 06) lista los volcados (`dump_log`) y permite
  limpiar el log.

### CLI (`scripts/wordpress-panel-cli.sh`)

- `wordpress-panel-cli snapshot create <label>` — crea snapshot (que
  internamente exporta DB).
- `wordpress-panel-cli snapshot clone <id>` — clona desde snapshot.

### MCP (`mcp/server.mjs`)

- `create_snapshot`, `clone_snapshot`, `list_snapshots`, `delete_snapshot`.

## Tests

- `integration_tests::import_disconnected_marks_pending` — fluye desde
  carpeta traída de otro sistema hasta `migration_pending = true` con
  `tauri::test::mock_app()`.
- `backup::tests::rotate_conserva_los_n_mas_recientes_e_ignora_ruido` —
  verifica que la rotación toca solo `db-*.sql` y deja `imported.sql`,
  `local.sql`.
- `backup::tests::rotate_no_borra_si_hay_menos_o_igual_que_keep`.
- `dumplog::tests::clean_por_fecha_borra_anteriores`,
  `clean_por_db_borra_solo_esa`, `clean_combinado_es_interseccion`,
  `clean_sin_filtros_borra_todo`.
- `config::tests::parse_db_name` (implícito en `reconstruct_config`):
  extrae `DB_NAME` del `wp-config.php`.

## Limitaciones

- **Un container DB por (motor, versión)**: dos proyectos con MySQL 8.0
  comparten el mismo container; los datos están separados por esquema.
- **`MYSQL_ROOT_HOST=%`** permite conexiones root desde `panel-net` — solo
  está expuesto en la red interna.
- **Postgres** no tiene gate barato (`Innodb_rows_*`); siempre se hace
  dump + hash (puede gastar CPU con DBs grandes ociosas).
- **Una sola DB por proyecto** (no soporta multi-DB en un mismo proyecto).
- **El import NO respeta el tamaño del datadir**: para dumps de +1 GB,
  el `COMMIT` final puede tardar segundos en un fsync grande. El watchdog
  mide el tamaño real, así que no cancela mientras la DB crece.
- **`dump_bytes` con DB muy grande**: capta TODO en memoria (`exec_capture`).
  Para dumps de >500 MB, considerar streaming a disco antes si se observa
  OOM.
- **`migrate_db_to_volume` usa `docker cp` (CLI)** — requiere `docker` en
  PATH y permisos de dockerd.

## Invariantes a NO romper

- **DB datadir = `config_dir/db-data/{container}` bindeado a
  `DbType::datadir()`** — sobre capas anónimas esto es volátil.
- **`wait_db_ready` con `-h127.0.0.1`** — `-h localhost` o socket local
  puede dar un falso positivo en el init de MySQL.
- **`--skip-dump-date`** en `mysqldump` — el auto-dump compara por hash;
  si la fecha cambia, vuelca siempre.
- **Import por `docker exec -i` (CLI), no bollard** — bollard se cuelga
  con `exec_stdin`.
- **`reset_database` SIEMPRE tras un import cancelado** — un dump a
  medias corrompe la DB.
- **`exclude dir` solo `db-*.sql`** — `rotate_dumps` no toca los
  `imported.sql`, `local.sql`, `from-parent-*.sql`.
- **`migrate::fix_site_url` con `--skip-plugins --skip-themes`** — un
  plugin que se cuelga al cargar bloquea la migración.
- **`MYSQL_ROOT_HOST=%`** — sin esto, el php-fpm del proyecto no podría
  conectar desde `panel-net`.

## Recomendaciones breves (rebuild)

- **Container DB compartido por (motor, versión)**, no por proyecto.
- **El datadir SIEMPRE bindeado** a `config_dir/db-data/{container}`.
- **Cada esquema por proyecto** se llama `"{slug}_db"` (slug con `-`
  reemplazado por `_`).
- **`DROP DATABASE`** antes de `CREATE DATABASE` cuando se hace rollback
  (`reset_database`).
- **Import por CLI `docker exec -i` con `pipes` + `writer` + `watchdog` +
  `drenaje` + `rollback`** — nunca vía bollard `exec_stdin`.
- **Gate barato solo en MySQL/MariaDB** vía `Innodb_rows_*`; Postgres
  confía en el hash.

## Fuentes primarias

- `src-tauri/src/docker.rs` — `DockerManager::ensure_db`, `db_has_volume`,
  `migrate_db_to_volume`, `wait_db_ready`, `db_container_name`,
  `db_data_dir`, `db_env`, `is_running`, `remove_container`, `exec`,
  `exec_as`, `exec_capture`.
- `src-tauri/src/wordpress.rs` — `create_database`, `reset_database`,
  `drop_database`, `wp_config_create`, `wp_core_install`,
  `sync_mu_plugins`.
- `src-tauri/src/backup.rs` — `dump_bytes`, `export_db`, `export_db_to`,
  `rotate_dumps`.
- `src-tauri/src/migrate.rs` — `migrate_site`, `run_migration`,
  `latest_dump`, `fix_site_url`, `import_dump`, `progress_bar`,
  `query_db_size`, `IMPORT_IDLE_TIMEOUT`, `IMPORT_CHUNK`, `IMPORT_TICK`,
  `IMPORT_PREAMBLE`, `IMPORT_EPILOGUE`.
- `src-tauri/src/autodump.rs` — `AutoDump`, `watch`, `write_counter`,
  `latest_dump_hash`, `persist`, `hash_bytes`, `POLL`.
- `src-tauri/src/snapshot.rs` — `create_snapshot`, `snapshot_dir`,
  `run` (interno).
- `src-tauri/src/clone.rs` — `create_clone`, `run` (interno).
- `src-tauri/src/worktree.rs` — `create_worktree` (worktree con DB copiada).
- `src-tauri/src/localwp.rs` — `pick_supported`, `read_raw`, `import_site`.
- `src-tauri/src/config.rs` — `DbType`, `DbService`, `datadir`,
  `service_prefix`, `image`, `parse_db_name`.
- `src-tauri/src/lib.rs` — `migrate_site`, `export_db`, `delete_site`,
  `import_localwp_site`, `import_disconnected_site`.
- `src/lib/api.ts` — `migrateSite`, `exportDb`, `systemStatus`, etc.
- `src/lib/components/ProjectDetail.svelte` — botones Adminer y Export.
- `mcp/server.mjs` — `create_snapshot`, `clone_snapshot`, `list_snapshots`.
