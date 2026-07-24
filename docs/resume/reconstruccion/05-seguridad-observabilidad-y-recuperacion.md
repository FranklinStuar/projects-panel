# 05 · Seguridad, observabilidad y recuperación

> Documento 5 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este capítulo cierra los pilares transversales del rebuild:
> secretos, validación de paths/builds, streaming backups, observabilidad
> estructurada, ports multiplataforma, recovery al arranque y secretos.

---

## 1. Seguridad local-first

### 1.1 Principios

- **Nada sale del host sin consentimiento explícito.** El panel no hace
  telemetry. Las únicas llamadas externas son las documentadas en
  `wordpress.rs::fetch_versions` (wordpress.org) y `download_core`
  (wordpress.org) y `gh::clone`/`pull` (GitHub).
- **Secretos locales no son secretos.** El usuario root de MySQL tiene
  password `panel`; la `MYSQL_ROOT_HOST=%` lo deja abierto a la red
  interna. No es producción. La documentación lo explicita.
- **Secretos externos al keyring.** Las API keys del proveedor de IA
  (futuro) van al keyring del SO (libsecret/Keychain). El panel no
  las persiste en disco.
- **Superficie mínima al host.** Solo los containers `panel-*` publican
  puertos, todos a `127.0.0.1`. El `wp-{id}` no publica nada.
- **mkcert como CA local.** Documentado y aceptado.

### 1.2 Inventario de secretos

| Secreto | Ubicación | Rotación |
|---|---|---|
| `panel` (DB root) | efímero (container env) | por recreado |
| `panel-secret` (MinIO) | efímero (container env) | por recreado |
| CA mkcert | `mkcert -CAROOT` | nunca (es local) |
| API key IA (futuro) | keyring (`libsecret`) | manual |
| Cert por dominio | `~/panel-wp/{slug}/ssl/` | por `regenerate_ssl` |
| GitHub PAT (futuro) | keyring | manual |

### 1.3 Proveedor de keyring

```rust
// ports/keyring.rs
#[async_trait]
pub trait KeyringAccessor: Send + Sync {
    fn set(&self, account: &str, secret: &str) -> Result<(), AppError>;
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn delete(&self, account: &str) -> Result<(), AppError>;
}
```

Adapter `keyring_libsecret.rs` (Linux):

```rust
pub struct LibsecretKeyring {
    conn: zbus::blocking::Connection,
    service: String,
}

impl KeyringAccessor for LibsecretKeyring {
    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let attrs = HashMap::from([
            ("application".to_string(), "wordpress-panel".to_string()),
            ("account".to_string(), account.to_string()),
        ]);
        let item = oo7::Item::with_attributes(&self.service, attrs)?;
        let mut attributes = HashMap::new();
        attributes.insert("account", account);
        item.set_attributes(attributes)?;
        item.set_secret(secret)?;
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let items = oo7::Item::search(&self.conn, &self.service, HashMap::from([
            ("application".to_string(), "wordpress-panel".to_string()),
            ("account".to_string(), account.to_string()),
        ]))?;
        Ok(items.into_iter().next().map(|i| i.secret().unwrap_or_default().to_string()))
    }
    fn delete(&self, account: &str) -> Result<(), AppError> {
        let items = oo7::Item::search(&self.conn, &self.service, HashMap::from([
            ("application".to_string(), "wordpress-panel".to_string()),
            ("account".to_string(), account.to_string()),
        ]))?;
        for item in items { item.delete().await?; }
        Ok(())
    }
}
```

Adapter `keyring_mock.rs` (tests):

```rust
pub struct MockKeyring {
    store: Arc<Mutex<HashMap<String, String>>>,
}
```

### 1.4 Uso del keyring

- `domain::secret::store_api_key(provider: &str, key: &str)` →
  `KeyringAccessor::set`.
- `domain::secret::load_api_key(provider: &str) -> Option<String>` →
  `KeyringAccessor::get`.
- `domain::secret::delete_api_key(provider: &str)` →
  `KeyringAccessor::delete`.

### 1.5 Threat model

