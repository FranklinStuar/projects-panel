# 03 · Contratos, estado y persistencia

> Documento 3 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este capítulo ata los **contratos** que cruzan la frontera
> UI↔backend (Tauri IPC), los **schemas versionados** para los archivos
> en disco, los **locks** y **escrituras atómicas**, las **migraciones de
> disco** y la **trazabilidad** de cada cambio de estado.

---

## 1. Modelo de error: `AppError`

### 1.1 Por qué sustituir `Result<T, String>`

Hoy todos los comandos devuelven `Result<T, String>` y el helper `e()`
empaqueta `anyhow::Error` en un `String`. Esto implica:

- El frontend no puede tipar el error.
- No hay `code` discriminable para la UI.
- No hay `hint` accionable.
- Pérdida de la cadena de causa.

El rebuild introduce `domain::error::AppError` como **único** error de
la aplicación, con `Serialize` en un shape estable.

### 1.2 Definición

```rust
// domain/error.rs
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {what} {id}")]
    NotFound { what: &'static str, id: String },

    #[error("validation: {field}: {message}")]
    Validation { field: String, message: String },

    #[error("conflict: {resource}: {reason}")]
    Conflict { resource: String, reason: String },

    #[error("busy: {resource} (locked by another operation)")]
    Busy { resource: String },

    #[error("permission: {action} requires {what}")]
    Permission { action: String, what: String },

    #[error("io: {path}: {source}")]
    Io { path: String, source: std::io::Error },

    #[error("docker: {0}")]
    Docker(bollard::errors::Error),

    #[error("database: {0}")]
    Database(String),

    #[error("network: {0}")]
    Network(String),

    #[error("parse: {what}: {message}")]
    Parse { what: String, message: String },

    #[error("schema: {what}: {message}")]
    Schema { what: String, message: String },

    #[error("operation failed: {kind}: {message}")]
    Operation { kind: String, message: String },

    #[error("cancelled: {operation}")]
    Cancelled { operation: String },

    #[error("unsupported: {what}")]
    Unsupported { what: String },

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}
```

### 1.3 Serialización a IPC

```rust
// adapters/tauri/error.rs
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppErrorDto<'a> {
    code: &'a str,
    message: String,
    hint: Option<String>,
    cause: Option<Box<AppErrorDto<'a>>>,
    retryable: bool,
}

impl<'a> From<&'a AppError> for AppErrorDto<'a> {
    fn from(err: &'a AppError) -> Self {
        let (code, hint, retryable) = classify(err);
        AppErrorDto {
            code,
            message: err.to_string(),
            hint,
            cause: err.source().map(|c| Box::new(AppErrorDto::from(c))),
            retryable,
        }
    }
}
```

`classify` mapea variantes a códigos:

| Variante | code | hint | retryable |
|---|---|---|---|
| `NotFound` | `not_found` | «Verifica el id» | false |
| `Validation` | `validation` | mensaje del campo | false |
| `Conflict` | `conflict` | mensaje | false |
| `Busy` | `busy` | «Reintenta en unos segundos» | true |
| `Permission` | `permission` | «Verifica que pkexec esté disponible» | false |
| `Io` | `io` | «Comprueba el espacio en disco y los permisos» | true |
| `Docker` | `docker` | mensaje | false |
| `Database` | `database` | mensaje | false |
| `Network` | `network` | «Comprueba tu conexión» | true |
| `Parse` | `parse` | mensaje | false |
| `Schema` | `schema` | «Actualiza el panel o restaura la versión anterior» | false |
| `Operation` | `operation` | mensaje | false |
| `Cancelled` | `cancelled` | null | false |
| `Unsupported` | `unsupported` | «Esta plataforma no está soportada» | false |
| `Internal` | `internal` | null | false |

### 1.4 En el frontend

```ts
// lib/api.ts
export type AppErrorDto = {
  code: string;
  message: string;
  hint?: string;
  cause?: AppErrorDto;
  retryable: boolean;
};

export class PanelError extends Error {
  constructor(public dto: AppErrorDto) {
    super(dto.message);
  }
  get code() { return this.dto.code; }
  get hint() { return this.dto.hint; }
  get retryable() { return this.dto.retryable; }
  static wrap(err: unknown): PanelError {
    if (err instanceof PanelError) return err;
    if (typeof err === 'string') return new PanelError({ code: 'unknown', message: err, retryable: false });
    return new PanelError({ code: 'unknown', message: String(err), retryable: false });
  }
}
```

