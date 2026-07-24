# 06 — Backups, auto-dump y dumplog

> Trazabilidad UI/IPC/CLI/MCP ↔ backend para los tres tipos de dump de DB
> (auto / stop / manual), el dedup por hash, la retención de los `db-*.sql`,
> el log de volcados (`dump-log.jsonl`), los puntos de guardado (snapshots:
> código + dump) y los clones temporales.

## Resultado para el usuario

El usuario tiene siempre un dump fresco de cada proyecto activo en
`app/sql/db-*.sql` (rotado a 3 últimos), aunque la máquina se apague de
golpe. Puede revisar el log de volcados (`/dumps`) para saber cuándo se
generó cada uno, con qué tamaño y por qué motivo (auto / stop / manual),
y limpiar el log por fecha o por DB. Además puede crear un **punto de
guardado** de un proyecto (código sin uploads/cache/wp-config + dump
completo) y levantar un **clone temporal** desde él para probar algo sin
tocar el proyecto principal.

## Precondiciones

- Container DB del proyecto corriendo (`docker::is_running(db_container)`).
- `app/sql/` existe (lo crea `wordpress::create_dirs`).
- `~/.config/wordpress-panel/dump-log.jsonl` se crea solo al primer
  `dumplog::append`.
- Para snapshots: suficiente espacio en `~/panel-wp/{slug}/snapshots/{id}/`.

## Flujo feliz

### A. Dump de la DB (`backup::dump_bytes`)

1. `dump_bytes(docker, site)`:
   - `mysqldump -uroot -ppanel --single-transaction --no-tablespaces
     --skip-dump-date {db_name}` ejecutado en el container DB.
   - stdout capturado por `docker::exec_capture` (no `wp db export` porque
     el cliente mariadb del container php falla la verificación del cert
     autofirmado de MySQL 8).
   - `--skip-dump-date` para que el hash sea estable entre volcados sin
     cambios.
2. `export_db_to(docker, site, dest)` lo escribe a un path arbitrario.
3. `export_db(docker, site)` lo escribe a `app/sql/db-{stamp}.sql`
   (stamp = `Utc::now().format("%Y%m%d-%H%M%S")`).
4. `rotate_dumps(site, keep=3)`:
   - Lista `db-*.sql` en `app/sql/` por mtime.
   - Si `len > keep`, ordena de más nuevo a más viejo y borra los `keep+1..n`.
   - NO toca `imported.sql`, `local.sql`, `from-parent-*.sql`, etc.
5. Single-transaction (no lock); para InnoDB es consistente.

### B. Auto-dump (`autodump`)

1. `lib.rs::start_site` engancha el watcher: `autodump::AutoDump::start(site)`.
   - Estado Tauri (`manage(AutoDump::default())` en `run`).
   - `HashMap<site_id, JoinHandle<()>>` (no `DockerManager` porque se
     reconstruye en cada comando).
   - Idempotente (`if contains_key { return }`).
2. `lib.rs::run` (en `setup`) engancha watchers para los proyectos que ya
   estaban activos (containers que sobrevivieron a la sesión anterior).
3. `watch(site)`:
   - `last_hash = latest_dump_hash(site)` para sembrar la línea base
     desde el último dump en disco (no desde el estado vivo):
     si la DB cambió mientras el panel estaba cerrado, el primer
     sondeo lo vuelca.
   - Bucle cada `POLL = 20s`:
     - Si `!is_running(db_container)` → `continue`.
     - **Gate barato**: `write_counter` para MySQL/MariaDB
       (`SHOW GLOBAL STATUS WHERE Variable_name IN
       ('Innodb_rows_inserted','Innodb_rows_updated','Innodb_rows_deleted')`).
       - Postgres devuelve `None` (no hay equivalente).
     - Si `last_writes == Some(writes)` → no volcar (inactivo).
     - `dump_bytes` + `hash_bytes` (DefaultHasher).
     - Si `last_hash == Some(hash)` → no escribir (igual al último).
     - `persist(site, dump)`:
       - `app/sql/db-{stamp}.sql` (segundos en UTC).
       - `dumplog::append(site, &dest, "auto")`.
       - `rotate_dumps(site, 3)`.