| Amenaza | Mitigación |
|---|---|
| Otro usuario lee `~/panel-wp/...` | Permisos Unix 0700 en `~/panel-wp/{slug}` y 0600 en `config.json`. |
| Aplicación maliciosa lee `panel-inginx` | Bind solo a `127.0.0.1`. |
| Otro usuario lee el certificado | mkcert CA es local; no se confía fuera del host. |
| Fuga de API key IA | Keyring, no disco. |
| Comando wp-cli inyectado a través de un plugin | `wpcli::run` con `--skip-plugins --skip-themes` cuando se usa en pasos sensibles (migración). Timeouts duros. |
| `docker cp` con un destino arbitrario | Solo lo invoca `db_migrate_volume` con paths controlados. |
| `mkcert` con dominio arbitrario | Validación: `DomainName::validate` antes. |
| pkexec con script arbitrario | `scripts/first-run.sh` es estático; `RegenerateSsl` no lo usa. |

### 1.6 Auditoría

`audit.jsonl` registra quién hizo qué:

```json
{"ts":"2026-07-23T12:34:56Z","opId":"uuid","siteId":"uuid","actor":"cli","action":"start_site","diff":{"state":"stopped -> running"}}
{"ts":"2026-07-23T12:35:00Z","opId":"uuid","siteId":"uuid","actor":"mcp","action":"stop_site","diff":{"state":"running -> stopped"}}
```

`actor` ∈ `user | dbus | cli | mcp | reconcile`. Permite reconstruir
qué superficie produjo cada cambio.

### 1.7 CSP / Capability (Tauri)

- `core:default`, `core:event:default` en `default.json`.
- Sin `shell:allow-execute` (no usamos tauri-plugin-shell para ejecutar).
- Sin `fs:allow-write` (todo el FS lo hace el backend, no la WebView).
- Sin `http:default` (no hacemos fetch desde la WebView; el backend
  usa `reqwest`).

---

## 2. Path y build validation

### 2.1 Validación de paths

Toda escritura o lectura pasa por `domain::value::paths::validate_path`:

```rust
pub fn validate_path(
    path: &Path,
    allowed_roots: &[&Path],
    must_be_relative: bool,
    must_exist: bool,
) -> Result<PathBuf, AppError> {
    if must_be_relative && path.is_absolute() {
        return Err(AppError::Validation { field: "path".into(), message: "debe ser relativo".into() });
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let allowed = allowed_roots.iter().any(|root| {
        let root_c = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        canonical.starts_with(&root_c)
    });
    if !allowed {
        return Err(AppError::Permission { action: format!("write {path:?}"), what: "fuera de roots permitidos".into() });
    }
    if must_exist && !canonical.exists() {
        return Err(AppError::NotFound { what: "path", id: path.display().to_string() });
    }
    Ok(canonical)
}
```

### 2.2 Reglas generales

- `~/panel-wp/{slug}/**` y `~/.config/wordpress-panel/**` son los dos
  únicos roots en los que el backend escribe.
- `snapshot_excludes` se validan contra `public_dir` (no pueden
  apuntar fuera).
- `find_free_slot` solo propone paths bajo `projects_root`.
- `wp-config.php` del worktree se valida contra el patrón de WordPress
  antes de montarse (no se monta un path que no exista como archivo,
  ver docker.rs::create_php_container).

### 2.3 Validación de builds

`gh_set_deploy` acepta `build_cmd: Option<String>`. La validación:

```rust
pub fn validate_build_cmd(cmd: &str) -> Result<(), AppError> {
    if cmd.is_empty() { return Ok(()); }
    if cmd.contains('\0') { return Err(AppError::Validation { field: "build_cmd".into(), message: "no nul byte".into() }); }
    if cmd.contains(";") || cmd.contains("&&") && !cmd.contains("||") {
        // chain warning, no bloqueante
    }
    // Verificar que el binario existe en PATH.
    let first = cmd.split_whitespace().next().unwrap_or("");
    if which(first).is_none() {
        return Err(AppError::Validation {
            field: "build_cmd".into(),
            message: format!("binario '{first}' no encontrado en PATH"),
        });
    }
    Ok(())
}
```

### 2.4 Allowlist de binarios

`adapters/host/process.rs` mantiene una allowlist de binarios
invocados:

```rust
pub const ALLOWED_BINARIES: &[&str] = &[
    "tar", "mysqldump", "mysql", "psql", "git", "gh", "mkcert",
    "docker", "wp", "php", "php-fpm", "nginx", "pg_isready",
    "openssl", "x-terminal-emulator", "konsole", "gnome-terminal",
    "xfce4-terminal", "kitty", "alacritty",
    "lsof", "scutil", "dscacheutil",   // macOS
    "pkexec", "systemctl", "which",    // Linux
];
```

El `ProcessRunner::run` rechaza binarios fuera de la lista con
`AppError::Permission { action: "exec", what: bin }`.

### 2.5 Sandbox de exec

`ContainerEngine::exec` y `ProcessRunner::run` validan:

- Binario en allowlist.
- Argumentos no contienen `\0`.
- Longitud de arguments ≤ 64 KB.
- Working dir validado contra roots.

### 2.6 Rate limiting

El MCP y la CLI pueden gatillar operaciones. El coordinador aplica
un **rate limit** por `op_id` por minuto:

```rust
pub struct RateLimiter {
    pub max_per_site_per_minute: u32,
    pub max_global_per_minute: u32,
    pub last: Arc<DashMap<SiteId, Mutex<VecDeque<Instant>>>>,
}
```

Default: 10 ops/min por sitio, 30 ops/min global. Configurable en
`panel.json`.

---

## 3. Streaming backups

### 3.1 Por qué streaming

`backup::dump_bytes` carga todo en memoria. Para sitios típicos
(WordPress < 100 MB) está bien. Para un sitio con 1 GB de DB, son
1 GB de RAM durante el volcado. El rebuild introduce un modelo
streaming.

### 3.2 `StreamDump` step

```rust
pub struct StreamDump {
    pub container: ContainerName,
    pub schema: String,
    pub dest: PathBuf,
    pub opts: DumpOpts,
}

pub struct DumpOpts {
    pub preamble: Vec<u8>,
    pub epilogue: Vec<u8>,
    pub chunk_size: usize,
    pub tick: Duration,
    pub cancel: CancellationToken,
    pub progress: ProgressSink,
}
```

Implementación (`application::backup::stream_dump`):

```rust
pub async fn execute(&self, ctx: &OpContext) -> Result<(), AppError> {
    let mut child = ctx.proc.run_background(ProcessCommand {
        program: "docker".into(),
        args: vec![
            "exec".into(), "-i".into(),
            self.container.to_string(),
            "mysqldump".into(),
            "-uroot".into(), "-ppanel".into(),
            "--single-transaction".into(),
            "--no-tablespaces".into(),
            "--skip-dump-date".into(),
            self.schema.clone(),
        ],
        cwd: None,
        env: vec![],
    }).await?;
    let mut stdin = child.stdin().await?;
    let mut stdout = child.stdout().await?;

    let mut tmp = ctx.fs.open_write(&self.dest.with_extension("tmp")).await?;
    let total = ctx.probe_schema_size(&self.container, &self.schema).await?;
    let mut written: u64 = 0;
    let mut buf = vec![0u8; self.opts.chunk_size];

    stdin.write_all(&self.opts.preamble).await?;
    let mut chunk = Vec::with_capacity(self.opts.chunk_size);
    // Read from stdout in chunks, write to tmp.
    loop {
        if self.opts.cancel.is_cancelled() { return Err(AppError::Cancelled { ... }); }
        let n = match stdout.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(e.into()),
        };
        tmp.write_all(&buf[..n]).await?;
        written += n as u64;
        if written % (1 << 20) == 0 {
            tmp.sync_data().await?;
            self.opts.progress.progress(written as f32 / total as f32, Some("bytes"));
        }
    }
    stdin.write_all(&self.opts.epilogue).await?;
    drop(stdin);
    let status = child.wait().await?;
    if !status.success() { return Err(AppError::Database("mysqldump failed".into())); }
    tmp.sync_all().await?;
    drop(tmp);
    ctx.fs.rename(&self.dest.with_extension("tmp"), &self.dest).await?;
    Ok(())
}
```

### 3.3 Cancelación

El `cancel` se chequea cada chunk. Si el usuario cancela, el dump se
aborta, el `.tmp` se borra, y la compensación corre.

