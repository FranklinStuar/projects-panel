# 09 · Puntos de guardado y clones temporales

Cubre la creación, listado y borrado de puntos de guardado
(`snapshot.rs`) por proyecto, y la creación/eliminación de clones
temporales a partir de un punto de guardado (`clone.rs`).

> **Importante**: un snapshot es **código + dump + meta**; **no se restaura
> in-place** sobre el proyecto origen. Para volver a un estado anterior
> desde un snapshot, el camino es: crear un clone desde ese snapshot,
> trabajar en el clone, y aplicar manualmente lo que se quiera mantener.
> Si lo que se busca es «rollback», primero crear un snapshot actual
> (para no perder cambios), luego clonar el viejo y mergear.

## Resultado para el usuario

- **Crear un punto de guardado** de un proyecto (código + dump SQL).
- **Listar** los puntos de guardado del proyecto (más reciente primero).
- **Detectar y persistir exclusiones** extra (carpetas de backups
  conocidos: UpdraftPlus, All-in-One WP Migration, WPvivid, Duplicator,
  Backuply) y rutas propias del usuario.
- **Borrar** un punto de guardado del disco.
- **Clonar** un punto de guardado en un proyecto nuevo (independiente) con
  un sub-dominio y un esquema de DB propios; compartir el engine DB y
  nginx con el resto.
- **Eliminar** un clone (igual que cualquier proyecto: `delete_site`).

## Precondiciones

- El proyecto debe existir en `~/panel-wp/{slug}/config.json` (puede estar
  apagado: el motor DB se enciende on-demand solo para el dump; el engine
  DB compartido puede requerir docker).
- Para **crear el dump** se necesita el motor de DB correspondiente
  (`panel-mysql-{ver}` o equivalente) corriendo. `snapshot::create_snapshot`
  llama `docker::ensure_db` (`docker.rs`), que lo arranca si no está.
- El **clone** requiere además que `wp-{id}` arranque (crea su propio
  container php y vhost).
- **Exclusiones por proyecto** se persisten en `config.json` y se aplican
  en cada snapshot posterior (`snapshot_excludes`).

## Flujo feliz (numerado)

### Crear un snapshot

1. UI: tab «Puntos de guardado» en `ProjectDetail.svelte` →
   `createSnapshot(id, label)` (`ProjectDetail.svelte:170`) →
   `api.createSnapshot(id, label.trim())` → `create_snapshot`
   (`lib.rs::691`) → `snapshot::create_snapshot` (`snapshot.rs::76`).
2. `snapshot::run` (`snapshot.rs::91`) emite por `op-log`:
   - `▶ Punto de guardado «{label}» — «{site.name}»`.
   - `[1/3] Arrancando motor de base de datos…` + `✓ Motor listo.`
   - `[2/3] Exportando base de datos «{db_name}»…` → `backup::export_db_to`
     (`backup.rs::52`) corre `mysqldump --single-transaction --no-tablespaces
     --skip-dump-date {db}` dentro del container DB por socket local (sin
     TLS) y escribe `snapshots/{uuid}/db.sql`.
   - `[3/3] Comprimiendo código fuente (excluyendo uploads, caché…)…`:
     `tar --zstd -cf snapshots/{uuid}/code.tar.zst --exclude=./wp-content/uploads
     --exclude=./wp-content/cache --exclude=./wp-config.php --exclude=./*.log
     [--exclude=./{snapshotExcludes…}] -C {public} .`.
   - `✓ Punto de guardado listo — total en disco: {fmt_bytes(total)}.`
3. `meta.json` se escribe con `SnapshotMeta { id, label, createdAt,
   dbName, dbType, codeBytes, dbBytes, excludes[] }`.
4. Errores no fatales de `tar` (exit 1, típico «file changed as we read
   it» con WP activo) se registran como `⚠` y no abortan el snapshot
   (`snapshot.rs::158-174`). Códigos 2+ sí abortan y borran el dir.

### Listar snapshots

1. `list_snapshots(id)` (`lib.rs::703`) →
   `snapshot::list_snapshots(site)` (`snapshot.rs::210`):
   - Lee `~/panel-wp/{slug}/snapshots/*/meta.json`, parsea
     `SnapshotMeta`, ordena por `created_at` desc.
2. La UI muestra cada uno con id, label, fecha, tamaño formateado
   (`fmtBytes` en `ProjectDetail.svelte`).

