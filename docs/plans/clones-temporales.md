# Clones temporales + puntos de guardado

## Context

El usuario necesita experimentar sobre sus proyectos WordPress locales sin tocar el
original: probar updates de theme/plugin, mover archivos y medir el alcance del daño,
validar despliegues. Hoy no hay forma de hacerlo aislado y barato.

Dos necesidades distintas surgieron de la conversación:

1. **Punto de guardado** — un checkpoint "tipo git" del estado *limpio* del proyecto
   (código + DB, **sin** uploads/cache). El dump automático que se genera al detener
   un proyecto NO sirve como baseline: refleja el último estado cualquiera (posiblemente
   sucio) y el usuario puede apagar/encender entre medias. El usuario quiere marcar
   explícitamente un estado conocido-limpio desde el cual experimentar.

2. **Clone temporal** — un proyecto efímero que nace *desde un punto de guardado*,
   corre en paralelo al original, y se destruye al terminar. Sobre él se hacen las
   pruebas destructivas.

Restricción rectora del proyecto (CLAUDE.md): **minimizar recursos**. El diseño se
subordina a: nada corre si no hace falta, compartir antes que duplicar, imágenes mínimas.

### Decisiones tomadas (con el usuario)

- **Punto de guardado = snapshot simple**: `tar` (zstd) del código excluyendo uploads/cache
  + dump SQL atado al checkpoint. Sin git.
- **Uploads del clone vía nginx `try_files`**: los uploads nuevos se guardan en el clone
  (rw); los viejos se sirven solo-lectura desde el principal vía fallback de nginx. Sin
  privilegios, sin copiar media. El principal nunca se contamina.
- **Vida del clone = manual** (sin timeout). Pruebas largas. Se destruye a mano.
- **DB del clone = nueva DB en el engine compartido** (`panel-mysql-{ver}`), no container nuevo.
- **Código del clone**: se extrae del snapshot. Uploads NO se copian (se apuntan al principal).

### Costo de recursos de un clone activo

- 1 container php `wp-{clone-id}` (inevitable para un sitio activo).
- 1 schema DB dentro del engine compartido (0 containers extra).
- nginx/mailpit compartidos (ya corren si hay otro activo).
- Disco extra ≈ tamaño del código del snapshot (uploads apuntados, no copiados).

---

## Modelo de datos

### `src-tauri/src/config.rs` — extender `SiteConfig`

Añadir bloque opcional de clone (camelCase en serde, espejo en TS):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub clone_of: Option<CloneInfo>,
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneInfo {
    pub parent_id: String,
    pub parent_dirname: String,   // slug del principal, para la ruta nginx de uploads-base
    pub snapshot_id: String,
    pub created_at: String,
}
```

- `clone_of.is_some()` ⇒ el sitio es un clone temporal (badge en UI, lógica de teardown).
- `parent_dirname` permite a `nginx.rs` construir la ruta de uploads del principal sin
  cargar el config del padre.

### `src/lib/types.ts` — espejo

Añadir `cloneOf?: CloneInfo | null` a `SiteConfig` y la interfaz `CloneInfo`.

---

## Backend

### 1. Nuevo módulo `src-tauri/src/snapshot.rs`

Almacén por proyecto: `~/panel-wp/{slug}/snapshots/{snapshot-id}/`
- `code.tar.zst` — tar del código, excluye `wp-content/uploads`, `wp-content/cache`,
  `wp-config.php` (se regenera), `*.log`.
- `db.sql` — dump completo.
- `meta.json` — `{ id, label, createdAt, dbName, dbType }`.

Funciones:
- `create_snapshot(docker, site, label) -> Result<SnapshotMeta>`
  - DB: reusar la lógica de `backup::export_db` (refactor: extraer
    `export_db_to(docker, site, dest_path)` y que `export_db` lo llame con el destino
    rotativo actual). Escribir a `snapshots/{id}/db.sql`.
  - Código: `tar` host-side sobre `site.public_dir()` con `--exclude` (uploads/cache/
    wp-config). Comprimir zstd (`tar --zstd` o crate `zstd`).
  - El sitio NO necesita estar encendido para el tar; para el dump sí (engine DB up).
- `list_snapshots(site) -> Vec<SnapshotMeta>` — escanea el dir (patrón idéntico a
  `load_all_sites` en `config.rs:303`).
- `delete_snapshot(site, id)` — `remove_dir_all`.

### 2. Crear clone — `src-tauri/src/clone.rs` (o extender `wordpress.rs`)

`create_clone(docker, parent_id, snapshot_id, opts) -> Result<SiteConfig>`. Reutiliza
intensivamente el flujo de `wordpress::create_site` (`wordpress.rs:106`):

1. Cargar padre (`config::find_site`) y `SnapshotMeta`.
2. Nuevo `SiteConfig`: `id = uuid`, `name = "{padre} (clone)"`,
   `domain = "{parent-slug}-clone.test"` (sufijo corto si colisiona),
   `services` copiados del padre, `db_name = "{slug}_clone_db"`,
   `clone_of = Some(CloneInfo{ parent_id, parent_dirname, snapshot_id, created_at })`.
3. `wordpress::create_dirs(&site)` + `write_php_ini` + `config::write_site_config`.
4. Extraer `code.tar.zst` del snapshot dentro de `site.public_dir()`.
5. **NO** crear `wp-content/uploads` con contenido — queda vacío (rw para nuevos).
6. `docker.ensure_db` → `wordpress::create_database` (nueva DB en engine compartido).
7. `migrate::import_dump` con `snapshots/{id}/db.sql` (reusa el path `docker exec -i`
   con watchdog de `migrate.rs:243`).
8. `wordpress::sync_mu_plugins` (mailpit/autologin).
9. `docker.start_site(&site)` (`docker.rs:565`) — crea container, vhost, nginx.
10. `wordpress::wp_config_create` apuntando a la nueva DB.
11. Fix de URL: reusar el paso WP-CLI search-replace/option de `migrate::migrate_site`
    para reescribir home/siteurl al dominio del clone.

### 3. nginx `try_files` para uploads — `src-tauri/src/nginx.rs`

En `render_vhost` (`nginx.rs:31`), si `site.clone_of.is_some()`, emitir bloque que sirve
uploads del clone con fallback al principal (nginx ya monta todos los proyectos en
`/srv/projects` ro, así que la ruta del padre es accesible sin binds extra):

```nginx
location ^~ /wp-content/uploads/ {
    root /srv/projects/{clone-dirname}/app/public;
    try_files $uri @uploads_base;
}
location @uploads_base {
    root /srv/projects/{parent-dirname}/app/public;
    try_files $uri =404;
}
```

`{parent-dirname}` viene de `site.clone_of.parent_dirname`. Lectura web de media vieja
cubierta; escritura de media nueva cae en la carpeta del clone (rw). No requiere binds
nuevos en el container php.

### 4. Teardown del clone

- **Detener** (pausa, conserva datos para reanudar): `docker.stop_site` tal cual
  (`docker.rs:656`) + `teardown_unused_shared`.
- **Destruir** (botón borrar): drop DB (`wordpress::drop_database`, `wordpress.rs:303`)
  + `nginx::remove_vhost` + `remove_dir_all` de la carpeta del clone (incluye los uploads
  nuevos). Reusar el flujo de `delete_site` existente; la rama de clone solo añade el
  drop de su DB. **Importante**: como las DB son por-versión compartidas, NO tocar el
  engine; solo `DROP DATABASE {slug}_clone_db`.

### 5. Comandos IPC — `src-tauri/src/lib.rs`

Nuevos `#[tauri::command]` (devuelven `Result<T, String>` con helper `e()`), registrar en
`invoke_handler!` y exponer en `src/lib/api.ts`:
- `create_snapshot(siteId, label)`
- `list_snapshots(siteId)`
- `delete_snapshot(siteId, snapshotId)`
- `create_clone(parentId, snapshotId)`