Cada `invoke` se envuelve con `PanelError.wrap(err)` para que la UI
siempre obtenga un `PanelError` tipado.

### 1.5 Mapping a `zod` (runtime)

```ts
// lib/contracts/error.ts
import { z } from 'zod';

export const AppErrorDtoSchema = z.object({
  code: z.string(),
  message: z.string(),
  hint: z.string().optional(),
  cause: z.lazy(() => AppErrorDtoSchema.optional()),
  retryable: z.boolean(),
});
```

---

## 2. Contrato IPC tipado

### 2.1 Single source of truth

El contrato se define **una sola vez** en `src-tauri/src/contracts/`
como JSON Schema derivado de los modelos serde. Una build step
(`xtask gen-contracts`) genera `src/lib/contracts/*.ts` con:

- Tipos TS para cada comando.
- `zod` schemas para validación en runtime.
- Constantes de nombres de eventos.

### 2.2 Ejemplo: `start_site`

```json
// src-tauri/src/contracts/commands.json
{
  "name": "start_site",
  "args": { "id": "string" },
  "result": { "$ref": "Operation" },
  "errors": ["not_found", "operation", "docker", "busy"]
}
```

Genera:

```ts
// src/lib/contracts/commands.ts
import { z } from 'zod';

export const StartSiteArgs = z.object({ id: z.string().uuid() });
export type StartSiteArgs = z.infer<typeof StartSiteArgs>;

export const OperationSchema = z.object({
  id: z.string().uuid(),
  kind: z.string(),
  siteId: z.string().uuid(),
  status: z.enum(['pending', 'running', 'succeeded', 'failed', 'cancelled']),
  startedAt: z.string().datetime(),
  finishedAt: z.string().datetime().optional(),
  error: AppErrorDtoSchema.optional(),
});
export type Operation = z.infer<typeof OperationSchema>;

export const startSite = (args: StartSiteArgs) =>
  invoke<Operation>('start_site', args).catch(PanelError.wrap);
```

### 2.3 Validación en frontera

- **Backend**: `AppError::Validation` se devuelve si los args no son
  parseables. Esta es la **validación de esquema** (no la de negocio).
- **Backend**: el use case valida la **semántica** (slug libre, dominio
  no usado, motor DB soportado).
- **Frontend**: `zod` valida en dev/test; en prod, es una red de
  seguridad, no la fuente de verdad.

### 2.4 Catálogo de eventos

```ts
// src/lib/contracts/events.ts
export const Events = {
  OpLog: 'op-log',                  // OpEvent tipado (no string)
  LogStream: (id: string) => `log:${id}`,
  SitesChanged: 'sites-changed',
  DriftDetected: 'drift-detected',
  SocketConnected: 'socket-connected',
} as const;
```

El backend emite `op-log` con un envelope:

```json
{
  "opId": "uuid",
  "ts": "2026-07-23T12:34:56Z",
  "evt": {
    "type": "step",
    "idx": 3,
    "total": 7,
    "label": "Importando dump…"
  }
}
```

El `OpConsole.svelte` actual solo pinta strings; el rebuild lo cambia
a un árbol tipado que permite:

- Mostrar el plan completo antes de ejecutar.
- Marcar steps como hechos/fallidos con iconos.
- Reanudar manualmente un step que quedó pendiente.

---

## 3. Schema versionado: `config.json`

### 3.1 Header obligatorio