### 3.4 Watchdog

El `ProbeSchemaSize` mide el esquema periódicamente. Si la DB no
crece en `idle_timeout` y tampoco llega stdout, el dump se aborta con
`AppError::Database("no progress")`.

### 3.5 `ImportDump` streaming

Mismo patrón espejado, con `mysqldump` reemplazado por
`mysql --database=...` y la fuente siendo un `tar` archivado o el
`.sql` directo.

### 3.6 `SnapshotDatabase` step

```rust
Step::StreamDump { container, schema, dest: snap_dir.join("db.sql"), opts: DumpOpts { ... } }
```

Reutiliza la infraestructura. El snapshot es un plan de 3 steps:

1. `CreateDir { snap_dir }`.
2. `StreamDump { ... }`.
3. `SnapshotCode { ... }` (tar con excludes).
4. `WriteSnapshotMeta { ... }`.

### 3.7 `ExportDb` (manual)

Mismo step que `SnapshotDatabase`, pero con `dest = app/sql/db-{timestamp}.sql`.
Se triggea desde `export_db` y al `stop_site`.

---

## 4. Observabilidad

### 4.1 `tracing` estructurado

```rust
// adapters/observability.rs
use tracing::{info, warn, error, instrument};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub struct TracingObservability {
    pub journal: Arc<OperationJournal>,
    pub metrics: Arc<Metrics>,
}

impl Observability for TracingObservability {
    fn info(&self, target: &str, msg: &str) {
        tracing::info!(target = target, "{msg}");
    }
    fn warn(&self, target: &str, msg: &str) {
        tracing::warn!(target = target, "{msg}");
    }
    fn error(&self, target: &str, msg: &str) {
        tracing::error!(target = target, "{msg}");
    }
    fn metric(&self, name: &str, value: MetricValue) {
        self.metrics.record(name, value);
    }
    fn oplog(&self, op: OperationId, evt: OpEvent) -> Result<(), AppError> {
        self.journal.append(op, &evt)?;
        // Also emit to Tauri.
        self.event_tx.send((op, evt))?;
        Ok(())
    }
}
```

### 4.2 Init en `setup()`

```rust
pub fn init_obs(config: &ObsConfig) -> Result<TracingObservability, AppError> {
    let log_path = config.config_dir.join("logs/panel.log");
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
    let (json, _guard) = tracing_appender::non_blocking(file);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,wordpress_panel_lib=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(json).with_ansi(false).json())
        .with(fmt::layer().with_writer(std::io::stderr).compact())
        .init();

    Ok(TracingObservability { /* ... */ })
}
```

### 4.3 Métricas operativas

```rust
pub struct Metrics {
    pub counters: Arc<DashMap<String, i64>>,
    pub histograms: Arc<DashMap<String, Histogram>>,
}

impl Metrics {
    pub fn record(&self, name: &str, value: MetricValue) {
        match value {
            MetricValue::Count(n) => self.counters.entry(name.to_string()).and_modify(|c| *c += n).or_insert(n),
            MetricValue::Duration(d) => self.histograms.entry(name.to_string()).or_insert_with(Histogram::new).record(d),
        }
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            counters: self.counters.iter().map(|e| (e.key().clone(), *e.value())).collect(),
            histograms: self.histograms.iter().map(|(k, h)| (k.clone(), h.summary())).collect(),
        }
    }
}
```

Métricas emitidas:

- `panel.op.duration_seconds{ kind, status }` (histogram).
- `panel.op.count{ kind, status }` (counter).
- `panel.autodump.bytes{ site_id }` (counter).
- `panel.autodump.dumps{ site_id, source }` (counter).
- `panel.reconcile.drifts{ kind, severity }` (counter).
- `panel.docker.ops{ method, status }` (counter).

### 4.4 Persistencia de métricas

Cada minuto, `Metrics` se serializa a `~/.config/wordpress-panel/metrics.jsonl`
(append) con un TTL de 7 días (rotación).

### 4.5 Comando IPC

```rust
#[tauri::command]
pub async fn metrics_snapshot(ctx: State<'_, AppContext>) -> Result<MetricsSnapshot, AppError> {
    Ok(ctx.obs.metrics().snapshot())
}
```