4. `lib.rs::stop_site` llama `autodump::stop(id)` ANTES de exportar (el
   stop export es la última copia).
5. Borrado del container DB (proyecto apagado): el watcher sigue
   sondeando pero `is_running` corta rápido; cuando reaparece un
   container nuevo, retoma.

### C. Export al detener (`stop_site`)

1. `docker::stop_site(site, others)`:
   - Si `is_running(container)`:
     - `backup::export_db(docker, site)` → `app/sql/db-{stamp}.sql` (best-effort).
     - `dumplog::append(site, &path, "stop")` (best-effort).
     - `backup::rotate_dumps(site, 3)`.
     - `stop_container(container, t: 10)`.
   - `nginx::remove_vhost(site)` + `reload_nginx`.
   - `teardown_unused_shared(site, others)`.

### D. Export manual (`export_db` command)

1. `lib.rs::export_db(id)`:
   - `backup::export_db(docker, site)` → `app/sql/db-{stamp}.sql`.
   - `dumplog::append(site, &path, "manual")`.
   - Devuelve el path al usuario.

### E. Log de volcados (`dumplog`)

1. `dumplog::log_path` = `config_dir/dump-log.jsonl`.
2. `dumplog::append(site, file, source)`:
   - `DumpLogEntry { timestamp: YYYY-MM-DDTHH:MM:SSZ, site_id, site_name,
     db_name, file, bytes, source }`.
   - `std::fs::OpenOptions::new().create(true).append(true)` → una línea
     JSON por volcado.
   - Best-effort (`Result` se descarta `.ok()` por el caller).
3. `dumplog::read_all()` → `Vec<DumpLogEntry>`, más nuevas primero.
   - Tolera líneas corruptas (`filter_map`).
4. `dumplog::clean(before?, db_name?)`:
   - `removed = total - kept` donde `keep = NOT (by_date AND by_db)`.
   - `by_date = before.map(|b| e.timestamp < b).unwrap_or(true)` (si hay
     filtro, elimina entradas anteriores).
   - `by_db = db_name.map(|d| e.db_name == d).unwrap_or(true)` (intersección).
   - Sin filtros → borra todo.
   - NUNCA borra los `.sql`, solo el log.
   - Reescribe el archivo en orden cronológico (más viejo primero).
5. `/dumps` (`src/routes/dumps/+page.svelte`):
   - Lista entradas con sitio, DB, archivo, timestamp, tamaño, fuente.
   - "Borrar por fecha" → `clean_dump_log(before, null)`.
   - "Borrar por base" → `clean_dump_log(null, db)`.
   - "Borrar todo" → `clean_dump_log(null, null)`.

### F. Punto de guardado (`snapshot`)

