# Plan: Re-importar proyectos desconectados (carpeta conservada)

## Contexto

El panel ya permite **borrar un proyecto conservando la carpeta** (`delete_site`
con `deleteFolder=false`): apaga, borra la DB del servidor compartido y, en la
rama "desconexión", **elimina `config.json`** para que `load_all_sites()` lo
olvide. La carpeta queda en `~/panel-wp/{slug}/` con todo lo importante: WP files
(`app/public/`), dumps SQL (`app/sql/db-*.sql`, escritos por el export-al-detener
ANTES de hacer DROP), `conf/php/php.ini`, ssl, logs.

El usuario quiere **re-conectar** esas carpetas (tras un borrado, un formateo, u
otra PC) desde una opción "Importar proyecto" en la lista de proyectos, que liste
las carpetas presentes en `~/panel-wp/` que ya no están en el panel.

**Problema raíz:** hoy la desconexión BORRA `config.json`, así que se pierde la
metadata (id, nombre, dominio, versiones PHP/DB, `dbName`). Para re-importar sin
pérdida hay que **conservar** esa config.

**Decisiones tomadas (usuario):**
1. **Conservar config**: la desconexión renombra `config.json` →
   `config.disconnected.json` (no lo borra). Re-importar = restaurarlo, sin
   pérdida. Carpetas viejas sin ninguna config → reconstrucción best-effort.
2. **Post-import**: el proyecto queda **`migrationPending`**; el usuario pulsa
   "Migrar y encender" (flujo `migrate_site` existente) que recrea la DB, importa
   el último dump de `app/sql/` y enciende. Respeta la regla "nada corre si no
   hace falta".

**Resultado esperado:** botón "Importar proyecto" en el dashboard → modal lista
carpetas desconectadas → "Importar" restaura el proyecto como pendiente de
migración, con consola de progreso.

## Rama

`feat/import-disconnected-projects` (desde `main`).

---

## Backend (Rust)

### 1. `src-tauri/src/config.rs` — conservar y descubrir

- Constante `pub(crate) const DISCONNECTED_CONFIG: &str = "config.disconnected.json";`
  y helper `disconnected_config_path(path: &str) -> PathBuf`.
- Nueva struct serializable (camelCase, espejo en `types.ts`):
  ```rust
  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct DisconnectedSite {
      pub folder_name: String,
      pub path: String,
      pub name: String,
      pub domain: String,
      pub php_version: String,
      pub db_version: String,
      pub db_type: String,        // "mysql" | "mariadb" | "postgres"
      pub has_dump: bool,         // hay algún app/sql/*.sql
      pub kind: String,           // "preserved" | "reconstructed"
  }
  ```
- `pub fn list_disconnected_sites() -> Result<Vec<DisconnectedSite>>`: escanea
  `projects_root()`. Para cada dir **sin** `config.json`:
  - tiene `config.disconnected.json` → `read_site_config` sobre ese archivo →
    `kind="preserved"`, rellena campos desde la config.
  - si no, pero existe `app/public/wp-config.php` → `kind="reconstructed"`:
    nombre = nombre de carpeta, dominio = `{slug}.test`, `dbName` parseado de
    `wp-config.php` (`define('DB_NAME', '…')`) o slug, versiones por defecto
    (últimas soportadas, reusar `localwp::pick_supported_*` / constantes).
  - si no, omitir (no es un proyecto del panel).
  - `has_dump` = hay algún `*.sql` en `app/sql/` (reusar criterio de
    `migrate::latest_dump`).
- Reusar `read_site_config` (acepta cualquier ruta de archivo) — ya existe.

### 2. `src-tauri/src/lib.rs` — comandos IPC

- **Cambiar la rama de desconexión de `delete_site`** (única modificación a la
  feature de borrado): en vez de `fs::remove_file(config.json)`, **renombrar**
  `config.json` → `config.disconnected.json` (sobrescribe si ya existe). Ajustar
  el texto del `op-log` ("Desconectando… (se conserva la carpeta y su
  configuración para reimportar)").