La UI muestra un tile en `/settings` con las métricas más relevantes.

### 4.6 Logs estructurados

Cada log es un JSON:

```json
{
  "timestamp": "2026-07-23T12:34:56.789Z",
  "level": "info",
  "target": "application::lifecycle::start",
  "fields": { "site_id": "uuid", "step": 3, "label": "EnsureDb" },
  "message": "step ok"
}
```

Rotación: `logs/panel.{timestamp}.log` los últimos 7. Más viejos se
borran en `startup_recovery`.

### 4.7 `errors` agregados

Los `AppError` se loguean con su cadena de `cause`:

```rust
fn log_error(&self, err: &AppError) {
    let mut source = err.source();
    let mut e = err;
    let mut depth = 0;
    while let Some(s) = source {
        tracing::error!(parent = ?e, source = ?s, "error chain depth {depth}");
        e = s;
        source = e.source();
        depth += 1;
        if depth > 10 { break; }
    }
}
```

### 4.8 Tracing span por operación

```rust
#[instrument(skip(ctx, plan), fields(op_id = %op.id, kind = %kind))]
async fn run_plan(&self, ctx: Arc<AppContext>, kind: OperationKind, plan: Plan) -> Result<Operation, AppError> {
    // ...
}
```

Cada step crea un child span:

```rust
#[instrument(skip(oc), fields(step_idx = idx, step_label = %label))]
async fn execute_step(&self, idx: usize, step: &Step, oc: &OpContext) -> Result<(), AppError> {
    // ...
}
```

### 4.9 OpenTelemetry? No.

El rebuild **no** introduce OpenTelemetry. La razón: añade una
dependencia pesada (collector, exporter), y un panel de escritorio
local no lo necesita. Las métricas se quedan en disco y se consumen
por la UI. Si en el futuro se quiere exportar a OTLP, el adapter es
trivial.

### 4.10 Auditoría

`audit.jsonl` registra cambios de estado de los `Site`. Ver §1.6.

---

## 5. Platform ports

### 5.1 Linux

| Componente | Implementación |
|---|---|
| Filesystem | `nix` con `renameat2(RENAME_NOREPLACE)` cuando esté; fallback `write + rename`. |
| Port checker | `/proc/net/tcp{,6}` (migrado de `netcheck.rs`). |
| DNS resolver | `getent hosts panel-probe.test` + `/etc/NetworkManager/dnsmasq.d/`. |
| Cert authority | `mkcert -CAROOT`. |
| Process runner | `tokio::process::Command`. |
| Shell | `xdg-open` (vía `tauri-plugin-opener`). |
| pkexec | `pkexec -- <cmd>` con timeout. |
| NetworkManager | `nmcli` para reload (opcional). |

### 5.2 macOS

| Componente | Implementación |
|---|---|
| Filesystem | `write + rename`. |
| Port checker | `lsof -nP -iTCP -sTCP:LISTEN` parseado. |
| DNS resolver | `scutil --dns` + `dscacheutil -q host -a name`. |
| Cert authority | `openssl` manual + `security add-trusted-cert`. |
| Process runner | `tokio::process::Command`. |
| Shell | `open` (vía `tauri-plugin-opener`). |
| NetworkManager | n/a. |
| can_install_system_service | false. |

### 5.3 Windows

Sin soporte funcional. La portabilidad a Windows es **futuro**. Los
stubs devuelven `AppError::Unsupported` con un mensaje claro.

### 5.4 Detección de plataforma

```rust
pub fn current_platform() -> Box<dyn Platform> {
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxPlatform::new());
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacOsPlatform::new());
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Box::new(unsupported::UnsupportedPlatform::new());
}
```

La elección del binario `mkcert` se hace en `CertAuthority::ensure`:

```rust
let cmd = match platform {
    Platform::Linux => "mkcert",
    Platform::MacOs => "mkcert", // brew install mkcert
    _ => return Err(AppError::Unsupported { what: "cert authority".into() }),
};
```

### 5.5 `dns::wildcard::ensure`