```json
{
  "schemaVersion": 2,
  "id": "uuid",
  "name": "Mi sitio",
  "slug": "mi-sitio",
  "path": "/home/user/panel-wp/mi-sitio",
  "domain": "mi-sitio.test",
  "group": "Clientes",
  "createdAt": "2026-07-23T12:34:56Z",
  "lastMigratedAt": "2026-07-23T12:34:56Z",
  "services": {
    "php": { "version": "8.3" },
    "db":  { "type": "mysql", "version": "8.0", "dbName": "mi_sitio_db" },
    "nginx": { "ssl": true }
  },
  "shared": { "minio": false, "mailpit": true },
  "flags": {
    "oneClickAdmin": true,
    "xdebugEnabled": false,
    "headless": false,
    "frontendFramework": null,
    "migrationPending": false
  },
  "derived": {
    "cloneOf": null,
    "worktreeOf": null,
    "snapshotExcludes": []
  },
  "github": {
    "repos": [
      {
        "repo": "owner/theme",
        "branch": "main",
        "path": "wp-content/themes/mi-theme",
        "buildCmd": "npm ci && npm run build",
        "buildDirs": ["src"]
      }
    ]
  }
}
```

### 3.2 Reglas de versión

- `schemaVersion` es **obligatorio** en configs v2+.
- Configs v1 (sin `schemaVersion`) se aceptan y se migran silenciosamente
  al primer `write_site`.
- Configs con `schemaVersion > CURRENT_SCHEMA` se rechazan con
  `AppError::Schema`. El panel no puede leerlas.
- Cada cambio de versión lleva una migración:
  - `migrate_v1_v2.rs`: añade `schemaVersion: 2`, restructura `services`
    en sub-objeto, mueve `oneClickAdmin`/`xdebugEnabled`/`headless`/`frontendFramework`
    a `flags`.
  - `migrate_v2_v3.rs` (cuando exista): añade nuevo campo, opcional.

### 3.3 Idempotencia de migración

```rust
// config/schema.rs
pub fn migrate_to_current(raw: Value) -> Result<SiteConfig, AppError> {
    let v = detect_schema_version(&raw)?;
    if v == CURRENT_SCHEMA {
        return serde_json::from_value(raw).map_err(AppError::from);
    }
    if v > CURRENT_SCHEMA {
        return Err(AppError::Schema {
            what: "config.json".into(),
            message: format!("schemaVersion={v} > panel={CURRENT_SCHEMA}, actualiza el panel"),
        });
    }
    let mut value = raw;
    for step in v..CURRENT_SCHEMA {
        value = migrate_step(step, value)?;
    }
    serde_json::from_value(value).map_err(AppError::from)
}
```

`detect_schema_version` reconoce los legacy:

```rust
fn detect_schema_version(raw: &Value) -> Result<u32, AppError> {
    if let Some(v) = raw.get("schemaVersion").and_then(|v| v.as_u64()) {
        return Ok(v as u32);
    }
    // Heurística: una v1 tiene "services" como un objeto con "php" y "db"
    // separados, no como {php: {version: ...}}.
    let legacy = raw.get("services")
        .and_then(|s| s.get("php"))
        .and_then(|p| p.as_str())
        .is_some();
    if legacy {
        return Ok(1);
    }
    Err(AppError::Schema {
        what: "config.json".into(),
        message: "no se pudo detectar schemaVersion".into(),
    })
}
```

### 3.4 Backups de config

Antes de una migración destructiva, el `Config::write_site` hace:

```rust
pub async fn write_site(&self, site: &Site) -> Result<(), AppError> {
    let cfg_path = site.path.join("config.json");
    let backup = site.path.join(format!("config.bak.{}.json", timestamp()));
    if cfg_path.exists() {
        self.fs.copy(&cfg_path, &backup).await?;
    }
    let bytes = serde_json::to_vec_pretty(site)?;
    self.fs.atomic_write(&cfg_path, &bytes).await?;
    Ok(())
}
```

Los `.bak.{timestamp}` se rotan (mantener últimos 5).

---

## 4. Otros archivos persistidos

### 4.1 `panel.json`

```json
{
  "schemaVersion": 1,
  "endpoint": {
    "loopbackIp": "127.0.0.1",
    "httpPort": 80,
    "httpsPort": 443
  },
  "ignoredDrifts": ["dns-not-resolving:foo.bar"]
}
```

- `CURRENT_SCHEMA = 1`. Sin migraciones aún.
- `ignoredDrifts` lo escribe el reconciliador (acción `Ignore`).

### 4.2 `groups.json`

```json
{
  "schemaVersion": 1,
  "order": ["Clientes", "Personales", "Worktrees"]
}
```