- `#[tauri::command] async fn list_disconnected_sites() -> CmdResult<Vec<DisconnectedSite>>`
  → `config::list_disconnected_sites().map_err(e)`. Guardar contra solapamiento
  con `load_all_sites()` (no debería haber, pero filtrar por path por si acaso).
- `#[tauri::command] async fn import_disconnected_site(app, folder_name: String) -> CmdResult<ImportResult>`:
  - resolver ruta bajo `projects_root()`; validar que existe y tiene `app/public`.
  - `log(app, "▶ Re-importando «…»")`, pasos con `progress::log`.
  - construir `SiteConfig`:
    - preserved: `read_site_config(config.disconnected.json)`.
    - reconstructed: config por defecto (nuevo `id` uuid, slug, services default,
      dbName detectado).
  - `migration_pending = true`. Si el `id` colisiona con un proyecto vivo,
    regenerar uuid. (Dominio duplicado: dejar pasar; el usuario lo resolverá.)
  - `write_site_config(&site)` → crea `config.json`; luego borrar
    `config.disconnected.json` si existía.
  - `log(app, "✓ … re-importado. Usa «Migrar y encender» en Proyectos.")`.
  - devolver `ImportResult { site, note }`. **Reusar el tipo `ImportResult` de
    `localwp.rs`** (mover a un módulo común si hace falta, o re-exportar).
- Registrar ambos comandos en `tauri::generate_handler![…]`.

### 3. Tests Rust (hermético, sin Docker) en `integration_tests.rs`

- `list_e_import_disconnected_hermetico` (`#[ignore]`, redirige `HOME`):
  monta `panel-wp/` con (a) un proyecto vivo (`config.json`) → excluido; (b) uno
  desconectado (`config.disconnected.json` + `app/sql/db-x.sql`) → listado como
  `preserved`/`hasDump=true`; (c) una carpeta con `app/public/wp-config.php` sin
  config → `reconstructed`. Luego `import_disconnected_site` sobre (b): comprueba
  que aparece `config.json` con `migrationPending=true` y que desaparece el
  sidecar.

---

## Frontend (SvelteKit)

### 4. `src/lib/types.ts`

Añadir `DisconnectedSite` (espejo exacto camelCase del struct). `ImportResult`
(site + note) ya existe — reusar como retorno del import.

### 5. `src/lib/api.ts`

```ts
listDisconnectedSites: () => invoke<DisconnectedSite[]>('list_disconnected_sites'),
importDisconnectedSite: (folderName: string) =>
  invoke<ImportResult>('import_disconnected_site', { folderName }),
```

### 6. `src/routes/+page.svelte` (dashboard)

- Botón **"Importar proyecto"** en el header, junto a "Nuevo proyecto"
  (`<div class="flex gap-2">` lin. ~124-144). `onclick` abre el modal.
- `let importOpen = $state(false);` + `<ImportProjectModal bind:open={importOpen}
  onClose={(imported) => imported && load()} />` (recarga la lista para que el
  proyecto re-importado aparezca como pendiente de migración).

### 7. `src/lib/components/ImportProjectModal.svelte` (NUEVO)

Espeja el patrón de la sección "Importar desde LocalWP"
(`src/routes/settings/+page.svelte` lin. 205-239) + uso de `OpConsole` y el estilo
de modal de `DeleteProjectModal.svelte`.

- Props: `open = $bindable<boolean>()`, `onClose?: (imported: boolean) => void`.
- `$effect`: al pasar a `open`, llamar `api.listDisconnectedSites()` → `list`.
- Render: panel modal (`role="dialog"`) con lista; cada item muestra
  nombre → dominio, badge (`config conservada` / `reconstruido`), `PHP x · DB y`,
  y "con dump"/"sin dump"; botón "Importar" por fila (deshabilitado mientras
  `importing[folderName]`).