Start/stop/delete del clone reusan los comandos existentes (`startSite`/`stopSite`/
`deleteSite`) — el clone es un `SiteConfig` normal con `cloneOf` poblado.

---

## Frontend

### `src/lib/api.ts`
Añadir `createSnapshot`, `listSnapshots`, `deleteSnapshot`, `createClone` (espejo de los
comandos). Lifecycle reusa `startSite/stopSite/deleteSite/getSites`.

### `src/routes/site/[id]/+page.svelte`
- Botón **"Punto de guardado"** en la barra de acciones (`+page.svelte:315`) → modal con
  campo label → `createSnapshot`.
- Nueva pestaña **"Puntos de guardado"**: lista `listSnapshots`, cada uno con botón
  **"Clonar desde aquí"** (`createClone`) y borrar.
- Si `cloneOf` está poblado: badge "Clone temporal" + nota del padre/snapshot origen.

### `src/routes/+page.svelte` (dashboard)
- Badge ámbar "Clone" tras el dominio cuando `cloneOf` (`+page.svelte:184`).
- Reusar botones start/stop/delete existentes.

### Modal de creación de clone
Reusar el patrón de `DeleteProjectModal.svelte` (confirmación + OpConsole vía evento
`op-log`) para mostrar progreso de extracción tar + import de DB.

---

## Limitaciones conocidas (documentar en `docs/KNOWN_ISSUES.md`)

- **try_files cubre lectura web de media vieja, no lectura por filesystem desde PHP**
  (ej. regenerar thumbnails escanea el dir de uploads del clone, que solo tiene los
  nuevos). Aceptable para los casos de uso (validar update de theme, medir daño de mover
  archivos). Si en el futuro hace falta merge real a nivel filesystem → evaluar overlayfs
  (requiere pkexec).
- Uploads nuevos del clone se borran al **destruir** el clone, no al pausar (stop conserva
  para reanudar pruebas largas).

---

## Verificación

1. `cd src-tauri && cargo test` — lógica pura (render de vhost con bloque clone, exclusiones
   del tar, derivación de dominio/db-name). Añadir tests unitarios para `nginx::render_vhost`
   en modo clone y para el armado de `CloneInfo`.
2. `cargo test -- --ignored --test-threads=1` — integración Docker: crear snapshot, crear
   clone, verificar DB independiente y container `wp-{clone-id}` arriba.
3. Manual con `pnpm tauri dev`:
   - Sobre un proyecto con media: crear punto de guardado.
   - Subir/cambiar algo en el principal (ensuciar DB).
   - Clonar desde el punto de guardado → verificar que el clone tiene la DB *limpia*
     (no los cambios sucios) y que la media vieja se ve en el sitio del clone.
   - Subir un archivo nuevo en el clone → confirmar que aparece SOLO en el clone
     (`~/panel-wp/{clone-slug}/.../uploads`) y NO en el principal.
   - Mover/borrar archivos en el clone, medir efecto, sin afectar al principal.
   - Destruir el clone → confirmar `DROP DATABASE`, carpeta borrada, principal intacto,
     y que `teardown_unused_shared` apaga compartidos si no quedan activos.
4. `pnpm dev:mock` para revisar UI (badges, pestaña de puntos de guardado) sin backend.