Migración: añadir `schemaVersion: 1` al primer write (legacy es el
`{order: [...]}` directo).

### 4.3 `dump-log.jsonl`

Cada línea es un `DumpLogEntry`:

```json
{"schemaVersion":1,"timestamp":"2026-07-23T12:34:56Z","siteId":"uuid","siteName":"Mi sitio","dbName":"mi_sitio_db","file":"/home/user/panel-wp/mi-sitio/app/sql/db-20260723-123456.sql","bytes":1234567,"source":"auto"}
```

- El campo `schemaVersion` es **opcional** en la primera línea (legacy).
- Se loguea con `append_only` (lock + write + fsync).
- `clean_dump_log` borra entradas (no los `.sql`).
- `compact_dump_log` (periódico) reescribe el archivo con las entradas
  vivas (lock). Reduce drift.

### 4.4 `meta.json` (snapshots)

```json
{
  "schemaVersion": 1,
  "id": "uuid",
  "label": "pre-deploy",
  "createdAt": "2026-07-23T12:34:56Z",
  "dbName": "mi_sitio_db",
  "dbType": "mysql",
  "codeBytes": 1234567,
  "dbBytes": 9876543,
  "excludes": ["wp-content/updraft"]
}
```

Migración: añadir `schemaVersion: 1` al primer write.

### 4.5 `operations/{id}.jsonl` (journal)

Cada línea es un `JournalEntry`:

```json
{"ts":"2026-07-23T12:34:56Z","opId":"uuid","kind":"create_site","siteId":"uuid","event":{"type":"step_started","step":3,"label":"Importando dump"}}
{"ts":"2026-07-23T12:34:57Z","opId":"uuid","event":{"type":"progress","step":3,"ratio":0.42,"units":"MiB"}}
{"ts":"2026-07-23T12:35:00Z","opId":"uuid","event":{"type":"step_finished","step":3,"ok":true}}
{"ts":"2026-07-23T12:35:01Z","opId":"uuid","event":{"type":"done","status":"succeeded"}}
```

- Append-only.
- Tamaño objetivo: < 1 MB por operación. Rotado cuando supera 5 MB.
- Permite `inspect(op_id)` para depurar.

### 4.6 `nginx/conf.d/{site-id}.conf`

Texto generado por `nginx::render_vhost`. No versionado: se regenera
cada `start_site`. Si el archivo existe con contenido distinto, se
sobrescribe (idempotente).

### 4.7 `tls/` (cert-bot para SSL público, futuro)

El rebuild mantiene `ssl/{cert.pem,key.pem}` por proyecto (como hoy).
Un futuro adaptador a `certbot` o `acme-client` añade `tls/` con la
versión auto-renovada.

---

## 5. Locks y concurrencia

### 5.1 Diagrama de owners

| Recurso | Lock | Holder | Notas |
|---|---|---|---|
| `config.json` | `fs2::FileExt::lock_exclusive` (timeout 5 s) | `Config::write_site` | roto tras release; un kill lo libera |
| `groups.json` | idem | `GroupsFile::write` | |
| `panel.json` | idem | `PanelConfig::write_endpoint` | |
| `dump-log.jsonl` | idem | `DumpLog::append`, `DumpLog::clean` | |
| `operations/{id}.jsonl` | no (per-op) | `OperationCoordinator::run_plan` | un proceso, no concurrente |
| `~/panel-wp/{slug}/` (carpeta de proyecto) | `find_free_slot` antes de crear | `create_site`, `clone`, `worktree` | sin lock persistente, solo atomicidad |
| `panel-net` (Docker) | red global | `DockerManager::ensure_network` | idempotente, no lock |
| `panel-nginx` (container) | sección crítica en `ensure_nginx` | `DockerManager::ensure_nginx` | `Lock + ensure_nginx + Reload` |

### 5.2 Locking utilities

```rust
// ports/filesystem.rs
#[async_trait]
pub trait FileSystem: Send + Sync {
    fn try_lock(&self, path: &Path, timeout: Duration) -> Result<FileLock, AppError>;
    fn try_lock_shared(&self, path: &Path, timeout: Duration) -> Result<FileLock, AppError>;
}

pub struct FileLock {
    _file: fs2::File,
    path: PathBuf,
}

impl Drop for FileLock {
    fn drop(&mut self) { self._file.unlock().ok(); }
}
```