### Exclusiones

1. UI: panel plegable «Exclusiones» →
   `detectExcludable(id)` (`lib.rs::717`) →
   `snapshot::detect_excludable(site)` (`snapshot.rs::240`):
   - Lee subcarpetas inmediatas de `wp-content/` (excluyendo `uploads` y
     `cache` que ya están forzadas), calcula tamaño (`dir_size` recursivo).
   - Añade carpetas de backup conocidas fuera de `wp-content` (p. ej.
     `wp-snapshots`).
   - Marca `known = true` y `label` (UpdraftPlus, etc.) si coincide con
     `KNOWN_BACKUP_DIRS` (`snapshot.rs::56`):
     `wp-content/updraft`, `wp-content/ai1wm-backups`,
     `wp-content/wpvividbackups`, `wp-content/backups-dup-lite`,
     `wp-content/backups-dup-pro`, `wp-content/backuply`,
     `wp-snapshots`.
   - Ordena por tamaño desc.
2. La UI pinta cada candidato con checkbox + tamaño; permite añadir
   rutas manuales (`addManualExclude`). `saveExcludes` →
   `set_snapshot_excludes(id, excludes)` (`lib.rs::724`) persiste
   `SiteConfig::snapshot_excludes` (limpio: trim, sin `./` ni `/` inicial/
   final, dedup, sort).

### Borrar snapshot

1. UI confirma → `api.deleteSnapshot(id, snapshotId)` →
   `delete_snapshot(id, snapshot_id)` (`lib.rs::710`) →
   `snapshot::delete_snapshot(site, snapshot_id)` (`snapshot.rs::317`):
   `remove_dir_all` de `snapshots/{id}/`. **No** toca los `app/sql/db-*.sql`
   del proyecto (esos siguen su propia rotación).

### Clonar desde snapshot

1. UI: confirma en `cloneFromSnapshot(snapshotId)`
   (`ProjectDetail.svelte:200`) → `api.createClone(id, snapshotId)` →
   `create_clone` (`lib.rs::739`) → `clone::create_clone`
   (`clone.rs::20`).
2. `clone::run` (`clone.rs::35`) emite por `op-log`:
   - Carga el padre (`config::find_site`) y el snapshot
     (`snapshot_dir` + parse `meta.json`).
   - Deriva slug/dominio: `base_slug = "{parent_dirname}-{label_slug}"`,
     desambiguación con `-N` o UUID corto en `find_free_slot`
     (`clone.rs::211`). El nombre del clone es la `meta.label`.
   - `[1/8] Preparando carpeta del clone…`: `create_dirs`,
     `write_php_ini`, `write_site_config` con `clone_of` poblado.
   - `[2/8] Extrayendo código del snapshot…`: `tar --zstd -xf
     code.tar.zst -C {public}`; crea `wp-content/uploads/` vacío (rw).
   - `[3/8] Creando base de datos del clone…`: `ensure_db` +
     `create_database({clone}_db)`.
   - `[4/8] Importando base de datos ({mb} MB)…`: `migrate::import_dump`
     con el `db.sql` del snapshot (reusa el flujo de migración con
     pragmas y watchdog).
   - `[5/8] Sincronizando plugins del panel…`: `sync_mu_plugins`.
   - `[6/8] Generando certificado SSL…`: `ssl::generate` si SSL activo.
   - `[7/8] Arrancando el clone (container PHP + nginx)…`:
     `docker::start_site`.
   - `[8/8] Configurando WordPress del clone…`: `wp_config_create` +
     `fix_site_url` (ajusta `home`/`siteurl` al dominio del clone).
3. La UI navega al detalle del clone (`onSelect(cloneSite.id)`). El
   dashboard lo anida bajo su padre (`+page.svelte` agrupa clones por
   `cloneOf.parentId` cuando el padre está parado).

## Variantes y casos borde

- **Sin dump en el snapshot**: `db.sql` no se extrae (paso 4 falla). El
  flujo aborta; el usuario debe tener un dump previo (todos los snapshots
  lo crean).
- **Carpeta destino ocupada**: `find_free_slot` prueba `base`, `base-1`,
  …, `base-99`, y como fallback usa un UUID corto (`clone.rs::234`).