1. `snapshot::create_snapshot(app, docker, site, label)`:
   - `[1/3]` `docker::ensure_db(&site.services.db)` (solo el engine).
   - Genera `id = Uuid::new_v4()`, crea `snapshots/{id}/`.
   - `[2/3]` `backup::export_db_to(site, db_path = snapshots/{id}/db.sql)`.
   - `[3/3]` Ejecuta `tar --zstd -cf snapshots/{id}/code.tar.zst` sobre
     `public/`:
     - Excluye FIJOS: `./wp-content/uploads`, `./wp-content/cache`,
       `./wp-config.php`, `./*.log`.
     - Excluye EXTRA: cada ruta en `site.snapshot_excludes` (persistidas
       por `set_snapshot_excludes` vía UI; normalizadas: `trim`,
       `trim_start_matches("./")`, `trim_matches('/')`).
     - `tar --zstd` (código Zstandard).
     - `tar -C public` y `.` (incluye `./` en cada path para que los
       `--exclude` casen).
     - Códigos: `0` ok, `1` avisos no fatales (típico "file changed as
       we read" en cache/logs de un WP activo); `2+` error real. Solo
       aborta en `2+`.
   - `SnapshotMeta { id, label, created_at, db_name, db_type, code_bytes,
     db_bytes, excludes }` → `snapshots/{id}/meta.json`.
2. `snapshot::list_snapshots(site)` → `Vec<SnapshotMeta>` (más nuevo
   primero).
3. `snapshot::delete_snapshot(site, snapshot_id)` → `remove_dir_all`.
4. `snapshot::detect_excludable(site)`:
   - Subcarpetas inmediatas de `wp-content` (excepto `uploads`, `cache`).
   - Carpetas de backup conocidas (`KNOWN_BACKUP_DIRS`):
     `wp-content/updraft` (UpdraftPlus), `wp-content/ai1wm-backups`
     (All-in-One), `wp-content/wpvividbackups` (WPvivid),
     `wp-content/backups-dup-lite` (Duplicator),
     `wp-content/backups-dup-pro` (Duplicator Pro),
     `wp-content/backuply` (Backuply), `wp-snapshots` (Duplicator).
   - Ordena por tamaño desc.
5. `snapshot::set_snapshot_excludes` (via `lib.rs::set_snapshot_excludes`):
   - Normaliza, ordena, dedup; persiste en `SiteConfig.snapshot_excludes`.

### G. Clone temporal (`clone`)

1. `clone::create_clone(app, docker, parent_id, snapshot_id)`:
   - Lee `meta.json` del snapshot.
   - Deriva `slug = slugify(label)`, `path = ~/panel-wp/{parent}-{slug}`,
     `domain = {slug}.test` (libre).
   - `site` con `clone_of: Some(CloneInfo { parent_id, parent_dirname,
     snapshot_id, created_at })`.
   - `[1/8]` `create_dirs` + `write_php_ini` + `write_site_config`.
   - `[2/8]` `tar --zstd -xf snapshots/{id}/code.tar.zst -C public`.
   - `[3/8]` `docker::ensure_db` + `create_database`.
   - `[4/8]` `migrate::import_dump(app, docker, site, db_container, db.sql)`.
   - `[5/8]` `sync_mu_plugins`.
   - `[6/8]` `ssl::generate` (si SSL).
   - `[7/8]` `docker::start_site`.
   - `[8/8]` `wp_config_create` + `fix_site_url`.
2. El vhost (`nginx::render_vhost`) detecta `clone_of` y añade:
   - `location ^~ /wp-content/uploads/ { root /srv/projects/{clone}/app/public;
     try_files $uri @uploads_base; }`
   - `location @uploads_base { root /srv/projects/{parent}/app/public;
     try_files $uri =404; }`
   - Uploads viejos del padre (ro) accesibles vía fallback; uploads
     nuevos del clone (rw).
3. `delete_site` del clone limpia normal (su schema se borra por
   `drop_database`).

### H. Distinción de backups

| Tipo | Origen | Persiste en | Lo crea |
|---|---|---|---|
| **Auto-dump** | `autodump.rs` (cada 20 s si el gate cambia) | `app/sql/db-*.sql` | `autodump::persist` |
| **Export al detener** | `docker::stop_site` | `app/sql/db-*.sql` | stop site |
| **Export manual** | command `export_db` | `app/sql/db-*.sql` | usuario |
| **Snapshot** | `snapshot::create_snapshot` | `snapshots/{id}/db.sql` + `code.tar.zst` + `meta.json` | usuario (UI/CLI/MCP) |
| **Clone (snapshot)** | `clone::create_clone` | carpeta nueva del proyecto | usuario (UI/CLI/MCP) |
| **DB durable** | `docker::ensure_db` (bind-mount) | `config_dir/db-data/{container}/` | siempre, automático |
| **Sidecar disconnect** | `delete_site(deleteFolder=false)` | `config.disconnected.json` en la carpeta | `delete_site` |

## Variantes

- **Postgres**: `autodump::write_counter` devuelve `None` → siempre se
  hace dump + hash (más caro).
- **DB inactiva**: el gate barato corta el `dump_bytes` (no se hace
  trabajo inútil).
- **Hash colisión** (improbable pero posible con DefaultHasher): se
  escribiría un dump igual → `rotate_dumps` lo deja en `keep=3`.
- **Snapshot de WP activo**: tar puede avisar de "file changed as we
  read" (cache/logs); código `1` se considera no fatal.
- **Snapshot con snapshot_excludes vacío**: el aviso indica "(excluyendo
  uploads y caché)"; con extras, "(excluyendo uploads, caché y N
  ruta(s) del proyecto)".