### 5.3 Política de timeout

- Locks de escritura: 5 s. Falla con `AppError::Busy`.
- Locks de lectura (compartidos): 2 s.
- Compensación: si una operación tiene un lock y el programa muere,
  el SO libera el lock al cerrar el fd. No hay deadlocks «permanentes»
  en el panel en sí; pero un `kill -9` en medio de un `rename` puede
  dejar un `.tmp` huérfano, que `startup_recovery` borra.

### 5.4 Concurrencia entre Tauri y D-Bus

El panel corre en un solo proceso. Tauri y D-Bus son **uno** (el runtime
de Tauri). Las dos surfaces llaman a la misma `AppContext` y al mismo
`OperationCoordinator`. La cola interna del coordinator serializa las
operaciones largas (no se inicia un `start_site` mientras hay un
`migrate` activo).

```rust
impl OperationCoordinator {
    pub async fn run_plan(&self, ctx: Arc<AppContext>, kind: OperationKind, plan: Plan)
        -> Result<Operation, AppError>
    {
        let permit = self.semaphore.acquire().await?;
        let _permit = permit.forget(); // held until op completes
        // ...
    }
}

// Límite: 1 operación a la vez por sitio, 4 en total.
struct OperationCoordinator {
    per_site: Arc<DashMap<SiteId, Semaphore>>,
    global: Arc<Semaphore>,
}
```

---

## 6. Escrituras atómicas

### 6.1 Patrón

```rust
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let tmp = temp_path(path);
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("data"));
    parent.join(format!(".{}.tmp", name.to_string_lossy()))
}
```

- `O_CREAT | O_EXCL` evita pisar un `.tmp` huérfano de un crash.
- `sync_all()` fuerza fsync antes del rename.
- `rename` es atómico en POSIX y en modern Windows.

### 6.2 Variante para directorios

Para `nginx/conf.d/`, `operations/`, `db-data/`:

```rust
fn ensure_dir(path: &Path) -> Result<(), AppError> {
    if path.exists() { return Ok(()); }
    std::fs::create_dir_all(path)?;
    Ok(())
}
```

### 6.3 Variante para `dump-log.jsonl`

Append-only:

```rust
fn append_line(path: &Path, line: &str) -> Result<(), AppError> {
    let lock = fs.try_lock(path, Duration::from_secs(5))?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    f.sync_all()?;
    Ok(()) // lock libera con drop
}
```

`compact_dump_log` (periódico) abre el lock exclusivo, lee todas las
entradas, escribe un tmp, hace rename.

---

## 7. Migración de disco al arranque

### 7.1 `startup_recovery`