- **`tar` con archivo cambiando durante la copia** (caché, logs):
  exit 1, no fatal. El snapshot queda válido pero el `meta.json` se
  escribe igualmente; el `code.tar.zst` puede no incluir los últimos
  cambios de ese archivo concreto.
- **Duplicado de `id`**: el clone genera un UUID nuevo (`Uuid::new_v4`),
  no reutiliza el del padre.
- **Eliminar el padre**: borrar el padre (con o sin carpeta) no elimina
  los clones automáticamente: cada clone es un `SiteConfig` independiente
  con su propio `app/public`. `load_all_sites` los sigue mostrando hasta
  borrarlos uno a uno. Los clones **no** se listan al eliminar el padre.
- **Mismo `parent_id` para varios clones**: permitido; cada uno lleva su
  `CloneInfo { parent_id, parent_dirname, snapshot_id, created_at }`.
- **Uploads del padre en el clone** (nginx fallback):
  `nginx::render_vhost` (`nginx.rs:71-88`) añade para clones el bloque
  `location ^~ /wp-content/uploads/ { try_files $uri @uploads_base; }` +
  `location @uploads_base { root /srv/projects/{parent}/app/public; }`. Así
  los archivos viejos (en `app/public` del padre) son visibles ro, y los
  nuevos del clone (en su propio `app/public`) tienen precedencia por el
  prefijo `^~`.
- **Multi-clone anidado**: no soportado; un clon de un clon sigue siendo
  hijo del **padre original** (`cloneOf.parentId` siempre es el id del
  proyecto del que cuelga la jerarquía visible). El `parent_dirname` del
  segundo nivel es el basename del padre, no el del primer clon.
- **Borrar el sidecar del padre mientras un clon existe**: el clon sigue
  funcionando; el bloque nginx fallback apunta por `parent_dirname`, que
  es estable mientras la carpeta del padre exista en disco.
- **SSL no activo**: `[6/8] SSL desactivado, se omite.` (`clone.rs:161`).
- **DB fallando**: `migrate::import_dump` aplica el watchdog y resetea
  la DB (idempotente). Reintentar `create_clone` desde el mismo snapshot
  reanuda.

## Datos persistidos

- **Snapshot por proyecto** en `~/panel-wp/{slug}/snapshots/{snapshot-id}/`:
  - `code.tar.zst` — tar `--zstd` del `app/public/`, con exclusiones
    fijas (`wp-content/uploads`, `wp-content/cache`, `wp-config.php`,
    `*.log`) + `snapshot_excludes` del `SiteConfig`.
  - `db.sql` — dump completo vía `mysqldump`.
  - `meta.json` — `SnapshotMeta` (id, label, fecha ISO, dbName, dbType,
    codeBytes, dbBytes, excludes).
- **Clone como `SiteConfig`**: vive junto a los demás
  (`config.json` con `clone_of: Some(...)`). `name` = `meta.label`,
  `domain` = `{slug}.test` con desambiguación, `db_name` =
  `{slug}_db`, `migration_pending = false`.
- **`SiteConfig::snapshot_excludes`**: rutas relativas a `app/public/`,
  limpias (`./` y `/` quitados, dedup, sort).

## Containers y Docker

- **Snapshot**: solo enciende el motor DB compartido
  (`docker::ensure_db`) para correr `mysqldump`. **No** arranca el
  container php del proyecto (el tar corre sobre el bind-mount del host).
- **Clone**: enciende el motor DB (para `create_database` + `import_dump`)
  y crea `wp-{cloneId}` (container php independiente). Comparte `panel-net`,
  nginx y DB con el resto. Suma 1 container php y 1 schema al dashboard.
- **Uploads**: el bloque nginx fallback (`nginx.rs:71`) sirve los
  uploads del padre ro. Los nuevos del clone van a su `app/public`
  (rw, propio).
- **No hay teardown de DB compartida tras `delete_site`**: solo `DROP
  DATABASE {db_name}` del esquema del clone, no apaga el container
  compartido (`docker::teardown_unused_shared` solo lo apaga si ningún
  proyecto activo lo usa).

## Fallos y compensaciones

- **`tar` con exit ≥2**: error con stderr; `remove_dir_all` del dir del
  snapshot; no se persiste `meta.json`.