- **Clone con mismo nombre**: `find_free_slot` prueba `base`, `base-1`,
  `base-2`, … (hasta 99) y cae a UUID corto.
- **DB no tiene dump**: `migrate::latest_dump` devuelve `None` y la
  migración sigue con la DB vacía (`note` = "No había dump en app/sql/
  …").

## Datos leídos / escritos

| Dato | Lectura | Escritura |
|---|---|---|
| `~/panel-wp/{slug}/app/sql/db-*.sql` | `latest_dump`, `latest_dump_hash`, `rotate_dumps` | `backup::export_db`, `autodump::persist`, `stop_site` |
| `~/panel-wp/{slug}/app/sql/imported.sql` | `migrate::latest_dump` (cualquier `*.sql`) | `localwp::import_site` (copia desde LocalWP) |
| `~/panel-wp/{slug}/app/sql/from-parent-{ts}.sql` | `migrate::latest_dump` | `worktree::create_worktree` (BD copiada) |
| `~/panel-wp/{slug}/snapshots/{id}/db.sql` | `clone::create_clone` | `snapshot::create_snapshot` |
| `~/panel-wp/{slug}/snapshots/{id}/code.tar.zst` | `clone::create_clone` (tar -xf) | `snapshot::create_snapshot` (tar --zstd -cf) |
| `~/panel-wp/{slug}/snapshots/{id}/meta.json` | `clone::create_clone` | `snapshot::create_snapshot` |
| `~/.config/wordpress-panel/dump-log.jsonl` | `dumplog::read_all` | `dumplog::append`, `dumplog::clean` |
| `SiteConfig.snapshot_excludes` | `snapshot::create_snapshot` | `set_snapshot_excludes` |

## Containers / servicios

- `panel-{db}-{ver}` compartido (para dump e import).
- `wp-{site-id}` (no publica puertos).
- `panel-nginx` (para clones que llevan vhost).

## Fallos y compensaciones

- **DB no corriendo**: `dump_bytes` falla con `"la base de datos de
  '{name}' no está encendida"`. Auto-dump continúa (skip).
- **Sin espacio en disco**: `std::fs::write` falla → `dumplog::append`
  falla silenciosamente (best-effort). El auto-dump loguea a
  `eprintln` (no rompe el watcher).
- **WP super-grande con muchos archivos**: el tar puede tardar. El aviso
  en el log (`[3/3]`) lo deja claro.
- **Snapshot Exclusiones con prefijo `./` o `/`**: `set_snapshot_excludes`
  normaliza (`trim`, `trim_start_matches("./")`, `trim_matches('/')`),
  ordena y dedup.
- **Snapshot con archivo en uso**: `tar` avisa, exit code 1, el snapshot
  queda válido (carpeta no se elimina).
- **tar falla con exit 2+**: `std::fs::remove_dir_all(&dir).ok()` y
  `Err("tar falló (código {code}) al crear el snapshot de código: …")`.
- **Clone con dump corrupto**: `migrate::import_dump` lo trata;
  `reset_database` deja la DB vacía.
- **Camino del sidecar ya en uso**: `disconnected_config_path` se
  construye en `import_disconnected`; si existe, se borra con `.ok()`.
- **dumplog.append falla**: el caller lo descarta (`.ok()`); el `.sql`
  ya está en disco.
- **Snapshot vacío**: `db_bytes == 0 && code_bytes == 0` se reporta tal
  cual en `meta.json`.

## UI / IPC / CLI / MCP disponibles

### IPC (`lib.rs`)

- `export_db(id)` → `String` (path del dump).
- `dump_log()` → `Vec<DumpLogEntry>`.
- `clean_dump_log(before?, db_name?)` → `usize` (cantidad eliminadas).
- `create_snapshot(id, label)` → `SnapshotMeta`.
- `list_snapshots(id)` → `Vec<SnapshotMeta>`.
- `delete_snapshot(id, snapshot_id)` → `()`.
- `detect_excludable(id)` → `Vec<ExcludableEntry>`.
- `set_snapshot_excludes(id, excludes)` → `()`.
- `create_clone(parent_id, snapshot_id)` → `SiteConfig`.

### UI (`src/lib/components/ProjectDetail.svelte`, `src/routes/`)

- Tab `svc` del proyecto: botón "Exportar base de datos" → `export_db`.
- Tab `snapshots` del proyecto:
  - Form para crear (`create_snapshot`).
  - Lista con `label`, fecha, tamaño, código+db.
  - Botón "Borrar" por snapshot (`delete_snapshot`).
  - Botón "Clonar" → `create_clone` con OpConsole.
  - Form para excluir rutas (`set_snapshot_excludes`).
  - `detect_excludable` lista candidatas (UpdraftPlus, AIOWM, etc.).
- `/dumps` (`src/routes/dumps/+page.svelte`) — log + limpieza.

### CLI (`scripts/wordpress-panel-cli.sh`)

- `wordpress-panel-cli snapshot {list|create|delete|clone}`:
  - `list` → tabla con ID, LABEL, FECHA, TAMAÑO.
  - `create <etiqueta>` → `CreateSnapshot`.
  - `delete <snapshot_id>` → `DeleteSnapshot`.
  - `clone <snapshot_id>` → `CreateClone`.
- `wordpress-panel-cli open admin | site` (legado).

### MCP (`mcp/server.mjs`)

- `list_snapshots(project)`.
- `create_snapshot(project, label)`.
- `delete_snapshot(project, snapshotId)`.
- `clone_snapshot(project, snapshotId)`.

## Tests

- `backup::tests::rotate_conserva_los_n_mas_recientes_e_ignora_ruido` —
  fija `SiteConfig` con `db_type: Mysql, version: 8.0, db_name: "test"`,
  crea 5 dumps (`db-1..db-5.sql`) + `imported.sql` + `local.sql`,
  verifica que `rotate_dumps(_, 3)` deja `db-3..db-5.sql` + el ruido.
- `backup::tests::rotate_no_borra_si_hay_menos_o_igual_que_keep`.
- `dumplog::tests::clean_por_fecha_borra_anteriores`,
  `clean_por_db_borra_solo_esa`, `clean_combinado_es_interseccion`,
  `clean_sin_filtros_borra_todo`.
- `integration_tests::import_disconnected_marks_pending` (cubre el camino
  completo de re-importación con `tauri::test::mock_app()`).

## Limitaciones

- **Auto-dump solo escribe si hay un cambio**: si la DB está inactiva,
  el coste es solo el `SHOW GLOBAL STATUS` cada 20 s (cheap).
- **`hash_bytes` usa `DefaultHasher`** (no criptográfico). Suficiente para
  dedup, no para comparar entre hosts.
- **Rotación es local al proyecto**: cada proyecto rotativa a 3 dumps.
- **Snapshots NO se rotan**: el usuario debe borrar antiguos vía
  `delete_snapshot` o la UI.
- **`snapshot_excludes` solo afecta a snapshots**, no al auto-dump ni al
  export (el exclude GLOBAL se hace explícitamente en `tar`).
- **`dump_log` se queda pequeño para siempre**: no hay rotación del JSONL
  (es lineal y típicamente <10 KB/mes por proyecto).
- **No hay backup incremental**: cada dump es completo.
- **`dump_bytes` carga todo en memoria**: `exec_capture` retorna
  `Vec<u8>`. Para dumps de >500 MB usar streaming a disco directo si
  hay memoria limitada.
- **`snapshot` no se cifra** (no es su objetivo en dev).
- **Sidecar disconnect** (`config.disconnected.json`) solo se crea en
  `delete_site`; no confundir con `config.json` (el activo).

## Invariantes a NO romper

- **`backup::dump_bytes` con `--skip-dump-date`** — sin esto, el auto-dump
  vuelca siempre.
- **`autodump::write_counter` con `Innodb_rows_*`** — el gate barato
  evita dumps inútiles.
- **`dumplog::clean` solo borra el log, nunca los `.sql`** — la nota
  está en la firma del módulo.
- **`rotate_dumps` solo borra `db-*.sql`** — los `imported.sql`,
  `local.sql`, `from-parent-*.sql` son preservados.
- **`snapshot::create_snapshot` RE-INYECTA el mu-plugin** — en `clone.rs`
  también (`sync_mu_plugins`).
- **`snapshot::create_snapshot` aborta solo en `tar` exit code ≥ 2** —
  el `1` (file changed) es esperado y no fatal.
- **Sidecar disconnect** se llama `config.disconnected.json` EXACTAMENTE
  (literal `DISCONNECTED_CONFIG` en `config.rs`).
- **`STOP site` export ANTES de parar el container** (mientras la DB
  vive).
- **Auto-dump watcher se engancha al `start_site`, no al `create_site`** —
  un proyecto creado pero no encendido no debe disparar watcher.
- **`dumplog::append` se llama desde `autodump::persist`, `stop_site`,
  `export_db`, NO desde `snapshot::create_snapshot`** (esto es snapshot,
  no `db-*.sql`).

## Recomendaciones breves (rebuild)

- Distingue siempre los 3 clases de dump por `source` (`auto` / `stop` /
  `manual`) en `dumplog`.
- **Snapshot = código + `db.sql` separado**, NO es un dump de `app/sql/`.
- **DB durable es por container, no por proyecto** — un bind compartido
  para todos los proyectos del mismo (motor, versión).
- **Sidecar disconnect** es la diferencia entre "borrar" y "olvidar".
- **Auto-dump persiste el hash del último dump**, no del estado vivo.
- **El `dump_log` es SOLO para revisión**: la fuente de verdad de los
  `.sql` está en `app/sql/`.

## Fuentes primarias

- `src-tauri/src/backup.rs` — `dump_bytes`, `export_db`, `export_db_to`,
  `rotate_dumps`.
- `src-tauri/src/autodump.rs` — `AutoDump`, `start`, `stop`, `watch`,
  `write_counter`, `latest_dump_hash`, `persist`, `hash_bytes`, `POLL`.
- `src-tauri/src/dumplog.rs` — `DumpLogEntry`, `append`, `read_all`,
  `clean`, `log_path`.
- `src-tauri/src/snapshot.rs` — `create_snapshot`, `run`, `list_snapshots`,
  `delete_snapshot`, `detect_excludable`, `snapshot_dir`, `KNOWN_BACKUP_DIRS`.
- `src-tauri/src/clone.rs` — `create_clone`, `run`, `find_free_slot`,
  `slugify`.
- `src-tauri/src/lib.rs` — `export_db`, `dump_log`, `clean_dump_log`,
  `create_snapshot`, `list_snapshots`, `delete_snapshot`,
  `detect_excludable`, `set_snapshot_excludes`, `create_clone`,
  `start_site`, `stop_site`.
- `src-tauri/src/docker.rs` — `stop_site` (export-al-detener),
  `teardown_unused_shared`.
- `src-tauri/src/nginx.rs` — `render_vhost` (clone uploads fallback).
- `src-tauri/src/migrate.rs` — `fix_site_url`, `import_dump`,
  `latest_dump`.
- `src/lib/api.ts` — `exportDb`, `dumpLog`, `cleanDumpLog`,
  `createSnapshot`, `listSnapshots`, `deleteSnapshot`,
  `detectExcludable`, `setSnapshotExcludes`, `createClone`.
- `src/lib/types.ts` — `DumpLogEntry`, `SnapshotMeta`, `ExcludableEntry`.
- `src/lib/components/ProjectDetail.svelte` — tab `svc`, tab `snapshots`.
- `src/routes/dumps/+page.svelte` — log + limpieza.
- `mcp/server.mjs` — `list_snapshots`, `create_snapshot`,
  `delete_snapshot`, `clone_snapshot`.
- `scripts/wordpress-panel-cli.sh` — `snapshot`, `git`, `worktree`.