Función `application::lifecycle::startup_recovery(ctx) -> RecoveryReport`:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub configs_migrated: Vec<String>,
    pub operations_resumed: Vec<OperationId>,
    pub drifts_detected: Vec<Drift>,
    pub tmp_files_removed: Vec<PathBuf>,
    pub locks_released: Vec<PathBuf>,
}
```

Pasos:

1. **Migrar configs**: para cada `~/panel-wp/*/config.json`:
   - leer raw,
   - detectar `schemaVersion`,
   - migrar a `CURRENT_SCHEMA` (idempotente),
   - re-escribir atómicamente con `schemaVersion`.
2. **Recolectar operations huérfanos**: `operations/{id}.jsonl` sin
   `status: succeeded` o `failed`. Si `startedAt < now - 1h`, marcar
   como `failed` con `error: 'recovered: interrupted'`.
3. **Reanudar operations activas**: si la recent operación tiene
   `status: running` y `startedAt < now - 1m`, intentar `cancel()` y
   compensar.
4. **Limpiar `.tmp` huérfanos**: en `config_dir` y `projects_root`,
   borrar `*.tmp` de hace > 1 día.
5. **Limpiar `.lock` huérfanos**: en `config_dir`, borrar `*.lock` de
   hace > 1 día. El kernel ya los libera, pero el fs visible queda
   limpio.
6. **Borrar entries de `dump-log.jsonl` con archivos inexistentes**:
   iterar entradas, hacer `meta` check, prune.
7. **Detectar drifts**: el reconciliador corre con `autofix=true` para
   drifts `Info` (auto) y reporta los demás.

### 7.2 Idempotencia

`startup_recovery` es **idempotente**: si lo corres dos veces, la
segunda no hace nada. Verificable con un test que arranca, mata, y
rearranca simulando.

### 7.3 Frecuencia

- Al **primer arranque** del binario (`setup()` de Tauri).
- Tras **kill -9** detectado (comparar `startedAt` con tiempo de
  inactividad).
- **Manualmente** vía `reconcile()`.

### 7.4 Logging

Cada paso se loguea como `OpEvent::Log` con `level: info | warn | error`:

```json
{"type":"log","level":"info","text":"Migrados 3 configs de v1 a v2"}
{"type":"log","level":"warn","text":"Detectado drift: orphan container wp-abc123"}
```

La UI muestra el resultado en la pantalla de Configuración al primer
arranque.

---

## 8. Estado vivo vs estado durable

### 8.1 Estado vivo (en memoria)

- `Endpoint` (con `ArcSwap` para HMR).
- `ContainerName → ContainerState` (cache de `docker inspect` con TTL).
- `OperationId → OperationHandle` (activas).
- `SiteId → AutodumpHandle` (job background).

### 8.2 Estado durable

- `config.json` por proyecto.
- `panel.json` (estado global).
- `groups.json` (orden + conjunto).
- `dump-log.jsonl`.
- `operations/{id}.jsonl` (journal).
- `meta.json` (snapshots).
- `nginx/conf.d/{id}.conf` (derivado, regenerable).
- `ssl/{cert,key}.pem` (regenerable con mkcert).
- `db-data/{container}/` (datos DB vivos, no snapshot).

### 8.3 SSoT (single source of truth)

| Dato | SSoT |
|---|---|
| Lista de proyectos | `~/panel-wp/*/config.json` |
| Versión PHP de un proyecto | `config.json::services.php.version` |
| ¿El proyecto está corriendo? | `docker inspect wp-{id}.State.Running` |
| Endpoint del panel | `panel.json::endpoint` |
| Grupos | `groups.json::order` |
| ¿El sitio tiene WP? | `exists(config.json) + exists(public_dir/wp-config.php)` |
| DB del proyecto | `db-data/{container}/{schema}/` (en host) |
| Dump más reciente | `app/sql/db-*.sql` (ordenado por mtime) |
| Última operación | `operations/last.json` (simplificación) |

### 8.4 Cache de introspección

El `DockerManager` (hoy) consulta `docker inspect` por cada `is_running`.
El rebuild lo acelera con **`Cache<DashMap<ContainerName, (State, TTL)>>`**:

```rust
impl ContainerEngine for BollardContainer {
    async fn is_running(&self, name: &str) -> bool {
        if let Some((state, ts)) = self.cache.get(name) {
            if ts.elapsed() < Duration::from_secs(2) {
                return state.running;
            }
        }
        let state = self.fresh_inspect(name).await;
        self.cache.insert(name, (state.clone(), Instant::now()));
        state.running
    }
}
```

TTL de 2 s para evitar golpear el daemon en cada `get_sites`.

---

## 9. Trazabilidad de cambios

### 9.1 Eventos

Cada mutación de estado va al journal (`operations/{id}.jsonl`) y al
canal `op-log` (frontend). Ambos comparten el mismo `OpEvent`.

```rust
pub enum OpEvent {
    Plan { steps: Vec<Step> },
    Step { idx: usize, total: usize, label: String, status: StepStatus },
    Progress { idx: usize, ratio: f32, units: Option<String> },
    Line { text: String },
    Done { status: OpStatus, result: Option<Value> },
    Failed { error: AppErrorDto, compensation: Vec<Step> },
    Cancelled,
}
```

### 9.2 Auditoría

Cada `Site::write` registra:
- `op_id` (la operación que produjo el cambio).
- `timestamp`.
- `before` / `after` (diff de los campos cambiados).

El log de auditoría se persiste en `audit.jsonl` (rotado, máximo 100 MB).

```json
{"ts":"2026-07-23T12:34:56Z","opId":"uuid","siteId":"uuid","actor":"user","action":"start_site","diff":{"state":"stopped -> running"}}
{"ts":"2026-07-23T12:35:00Z","opId":"uuid","siteId":"uuid","actor":"dbus","action":"stop_site","diff":{"state":"running -> stopped"}}
```

`actor` ∈ `user | dbus | cli | mcp | reconcile` para distinguir el
origen del cambio.

### 9.3 Replay determinista (futuro)

`journal` + `audit` permiten reproducir operaciones si los snapshots
no son suficientes. Fuera del rebuild; backlog.

### 9.4 UI de auditoría

`/settings` muestra las últimas 50 entradas con filtros (por actor,
por site, por action). Acción: «ver journal completo» abre un modal
con el JSONL.

---

## 10. Lifecycle de un proyecto en disco

### 10.1 Estados

```
[New]--create_site()-->[Pending]--migrate() (si migrationPending)-->[Ready]
                                 |                                       |
                                 v                                       v
                          start_site()                          start_site()
                                 |                                       |
                                 v                                       v
                              [Running]----------------------------+
                                 | stop_site()                        |
                                 v                                    v
                              [Stopped]                            [Failed]
                                 |                                    |
                                 v                                    v
                          delete_site()                           reconcile()
                                 |                                    |
                                 v                                    v
                          [Disconnected] / [Gone]                [Recovered]