- **`tar` con exit 1**: `⚠` + snapshot válido (los warnings de "file
  changed as we read it" se ignoran).
- **Import del dump cancelado por watchdog (180 s sin avance)**:
  `migrate::import_dump` mata el exec, llama
  `wordpress::reset_database` (DROP + CREATE) y devuelve error con
  «reintenta la migración para importar de nuevo».
- **`fix_site_url` falla en el clon** (p. ej. plugin colgado en el
  dump): se registra `⚠` y se sigue. El clon queda encendido con el
  dominio del snapshot; el usuario puede ajustarlo desde el admin.
- **`create_database` falla**: el clone no se crea; `config.json` queda
  escrito (limpieza manual si se quiere borrar) — el orquestador no hace
  rollback completo de la carpeta en este caso (los pasos 2–8 sí, paso 1
  no porque `SiteConfig` ya está en memoria).
- **`ssl::generate` falla**: el vhost referenciará un cert inexistente y
  nginx `-s reload` fallará. El clon no es accesible por HTTPS hasta
  arreglarlo. `nginx::write_vhost` puede invocarse de nuevo tras
  `regenerate_ssl`.

## Superficies

### UI (SvelteKit, SPA)

- **`/site/[id]`** → tab «Puntos de guardado» en `ProjectDetail.svelte`
  (línea ~1129+): formulario «+ Nuevo punto de guardado» (label +
  «Guardar»), panel «Exclusiones» plegable con detect + manual,
  listado por fecha desc con botón «Clone», «Borrar».
- **No se muestra en clones**: el formulario está oculto si
  `site.config.cloneOf` está poblado
  (`ProjectDetail.svelte:1136`).
- **`OpConsole`** muestra el progreso de `create_snapshot` y `create_clone`
  (escuchan `op-log`).
- **`/` dashboard**: los clones se anidan bajo su padre cuando este
  está parado (badge `C` ámbar en cada clon; indentación `└`).

### IPC (Tauri commands en `lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `create_snapshot` | `id, label` | `snapshot::create_snapshot`; emite `op-log` |
| `list_snapshots` | `id` | `snapshot::list_snapshots` |
| `delete_snapshot` | `id, snapshot_id` | `snapshot::delete_snapshot` |
| `detect_excludable` | `id` | `snapshot::detect_excludable` |
| `set_snapshot_excludes` | `id, excludes` | Persiste `SiteConfig::snapshot_excludes` |
| `create_clone` | `parent_id, snapshot_id` | `clone::create_clone`; emite `op-log` |
| `delete_site` | `id, deleteFolder` | Eliminar un clon (igual que cualquier proyecto) |

`api.ts` (`src/lib/api.ts`) expone los espejos.

### CLI (`scripts/wordpress-panel-cli.sh`)

Autodetecta el proyecto por el CWD y requiere el panel abierto:

- `snapshot list` → `ListSnapshots` (dbus.rs). Imprime tabla con id,
  label, fecha, tamaño.
- `snapshot create "<label>"` → `CreateSnapshot` (dbus.rs).
- `snapshot delete <snapshotId>` → `DeleteSnapshot`.
- `snapshot clone <snapshotId>` → `CreateClone`.

### MCP (`mcp/server.mjs`)

Catálogo:

- `list_snapshots(project)`
- `create_snapshot(project, label)`
- `delete_snapshot(project, snapshotId)`
- `clone_snapshot(project, snapshotId)`

### D-Bus (`src-tauri/src/dbus.rs`)

- `CreateSnapshot(id, label)`, `ListSnapshots(id)`, `DeleteSnapshot(id,
  snapshot_id)`, `CreateClone(parent_id, snapshot_id)` (todos devuelven
  JSON con `{ok, …}` o `bool`).

## Tests

- `clone::tests::find_free_slot_base_libre`,
  `find_free_slot_evita_colision_path`,
  `find_free_slot_evita_colision_dominio`: desambiguación de slug.
- `clone::tests::slugify_etiquetas`: «Antes de actualizar» →
  `antes-de-actualizar`, vacío / solo símbolos → `clone`.
- `clone::tests::db_name_derivacion`: `mysite-clone` → `mysite_clone_db`.
- `snapshot.rs` no tiene tests puros (la lógica es tar + mysqldump + IO).
- `integration_tests.rs` (`#[ignore]`) cubre `create_snapshot` y
  `create_clone` con Docker real.

## Límites conocidos

- **Sin restore in-place**: los snapshots no se restauran sobre el
  proyecto origen. El flujo es «clona el snapshot viejo → migra lo que
  quieras». Si solo quieres volver atrás de cambios en la DB, el
  `app/sql/db-{ts}.sql` más reciente del auto-dump + un `wp db import`
  manual es el camino más directo.
- **No hay diff entre snapshots**: solo se comparan tamaño y bytes
  individuales; no hay UI para ver qué cambió.
- **`code.tar.zst` no preserva atributos raros** (setuid, xattrs): `tar`
  con `-cf` y `--zstd` no tiene flags `-p` ni `--xattrs`. Para WP normal
  (sin setuid) es suficiente.
- **Exclusiones se aplican al `tar` con paths relativos `./ruta`**: si la
  ruta tiene caracteres especiales, puede no funcionar como espera
  (`snapshot.rs:142-147` limpia `./` y `/` finales, pero no escapa
  comodines).
- **Snapshots sin restaurar el estado de plugins/uploads**: el `wp-config`
  puede llevar opciones cacheadas (transients, object cache) que el dump
  sí incluye, pero los archivos subidos después del snapshot no.
- **Clones de clones**: el `parent_dirname` del segundo nivel es el del
  padre original, no el del primer clon (porque `find_free_slot` solo usa
  el nombre del proyecto del que viene, no el id). Si renombras la carpeta
  del padre, los clones siguen apuntando al basename antiguo hasta que se
  regeneren los vhosts.

## Invariantes y recomendación rebuild

- **`snapshots_root` = `~/panel-wp/{slug}/snapshots/`**: la ruta del
  proyecto identifica los snapshots. Borrar la carpeta del proyecto
  borra sus snapshots.
- **`snapshot::list_snapshots` ignora carpetas sin `meta.json`**: si un
  snapshot queda a medias (sin `meta.json`), no aparece en la lista pero
  ocupa disco. Se puede borrar manualmente desde el explorador.
- **`backup::rotate_dumps` NO toca `db.sql` de snapshots**: solo rota
  `db-{ts}.sql` del `app/sql/` (los dumps del auto-dump y del
  export-al-detener). Los `snapshots/*/db.sql` no entran en esa rotación.
- **`code.tar.zst` y `db.sql` se almacenan juntos**: para «regenerar el
  clon» hay que tener ambos. Borrar uno deja el snapshot inútil.
- **Rebuild desde cero**: los snapshots se pueden re-crear vacíos, pero
  los snapshots ya creados son material histórico; perder la carpeta
  `snapshots/` los borra todos. Si la DB se pierde, los `db.sql` son la
  única foto completa (más el último `app/sql/db-{ts}.sql` del auto-dump).
- **Borrar un proyecto no borra sus snapshots si `deleteFolder=false`**:
  la carpeta queda como `disconnected`, los snapshots siguen dentro; al
  re-importar el proyecto (`import_disconnected`) los snapshots vuelven
  a ser visibles.

## Fuentes

- `src-tauri/src/snapshot.rs`
- `src-tauri/src/clone.rs`
- `src-tauri/src/config.rs` (`SiteConfig::snapshot_excludes`,
  `CloneInfo`)
- `src-tauri/src/migrate.rs::import_dump` (reusado por el clone)
- `src-tauri/src/wordpress.rs` (`sync_mu_plugins`, `wp_config_create`,
  `create_database`)
- `src-tauri/src/ssl.rs`
- `src-tauri/src/backup.rs::export_db_to` (reusado por el snapshot)
- `src-tauri/src/nginx.rs::render_vhost` (bloque uploads para clones)
- `src-tauri/src/lib.rs` (comandos `create_snapshot`, `list_snapshots`,
  `delete_snapshot`, `detect_excludable`, `set_snapshot_excludes`,
  `create_clone`)
- `src-tauri/src/dumplog.rs` (`DumpLogEntry::source = "auto"|"stop"|"manual"`;
  los snapshots no generan entrada aquí — solo los dumps `db-{ts}.sql`
  del proyecto)
- `src/lib/components/ProjectDetail.svelte` (tab Snapshots)
- `src/routes/+page.svelte` (anidación de clones bajo el padre)
- `src/lib/api.ts`
- `mcp/server.mjs`, `scripts/wordpress-panel-cli.sh`,
  `src-tauri/src/dbus.rs`
- `docs/ARCHITECTURE.md` (secciones snapshot y clone)