- Estado vacío: "No hay carpetas de proyectos desconectadas en `~/panel-wp/`."
- Al importar: `consoleOpen=true; consoleRunning=true`; `api.importDisconnectedSite(folderName)`;
  al terminar `consoleRunning=false`, refrescar `list`, marcar `imported=true`.
- `<OpConsole open={consoleOpen} running={consoleRunning} title="Importar proyecto"
  onClose={() => { consoleOpen=false; }} />` — escucha `op-log` igual que migración.
- Al cerrar el modal: `onClose?.(imported)`.

### 8. Mock (`src/lib/dev/`)

- `fixtures.ts`: `initialDisconnectedSites(): DisconnectedSite[]` con 2 ejemplos
  (uno `preserved` con dump, uno `reconstructed` sin dump).
- `mock-ipc.ts`: casos `list_disconnected_sites` (devuelve el array) y
  `import_disconnected_site` (emite líneas `op-log` con `progress([...])`, añade el
  sitio a `sites` como `migrationPending`, lo quita de `disconnectedSites`,
  devuelve `{ site, note: null }`).

---

## Pruebas e2e

### 9. `e2e/import-project.spec.ts` (NUEVO)

Patrón de `delete-site.spec.ts` (scopear con `getByRole('dialog')`):
- abrir `/` → click "Importar proyecto" → el modal lista las carpetas
  desconectadas (assert nombre + badge).
- click "Importar" en una → `OpConsole` muestra progreso + mensaje ✓; "Cerrar"
  se habilita.
- cerrar → el proyecto aparece en el dashboard como **pendiente de migración**
  (punto ámbar / botón "Migrar y encender").

Actualizar `docs/TESTING.md`: añadir `import-project` a la lista de specs (§B.2) y
un escenario §C ("Re-importar proyecto desconectado").

---

## Documentación

- `docs/ARCHITECTURE.md`: catálogo IPC → `list_disconnected_sites`,
  `import_disconnected_site`; nota de que la desconexión renombra
  `config.json`→`config.disconnected.json`; componente `ImportProjectModal.svelte`
  en la sección Frontend.
- `docs/CHANGELOG.md`: entrada "Re-importar proyectos desconectados".
- `docs/KNOWN_ISSUES.md`: la rama `reconstructed` (carpetas sin config) es
  best-effort — versiones PHP/DB por defecto, requiere revisar dominio/dbName.

---

## Verificación end-to-end

1. **Rust rápido**: `cd src-tauri && cargo build` y `cargo test` (incluye el
   hermético nuevo: `cargo test --lib integration_tests::list_e_import_disconnected_hermetico -- --ignored --exact`).
2. **Typecheck/lint frontend**: `pnpm check` (0/0).
3. **e2e mock**: `pnpm test:e2e` (debe pasar el spec `import-project`).
4. **Manual real** (`pnpm tauri dev`):
   - crear un proyecto, encenderlo, borrarlo con "Eliminar" **sin** marcar borrar
     carpeta → confirmar que en disco queda `config.disconnected.json` (no
     `config.json`) y desaparece del panel.
   - "Importar proyecto" → debe aparecer esa carpeta como `preserved` con dump →
     Importar → queda pendiente de migración → "Migrar y encender" levanta el
     sitio con sus datos (DB restaurada del dump).

## Archivos clave

- Backend: `src-tauri/src/config.rs` (scan + struct), `src-tauri/src/lib.rs`
  (comandos + cambio en `delete_site`), `src-tauri/src/integration_tests.rs`.
  Reusar: `migrate::migrate_site`/`latest_dump`, `localwp::ImportResult` +
  `pick_supported_*`, `backup`/export-al-detener (ya escribe los dumps).
- Frontend: `src/lib/types.ts`, `src/lib/api.ts`, `src/routes/+page.svelte`,
  `src/lib/components/ImportProjectModal.svelte` (nuevo), `src/lib/dev/fixtures.ts`,
  `src/lib/dev/mock-ipc.ts`.
- Tests/docs: `e2e/import-project.spec.ts`, `docs/{ARCHITECTURE,CHANGELOG,TESTING,KNOWN_ISSUES}.md`.