```

### 10.2 Transiciones explícitas

| From | To | Comando | Acción |
|---|---|---|---|
| — | `Pending` | `create_site` | filesystem + config |
| `Pending` | `Ready` | `migrate` | DB + dump + ssl |
| `Ready` | `Running` | `start_site` | arranca container |
| `Running` | `Stopped` | `stop_site` | export-al-detener + stop |
| `Stopped` | `Running` | `start_site` | igual que antes |
| `Running` | `Failed` | error irrecuperable | journal |
| `Failed` | `Ready` | `reconcile` o `repair_*` | manual |
| `Ready` | `Disconnected` | `delete_site(deleteFolder=false)` | sidecar |
| `Disconnected` | `Ready` | `import_disconnected_site` | restaura config |
| `Ready` | `Gone` | `delete_site(deleteFolder=true)` | elimina todo |

### 10.3 Invariantes

- Un sitio con `migrationPending: true` no expone URL pública confiable.
- Un sitio en `Running` siempre tiene `container_name() exists + running`.
- Un sitio en `Stopped` siempre tiene `config.json` válido.
- Un sitio `Disconnected` tiene `config.disconnected.json` válido y NO
  `config.json`.
- Un sitio `Clone` tiene `clone_of.parent_id` apuntando a un sitio
  válido.

---

## 11. Tests de contrato

### 11.1 Golden tests

```rust
// tests/contract/start_site.rs
#[test]
fn start_site_accepts_valid_input() {
    let v = serde_json::to_value(StartSiteInput { site_id: SiteId::parse("uuid").unwrap() }).unwrap();
    assert_snapshot!("start_site_request", v);
}

#[test]
fn start_site_rejects_invalid_id() {
    let err = serde_json::from_value::<StartSiteInput>(json!({ "id": "not-a-uuid" }));
    assert!(matches!(err, Err(_)));
}
```

### 11.2 Property-based tests

```rust
// tests/properties/config.rs
proptest! {
    #[test]
    fn slug_roundtrip(s in "[a-z0-9-]{1,64}") {
        let slug = Slug::parse(&s).unwrap();
        assert_eq!(slug.as_str(), s);
    }
}
```

### 11.3 Schema migration tests

```rust
#[test]
fn migrate_v1_to_v2_is_idempotent() {
    let v1 = fixture("config-v1.json");
    let v2a = migrate_to_current(v1.clone()).unwrap();
    let v2b = migrate_to_current(serde_json::to_value(&v2a).unwrap()).unwrap();
    assert_eq!(v2a, v2b);
}
```

---

## 12. Próximo paso

El capítulo 04 (Orquestación Docker y operaciones) detalla los steps
del OperationCoordinator, los watchers de auto-dump, el reconciliador
con sus drifts, los platform ports, y el manejo de los eventos
`op-log` / `log:{id}` / `sites-changed`.