```rust
pub struct WildcardRule { ip: Ipv4Addr }
pub struct DnsmasqConfig { /* config */ }

impl DnsConfigurator for DnsmasqConfig {
    async fn ensure_wildcard(&self, ip: &Ipv4Addr) -> Result<(), AppError> {
        if self.resolves_to(&format!("panel-probe.test"), ip.as_str()) {
            return Ok(());
        }
        let snippet = format!("address=/{}/{}\n", self.zone, ip);
        self.platform.write_snippet(&snippet).await?;
        self.platform.reload_dns().await?;
        Ok(())
    }
}
```

Linux: `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf`
macOS: brew-managed dnsmasq (futuro).

---

## 6. Startup recovery

### 6.1 Punto de partida

El rebuild formaliza el `startup_recovery` que ya está documentado
en `03-contratos-estado-y-persistencia.md` §7. Aquí vemos el detalle
de los pasos.

### 6.2 Pasos

```rust
pub async fn startup_recovery(ctx: Arc<AppContext>) -> Result<RecoveryReport, AppError> {
    let mut report = RecoveryReport::default();

    // 1. Migrar configs a schemaVersion actual.
    let entries = ctx.fs.read_dir(&ctx.config.projects_root().await?).await?;
    for entry in entries {
        let cfg_path = entry.path().join("config.json");
        if !cfg_path.exists() { continue; }
        let raw = ctx.fs.read_to_string(&cfg_path).await?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        let v = detect_schema_version(&value)?;
        if v < CURRENT_SCHEMA {
            let migrated = migrate_to_current(value)?;
            let bytes = serde_json::to_vec_pretty(&migrated)?;
            ctx.fs.atomic_write(&cfg_path, &bytes).await?;
            report.configs_migrated.push(cfg_path.display().to_string());
        }
    }

    // 2. Recolectar operations huérfanos.
    let journal_dir = ctx.config.config_dir().await?.join("operations");
    let orphans = ctx.fs.read_dir(&journal_dir).await?;
    for entry in orphans {
        let id = SnapshotId::from_path(&entry.path())?;
        let parsed = parse_journal(&entry.path())?;
        if parsed.status == OpStatus::Running {
            // Marcar como failed.
            append_failed_marker(&entry.path())?;
            report.operations_resumed.push(id);
        }
    }

    // 3. Limpiar .tmp huérfanos (> 1 día).
    let tmp_files = ctx.fs.glob("*.tmp", &ctx.config.config_dir().await?).await?;
    for f in tmp_files {
        if metadata(&f).modified().unwrap_or(SystemTime::UNIX_EPOCH) < now - Duration::from_secs(86400) {
            ctx.fs.remove(&f).await?;
            report.tmp_files_removed.push(f);
        }
    }

    // 4. Limpiar .lock huérfanos (> 1 día).
    let lock_files = ctx.fs.glob("*.lock", &ctx.config.config_dir().await?).await?;
    for f in lock_files {
        if metadata(&f).modified().unwrap_or(SystemTime::UNIX_EPOCH) < now - Duration::from_secs(86400) {
            ctx.fs.remove(&f).await?;
            report.locks_released.push(f);
        }
    }

    // 5. Dump log: prune entradas con archivos inexistentes.
    let dump_log = ctx.dump_log.read_all().await?;
    let mut keep = Vec::new();
    for entry in dump_log {
        if entry.file.exists() {
            keep.push(entry);
        } else {
            report.drifts.push(Drift::StaleDumpLog { entry });
        }
    }
    ctx.dump_log.write_all(&keep).await?;

    // 6. Reconciliador.
    let reconcile = application::reconcile::reconcile(ctx.clone(), ReconcileConfig {
        autofix: true,
        include_info: false,
    }).await?;
    report.drifts.extend(reconcile.drifts);
    report.fix_results = reconcile.fixed;

    Ok(report)
}
```

### 6.3 Tests

`tests/integration_recovery.rs`:

```rust
#[tokio::test]
#[ignore = "requires fs, no docker"]
async fn recovery_migrates_v1_config() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = build_test_context_with_root(tmp.path()).await.unwrap();
    let cfg_path = tmp.path().join("site").join("config.json");
    let raw = include_str!("fixtures/config-v1.json");
    tokio::fs::write(&cfg_path, raw).await.unwrap();
    let report = startup_recovery(ctx.clone()).await.unwrap();
    assert!(report.configs_migrated.iter().any(|s| s.contains("config.json")));
    let after = tokio::fs::read_to_string(&cfg_path).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(parsed.get("schemaVersion").unwrap().as_u64().unwrap(), 2);
}
```

### 6.4 Visibilidad

El `RecoveryReport` se retorna al `setup()` y la UI lo pinta en un
modal de bienvenida la primera vez que se ejecuta:

```svelte
{#if showRecovery}
  <Modal title="Recuperación al inicio">
    <ul>
      {#each report.configs_migrated as c}
        <li>Migrado {c}</li>
      {/each}
      {#each report.drifts as d}
        <li>Drift: {d.kind}</li>
      {/each}
    </ul>
  </Modal>
{/if}
```

Si no hay nada que reportar, el modal no aparece.

---

## 7. Filesystem layout

### 7.1 `~/panel-wp/`

```
~/panel-wp/
├── {slug}/
│   ├── config.json                  # SiteConfig (schemaVersion 2)
│   ├── config.bak.{ts}.json         # backups
│   ├── app/
│   │   ├── public/                  # WordPress (bind :php y :nginx)
│   │   │   ├── wp-config.php        # Generado por wp-cli
│   │   │   ├── wp-content/
│   │   │   └── ...
│   │   └── sql/
│   │       ├── db-{timestamp}.sql   # dumps
│   │       └── imported.sql         # del import LocalWP
│   ├── conf/
│   │   └── php/
│   │       └── php.ini              # plantila + extras
│   ├── logs/
│   │   └── php/                     # logs
│   ├── ssl/
│   │   ├── cert.pem
│   │   └── key.pem
│   ├── wt/
│   │   └── {basename}/              # git worktree (worktree-project)
│   └── snapshots/
│       └── {snap_id}/
│           ├── code.tar.zst
│           ├── db.sql
│           └── meta.json
```

### 7.2 `~/.config/wordpress-panel/`

```
~/.config/wordpress-panel/
├── panel.json                       # estado global
├── groups.json                      # {order: [...], schemaVersion: 1}
├── dump-log.jsonl                   # log de volcados
├── operations/
│   ├── {op_id}.jsonl                # journal
│   └── last.json                    # última op (cache)
├── metrics.jsonl                    # métricas operativas
├── audit.jsonl                      # auditoría
├── nginx/
│   └── conf.d/
│       ├── 00-panel-tuning.conf     # tuning
│       └── {site_id}.conf           # vhosts
├── db-data/
│   └── panel-{type}-{ver}/          # datadir bindeado
├── minio-data/                      # dato MinIO
├── dnsmasq-panel.conf               # snippet
├── wp-versions.json                 # cache 24h
├── wp-cli.phar                      # montado en cada wp-{id}
├── logs/
│   └── panel.{ts}.log               # logs estructurados
└── tmp/                             # para escrituras atómicas
```

### 7.3 Permisos

```rust
fn chmod_site(site_path: &Path) -> Result<(), AppError> {
    let mode = 0o700;
    std::fs::set_permissions(site_path, PermissionsExt::from_mode(mode))?;
    let cfg = site_path.join("config.json");
    std::fs::set_permissions(&cfg, PermissionsExt::from_mode(0o600))?;
    Ok(())
}
```

Aplicado en `create_site` y `import_disconnected_site`.

### 7.4 Cuarentena

Si un `config.json` no parse (corrupto), el panel lo mueve a
`config.quarantine.{ts}.json` y continúa. La UI muestra un warning
en `/settings`.

---

## 8. Errores y compensación

### 8.1 Tipos de compensadores

- **Triviales**: borrar lo creado (RemoveContainer, RemoveVhost,
  RemoveAll).
- **Reset**: llevar al estado anterior (ResetDatabase, RestoreConfig).
- **Idempotentes**: no-op si el sistema ya está en estado correcto.

### 8.2 Compensación por step

```rust
let compensation_lookup: HashMap<&'static str, Box<dyn Compensator>> = ...;
```

La idea es que cada step declara su compensador **por nombre**:

```rust
Step::CreateSchema { ... }.compensates_with(Step::DropSchema { ... })
```

El coordinator consulta el mapa y, en caso de fallo, ejecuta en
reversa.

### 8.3 Fallo de compensación

Si la compensación falla:

1. El journal marca `CompensationFailed`.
2. `reconcile` reporta el drift.
3. La UI muestra un banner rojo en `/settings` con acción manual.

---

## 9. Detalles de seguridad por superficie

### 9.1 Tauri IPC

- `core:default` permite `invoke` solo desde la ventana `main`.
- `core:event:default` permite `listen` desde la ventana `main`.
- No hay `shell:allow-execute` (la apertura de URLs/paths se hace
  con `tauri-plugin-opener`, que abre el navegador/explorer externo).

### 9.2 D-Bus

- Servicio `com.goldmediatech.WordpressPanel` solo en la sesión
  del usuario (`SESSION_BUS`).
- Métodos devuelven JSON strings (compatibilidad `gdbus`/`qdbus`).
- No expone paths ni secretos.
- Métodos que mutan emiten `sites-changed`.

### 9.3 CLI

- `wp` wrapper: solo detecta el proyecto por CWD y delega al container.
- `wordpress-panel-cli`: usa D-Bus o falla con mensaje claro.
- No tiene parámetros de limpieza.

### 9.4 MCP

- `mcp/server.mjs` ejecuta subprocess del CLI.
- Tiempo de espera de cada tool: 60 s (configurable).
- Sin streaming.
- Salida estructurada en JSON.

### 9.5 Logs

- `panel.log` rotado, máximo 7 días.
- Solo info/warn/error; sin debug en producción.
- Las passwords WP-CLI/admin se loguean enmascaradas (`• • • •`).
- Las API keys IA nunca se loguean.

---

## 10. Resiliencia

### 10.1 Degradación graciosa

| Componente | Caído | Comportamiento |
|---|---|---|
| Docker | Panel → setup falla | UI: "Install Docker" |
| `panel-nginx` | No publica | UI: "Restart nginx" |
| DB | No se puede iniciar proyecto | UI: "Restart DB" |
| Mailpit | El proyecto sigue, mail no capturado | UI: slight warning |
| MinIO | El proyecto tira error 5xx | UI: warning |
| Adminer | Visor de DB no se abre | UI: "Disable Adminer" |
| wp-cli | Comando retorna timeout | UI: error msg |
| DNS (`*.test`) | Sitios no resuelven | UI: warning |
| mkcert | SSL no funciona | UI: warning, fallback HTTP |

### 10.2 Watchdog

`Watchdog` chequea cada 30 s:

- `panel-nginx` caído con sitios activos → reiniciar.
- DB retrasada (> 60 s sin responder) → reiniciar.
- `panel-net` ausente → recrear.

### 10.3 Circuit breaker

El `OperationCoordinator` implementa un circuit breaker:

```rust
pub struct CircuitBreaker {
    pub failures: AtomicU64,
    pub threshold: u64,
    pub cooldown: Duration,
    pub open: AtomicBool,
    pub opened_at: AtomicU64,
}

impl CircuitBreaker {
    pub fn record_failure(&self) { /* ... */ }
    pub fn can_attempt(&self) -> bool { /* ... */ }
}
```

Si un step falla N veces en una ventana, el circuit abre y los
próximos intentos fallan rápido con `AppError::Busy`. El usuario
ve «Servicio en pausa, reintenta en 60s».

### 10.4 Reducción de superfície

El panel no expone nada al host fuera de `127.0.0.1`. Cualquier
intento de bind a `0.0.0.0` desde el código es un error de
programación detectado por tests.

### 10.5 Versionado de capabilities

```rust
pub const CAP_VERSION: u32 = 2;
```

Si la capability `default.json` está desactualizada, el setup
falla con `AppError::Schema`. Esto evita cambios incompatibles sin
migration.

---

## 11. Próximo paso

El capítulo 06 (Estrategia de pruebas y calidad) detalla la pirámide
de tests, los golden tests, los integration tests, los mocks, los
tests de propiedades, los tests de contrato, la cobertura objetivo y
la estrategia de CI.
