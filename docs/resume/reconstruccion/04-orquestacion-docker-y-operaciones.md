# 04 · Orquestación Docker y operaciones

> Documento 4 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este capítulo destila los pasos concretos del
> `OperationCoordinator`, los watchers de auto-dump, el reconciliador,
> los platform ports, y el manejo de los eventos backend → frontend.

---

## 1. Mapa del coordinador

### 1.1 Punto de partida

El código actual reparte la lógica de orquestación en cinco módulos
que reinventan el mismo patrón:

- `docker.rs::start_site / stop_site` (ciclo de vida de un sitio).
- `wordpress.rs::create_site` (alta end-to-end).
- `migrate.rs::run_migration` (import de dump + URL fix).
- `snapshot.rs::run` (tar + dump).
- `clone.rs::run` (snapshot + restart).
- `worktree.rs::run_create` (git worktree + wp-config + start).

Cada uno:

1. loguea un título con `log(app, "▶ ...")`.
2. emite pasos numerados `[1/7] …`.
3. en caso de error, intenta limpiar (`remove_container`, `remove_vhost`,
   `remove_dir_all`).
4. cierra con `✓ ...` o `✗ ...`.

El coordenador unifica este patrón.

### 1.2 Plan declarativo

```rust
pub struct Plan {
    pub steps: Vec<Step>,
    pub compensations: HashMap<StepIndex, Step>,
}

impl Plan {
    pub fn builder() -> PlanBuilder { PlanBuilder::default() }
}

pub struct PlanBuilder {
    steps: Vec<Step>,
    compensations: HashMap<StepIndex, Step>,
}

impl PlanBuilder {
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }
    pub fn step_with_compensation(mut self, step: Step, compensation: Step) -> Self {
        let idx = self.steps.len();
        self.steps.push(step);
        self.compensations.insert(idx, compensation);
        self
    }
    pub fn build(self) -> Plan { Plan { steps: self.steps, compensations: self.compensations } }
}
```

### 1.3 Ejecución

```rust
impl OperationCoordinator {
    pub async fn run_plan(
        self: &Arc<Self>,
        ctx: Arc<AppContext>,
        kind: OperationKind,
        plan: Plan,
    ) -> Result<Operation, AppError> {
        let op = Operation::new(kind, plan.steps.len());
        let id = op.id;
        let cancel = CancellationToken::new();
        let storage = OperationStorage::open(&self.journal_dir, id).await?;
        let handle = OperationHandle { op: op.clone(), cancel: cancel.clone() };
        self.active.lock().await.insert(id, handle);

        // Plan visible para la UI.
        ctx.obs.oplog(id, OpEvent::Plan { steps: plan.steps.iter().map(label_of).collect() })?;

        let mut failed = None;
        for (idx, step) in plan.steps.iter().enumerate() {
            if cancel.is_cancelled() {
                failed = Some(AppError::Cancelled { operation: format!("{:?}", kind) });
                break;
            }
            self.execute_step(idx, &step, &ctx, &op, &cancel, &storage).await
                .map_err(|e| { failed = Some(e); e })?;
            storage.append(&JournalEntry::StepFinished { step: idx, ok: true }).await?;
        }

        // Limpieza post-éxito.
        let status = match failed {
            None => {
                ctx.obs.oplog(id, OpEvent::Done { status: OpStatus::Succeeded, result: None })?;
                OpStatus::Succeeded
            }
            Some(err) => {
                self.compensate(&ctx, &op, &plan, &storage).await;
                ctx.obs.oplog(id, OpEvent::Failed { error: err.to_dto(), compensation: vec![] })?;
                OpStatus::Failed { error: err, compensation: vec![] }
            }
        };

        let mut op = op;
        op.status = status;
        op.finished_at = Some(ctx.clock.now());
        storage.append(&JournalEntry::Done { status: status.clone() }).await?;
        self.active.lock().await.remove(&id);
        Ok(op)
    }
}
```

### 1.4 Step execution

```rust
impl OperationCoordinator {
    async fn execute_step(
        &self,
        idx: usize,
        step: &Step,
        ctx: &AppContext,
        op: &Operation,
        cancel: &CancellationToken,
        storage: &OperationStorage,
    ) -> Result<(), AppError> {
        let label = label_of(step);
        ctx.obs.oplog(op.id, OpEvent::Step { idx, total: op.step_count, label: label.clone(), status: StepStatus::Started })?;
        storage.append(&JournalEntry::StepStarted { step: idx, label: label.clone() }).await?;

        let oc = OpContext {
            ctx: ctx.clone(),
            op,
            storage,
            cancel: cancel.clone(),
            progress: ProgressSink::new(ctx.obs.clone(), op.id),
        };

        let result = step.execute(&oc).await;

        ctx.obs.oplog(op.id, OpEvent::Step {
            idx, total: op.step_count, label,
            status: match &result {
                Ok(()) => StepStatus::Ok,
                Err(err) => StepStatus::Failed { retryable: err.is_retryable() },
            },
        })?;

        result
    }
}
```

### 1.5 Compensación

```rust
impl OperationCoordinator {
    async fn compensate(
        &self,
        ctx: &AppContext,
        op: &Operation,
        plan: &Plan,
        storage: &OperationStorage,
    ) {
        for (idx, step) in plan.steps.iter().enumerate().rev() {
            if let Some(comp) = plan.compensations.get(&idx) {
                let oc = OpContext {
                    ctx: ctx.clone(),
                    op,
                    storage,
                    cancel: CancellationToken::new(),
                    progress: ProgressSink::new(ctx.obs.clone(), op.id),
                };
                match comp.execute(&oc).await {
                    Ok(()) => {
                        storage.append(&JournalEntry::Compensated { step: idx, action: comp.clone() }).await.ok();
                    }
                    Err(err) => {
                        ctx.obs.warn("compensation", &format!("step {idx} compensation failed: {err}"));
                        storage.append(&JournalEntry::CompensationFailed { step: idx, error: err.to_string() }).await.ok();
                    }
                }
            }
        }
    }
}
```

---

## 2. Catálogo de steps

### 2.1 Networking

```rust
Step::EnsureNetwork { name: "panel-net" },
Step::CreateNetwork { name: "panel-net", driver: "bridge" },
Step::RemoveNetwork { name: "panel-net" },
```

Implementación en `application::lifecycle::step::networking`.

### 2.2 Containers genéricos

```rust
Step::EnsureImage { image: "nginx:alpine" },
Step::CreateContainer { spec: ContainerSpec },
Step::StartContainer { name: "panel-nginx" },
Step::StopContainer { name: "panel-nginx", timeout: Duration::from_secs(10) },
Step::RemoveContainer { name: "panel-nginx", force: false },
Step::RecreateContainer { name: "panel-nginx", image: "nginx:alpine" },
Step::RecreateContainerIfImageMismatch { name, image },
Step::ExecInContainer { name, cmd, user: Some("www-data") },
Step::ExecCapture { name, cmd, dest: Vec<u8> },
Step::StreamLogs { name, follow: true, tail: 200 },
Step::ExecInBackground { name, cmd, detached: true },
```

`RecreateContainerIfImageMismatch` resuelve el caso del `IMAGE_REV`
(`docker.rs::start_site`): si el container existe pero con image
distinta, recrearlo.

### 2.3 Services compartidos

```rust
Step::EnsureNginx,                       // ensure_nginx + endpoint + tuning
Step::ReloadNginx,                       // nginx -s reload (con recreate si zombie)
Step::EnsureMailpit,                     // arranca si no existe
Step::EnsureMinio,                       // solo si feature flag
Step::EnsureAdminer,                     // on-demand
Step::EnsureDnsmasqWildcard,             // instala regla + reload NM
Step::TeardownUnusedShared,              // apaga lo que ya no se use
```

### 2.4 DB engine

```rust
Step::EnsureDb { db: DbService },         // arrancar container DB compartido
Step::WaitDbReady { container: ContainerName },
Step::CreateSchema { container, schema, charset: "utf8mb4" },
Step::DropSchema { container, schema },
Step::ImportDump { container, schema, dump: PathBuf, opts: ImportOpts },
Step::StreamDump { container, schema, dest: PathBuf, opts: DumpOpts },
Step::ProbeSchemaSize { container, schema }, // para watchdog
```

### 2.5 Filesystem

```rust
Step::CreateDir { path: PathBuf },
Step::AtomicWrite { path: PathBuf, bytes: Vec<u8> },
Step::CopyFile { src: PathBuf, dst: PathBuf },
Step::MoveFile { src: PathBuf, dst: PathBuf },
Step::RemoveAll { path: PathBuf },
Step::SetPermissions { path: PathBuf, mode: u32 },
Step::BackupFile { path: PathBuf, suffix: ".bak.{timestamp}" },
Step::RotateDir { path: PathBuf, keep: usize, pattern: "...", strategy: LastModified },
```

### 2.6 WordPress-specific

```rust
Step::DownloadCore { version: String, dest: PathBuf },
Step::ExtractCore { tarball: PathBuf, dest: PathBuf },
Step::WritePhpIni { site: SitePath, template: PathBuf },
Step::InjectMailpitMuPlugin { site_path: SitePath },
Step::InjectAutologinMuPlugin { site_path: SitePath },
Step::SyncMuPlugins { site_path: SitePath, flags: SiteFlags },
Step::WriteWpConfig { site: SitePath, db: DbService, db_container: ContainerName },
Step::WpCoreInstall { site: SitePath, request: NewSiteRequest },
Step::FixSiteUrl { site: SitePath, url: String },
Step::GenerateSsl { site: SitePath, domain: DomainName },
Step::WriteVhost { site_id: SiteId, config: SiteConfig },
Step::RemoveVhost { site_id: SiteId },
Step::WpCli { site: SitePath, args: Vec<String>, user: Some("www-data"), timeout: Duration::from_secs(120) },
```

### 2.7 Git

```rust
Step::GitWorktreeAdd { repo: PathBuf, dest: PathBuf, branch: String, base: Option<String> },
Step::GitWorktreeRemove { repo: PathBuf, dest: PathBuf },
Step::GitBranchDelete { repo: PathBuf, branch: String },
Step::GitPull { repo: PathBuf, branch: String, ff_only: bool },
Step::GitFetch { repo: PathBuf },
Step::GitCheckout { repo: PathBuf, branch: String },
Step::GitDeploy { repo: PathBuf, branch: String, build_cmd: Option<String>, build_dirs: Vec<String> },
```

### 2.8 Observabilidad

```rust
Step::Emit { event: EventName, payload: Value },
Step::Log { level: LogLevel, text: String },
```

### 2.9 Snapshot

```rust
Step::SnapshotCode { site: SitePath, dest: PathBuf, excludes: Vec<SnapshotExclude> },
Step::SnapshotDatabase { site: SitePath, dest: PathBuf },
Step::WriteSnapshotMeta { snapshot_id: SnapshotId, dest: PathBuf, meta: SnapshotMeta },
Step::AdoptSnapshot { site: SitePath, snapshot_id: SnapshotId },
```

### 2.10 Custom

```rust
Step::Custom { label: String, action: Arc<dyn OperationAction> },
```

Para pasos que no encajan en los enum (p. ej. migración legacy de
datadir via `docker cp`).

---

## 3. Policies reutilizables

### 3.1 `teardown_unused_shared` (policy pura)

```rust
// domain/policy/teardown.rs
pub struct SharedTeardownDecision {
    pub stop_nginx: bool,
    pub stop_mailpit: bool,
    pub stop_adminer: bool,
    pub stop_minio: bool,
    pub stop_db: Vec<ContainerName>,
}

pub fn decide_teardown(
    stopped: &Site,
    active: &[Site],
    current_actual: &ActualState,
) -> SharedTeardownDecision {
    let mut db_in_use: HashSet<ContainerName> = active.iter()
        .filter(|s| current_actual.is_running(&s.container_name()))
        .map(|s| db_container_name(&s.services.db))
        .collect();
    let any_active = !db_in_use.is_empty();
    let any_minio = active.iter()
        .filter(|s| current_actual.is_running(&s.container_name()))
        .any(|s| s.shared.minio);

    let stop_db = if !db_in_use.contains(&db_container_name(&stopped.services.db)) {
        vec![db_container_name(&stopped.services.db)]
    } else { vec![] };

    SharedTeardownDecision {
        stop_nginx: !any_active,
        stop_mailpit: !any_active,
        stop_adminer: !any_active,
        stop_minio: !any_minio,
        stop_db,
    }
}
```

`decide_teardown` es **determinista** y pura. El step `TeardownUnusedShared`
ejecuta la decisión.

### 3.2 `autoselect_endpoint`

```rust
pub fn autoselect_endpoint(
    used_ports: &HashSet<u16>,
    local_ips_free_in: &HashSet<Ipv4Addr>,
) -> Endpoint {
    let ip = local_ips_free_in.iter().next().cloned().unwrap_or(Ipv4Addr::LOCALHOST);
    let http = (8080..9000).find(|p| !used_ports.contains(p)).unwrap_or(8080);
    let https = (8443..9000).find(|p| p != &http && !used_ports.contains(p)).unwrap_or(8443);
    Endpoint { loopback_ip: ip, http_port: http, https_port: https }
}
```

### 3.3 `validate_slug`

```rust
pub fn validate_slug(s: &str) -> Result<Slug, AppError> {
    let s = s.trim().to_lowercase();
    let re = Regex::new(r"^[a-z0-9][a-z0-9-]{0,63}$").unwrap();
    if !re.is_match(&s) {
        return Err(AppError::Validation {
            field: "slug".into(),
            message: "debe coincidir con ^[a-z0-9][a-z0-9-]{0,63}$".into(),
        });
    }
    Ok(Slug(s))
}
```

### 3.4 `find_free_slot`

```rust
pub fn find_free_slot<T: AsRef<Path>>(
    parent: T,
    base: &str,
    used: &[String],
) -> PathBuf {
    let mut path = parent.as_ref().join(base);
    if !path.exists() && !used.iter().any(|u| u == base) {
        return path;
    }
    let mut i = 1;
    loop {
        let candidate = parent.as_ref().join(format!("{base}-{i}"));
        if !candidate.exists() && !used.iter().any(|u| u == &format!("{base}-{i}")) {
            return candidate;
        }
        i += 1;
    }
}
```

`Slug` y `SiteId` son IDs únicos; `find_free_slot` solo añade
desambiguación al slug del filesystem.

---

## 4. Auto-dump

### 4.1 Diseño

El auto-dump vigila la DB de los proyectos **activos** y, cuando
detecta cambios, deja un dump fresco en `app/sql/`. Hoy vive en
`autodump.rs` con un `JoinHandle` en estado Tauri. El rebuild lo
convierte en un job gestionado por el `OperationCoordinator`.

### 4.2 `WatchDatabaseChanges`

```rust
pub struct WatchDatabaseChanges {
    pub site: Site,
    pub interval: Duration,
    pub idle_threshold: Duration,
    pub dump_dest: SqlDir,
    pub keep: usize,
}

impl OperationAction for WatchDatabaseChanges {
    fn label(&self) -> &'static str { "auto-dump" }

    async fn execute(&self, ctx: &OpContext) -> Result<(), AppError> {
        let mut last_writes: Option<u64> = None;
        let mut last_hash: Option<u64> = latest_dump_hash(&self.site.sql_dir());

        loop {
            tokio::select! {
                _ = ctx.cancel.sleep() => {
                    ctx.progress.line("auto-dump cancelled");
                    return Ok(());
                }
                _ = ctx.clock.sleep(self.interval) => {
                    if !ctx.docker.is_running(&db_container_name(&self.site.services.db)).await {
                        continue;
                    }
                    let writes = write_counter(ctx, &self.site).await?;
                    if last_writes == Some(writes) {
                        continue;
                    }
                    last_writes = Some(writes);
                    let dump = dump_bytes(ctx, &self.site).await?;
                    let hash = hash_bytes(&dump);
                    if last_hash == Some(hash) {
                        continue;
                    }
                    last_hash = Some(hash);
                    let stamped = ctx.clock.now().format("%Y%m%d-%H%M%S").to_string();
                    let dest = self.site.sql_dir().join(format!("db-{stamped}.sql"));
                    ctx.fs.atomic_write(&dest, &dump).await?;
                    ctx.dumplog.append(&self.site, &dest, "auto").await?;
                    ctx.fs.rotate_dumps(&self.site.sql_dir(), "db-*.sql", self.keep).await?;
                    ctx.obs.metric("panel.autodump.bytes", dump.len() as i64);
                }
            }
        }
    }

    async fn compensate(&self, ctx: &OpContext) -> Result<(), AppError> {
        Ok(()) // No hay cleanup necesario; el job se detiene.
    }
}
```

### 4.3 Integración con start/stop

```rust
// application/lifecycle/start.rs
pub async fn start_site(ctx: Arc<AppContext>, input: StartSiteInput) -> Result<Operation, AppError> {
    // ... plan normal

    // Step final: arrancar auto-dump.
    plan_builder.step(
        Step::Custom {
            label: "auto-dump".into(),
            action: Arc::new(WatchDatabaseChanges {
                site: site.clone(),
                interval: Duration::from_secs(20),
                idle_threshold: Duration::from_secs(60),
                dump_dest: site.sql_dir(),
                keep: 3,
            }),
        }
    );
    // ...
}
```

`stop_site` lo cancela explícitamente:

```rust
plan_builder.step(Step::CancelOperation { op_id: autodump_op_id });
```

### 4.4 Métricas

El auto-dump expone:
- `panel.autodump.bytes{ site_id }` (counter).
- `panel.autodump.dumps{ site_id, source }` (counter).
- `panel.autodump.duration_seconds` (histogram).

### 4.5 Idempotencia y limpieza

Si el panel se apaga con un auto-dump vivo, `startup_recovery` lo
detecta (journal sin `done`) y lo cancela. No se duplica porque
`WatchDatabaseChanges` se basa en `db_container_name` + `site_id`,
únicos.

---

## 5. `op-log` tipado

### 5.1 Contrato

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpEvent {
    Plan { steps: Vec<String> },
    Step { idx: usize, total: usize, label: String, status: StepStatus },
    Progress { idx: usize, ratio: f32, units: Option<String> },
    Line { text: String },
    Done { status: OpStatus, result: Option<Value> },
    Failed { error: AppErrorDto, compensation: Vec<String> },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Started,
    Ok,
    Failed { retryable: bool },
}
```

### 5.2 Soporte legacy

El `OpConsole.svelte` actual parsea strings. El rebuild lo actualiza
para consumir el `OpEvent` tipado. Mientras conviven, el backend
también emite un `Line` con el texto formateado para mantener
compatibilidad con scripts que parsean el log.

### 5.3 Cancelación UI

```svelte
<!-- /lib/components/OpConsole.svelte -->
<script lang="ts">
  import type { Operation, OpEvent } from '$lib/contracts/operations';
  let { operationId, onCancel } = $props();
  let events = $state<OpEvent[]>([]);
  let stepStatus = $state<StepStatus[]>([]);

  onMount(() => {
    listen<{opId: string, evt: OpEvent}>(Events.OpLog, (e) => {
      if (e.payload.opId !== operationId) return;
      // ...
      if (e.payload.evt.type === 'step') {
        stepStatus[e.payload.evt.idx] = e.payload.evt.status;
      }
    });
  });

  async function cancel() {
    await cancelOperation({ operationId });
    onCancel?.();
  }
</script>
```

El botón «Cancelar» se deshabilita cuando:
- El step actual no es `Started`.
- El sitio no está en `Pending`/`Running`.

---

## 6. Reconciliador

### 6.1 Diseño

El reconciliador es un caso de uso independiente:

```rust
pub async fn reconcile(
    ctx: Arc<AppContext>,
    cfg: ReconcileConfig,
) -> Result<ReconcileReport, AppError> {
    let desired = load_desired(&ctx).await?;
    let actual = load_actual(&ctx).await?;
    let drifts = detect_drifts(&desired, &actual);

    let mut report = ReconcileReport::default();
    for drift in drifts {
        report.drifts.push(drift.clone());
        if cfg.autofix && should_autofix(&drift.kind, &cfg) {
            match apply_fix(&ctx, &drift).await {
                Ok(()) => report.fixed.push(drift),
                Err(err) => {
                    report.refused.push(drift.clone());
                    report.errors.push(err);
                }
            }
        }
    }
    Ok(report)
}
```

### 6.2 `load_desired`

```rust
async fn load_desired(ctx: &AppContext) -> Result<DesiredState, AppError> {
    let sites = ctx.config.list_sites().await?;
    let groups = ctx.config.list_groups().await?;
    let endpoint = ctx.config.load_endpoint().await?;
    let panel = ctx.config.load_panel_config().await?;
    Ok(DesiredState { sites, groups, endpoint, panel })
}
```

### 6.3 `load_actual`

```rust
async fn load_actual(ctx: &AppContext) -> Result<ActualState, AppError> {
    let containers = ctx.docker.list_all().await?;
    let ports = ctx.platform.port_checker().scan_all().await?;
    let dns = ctx.platform.dns_resolver().scan_wildcard("test").await?;
    let folders = ctx.platform.filesystem().read_dir(&ctx.config.projects_root()?).await?;
    let nginx_conf = ctx.platform.filesystem().read_dir(&nginx_conf_d_dir()?).await?;
    Ok(ActualState { containers, ports, dns, folders, nginx_conf })
}
```

### 6.4 `detect_drifts`

```rust
fn detect_drifts(desired: &DesiredState, actual: &ActualState) -> Vec<Drift> {
    let mut drifts = Vec::new();

    // Orphan containers
    for c in &actual.containers {
        if !c.name.starts_with("wp-") { continue; }
        let id = &c.name[3..];
        if !desired.sites.iter().any(|s| s.id.to_string() == id) {
            drifts.push(Drift::new(
                DriftKind::OrphanContainer { cname: c.name.clone(), image: c.image.clone() },
                if c.image.contains("panel-php") { DriftSeverity::Warn } else { DriftSeverity::Info },
                Fix::RemoveContainer { name: c.name.clone(), force: false },
            ));
        }
    }

    // Orphan vhosts
    for vhost in &actual.nginx_conf {
        let id = vhost.file_stem().unwrap_or_default();
        if id.starts_with("00-") { continue; } // tuning
        if !desired.sites.iter().any(|s| s.id.to_string() == id) {
            drifts.push(Drift::new(
                DriftKind::OrphanVhost { path: vhost.path.clone() },
                DriftSeverity::Warn,
                Fix::RemoveVhost { path: vhost.path.clone() },
            ));
        }
    }

    // Shared running but no active
    let active = desired.sites.iter().filter(|s| actual.is_running(&s.container_name())).count();
    if active == 0 && actual.is_running("panel-nginx") {
        drifts.push(Drift::new(
            DriftKind::SharedRunningButNoActive { container: "panel-nginx".into() },
            DriftSeverity::Warn,
            Fix::StopContainer { name: "panel-nginx".into() },
        ));
    }

    // Drainage del dump-log
    for entry in &actual.dump_log_entries {
        if !entry.file.exists() {
            drifts.push(Drift::new(
                DriftKind::StaleDumpLog { entry: entry.clone() },
                DriftSeverity::Info,
                Fix::PruneDumpLog { entry: entry.clone() },
            ));
        }
    }

    // Endpoint conflict
    if let Some(ep) = &desired.endpoint {
        if !port_is_free(ep.http_port, &actual.ports) {
            drifts.push(Drift::new(
                DriftKind::EndpointConflict { port: ep.http_port, holder: None },
                DriftSeverity::Error,
                Fix::ReconfigureEndpoint,
            ));
        }
    }

    // DNS
    if !actual.dns.wildcard_active {
        drifts.push(Drift::new(
            DriftKind::DnsNotResolving { domain: "*.test".into() },
            DriftSeverity::Error,
            Fix::ReseedDnsmasq,
        ));
    }

    drifts
}
```

### 6.5 Categorías de autofix

| Severidad | Acción |
|---|---|
| `Info` | Auto (sin pedir). |
| `Warn` | Auto si `autofix=true`. |
| `Error` | Auto si `autofix=true` y el fix es seguro (`ReconfigureEndpoint`, `ReseedDnsmasq`). |
| `Critical` | Nunca auto. Requiere UI. |

### 6.6 UI

```svelte
<!-- /routes/settings/+page.svelte -->
<Card title="Estado del sistema">
  <SystemStatusView {status} />
  {#if status.drifts.length > 0}
    <DriftList drifts={status.drifts} onApply={applyFix} />
  {/if}
</Card>
```

`DriftList` muestra cada drift con su severidad, descripción, y un
botón «Aplicar fix» (o «Ignorar»).

---

## 7. Platform ports

### 7.1 `linux.rs`

```rust
// platform/linux.rs
pub struct LinuxPlatform {
    fs: Arc<dyn FileSystem>,
    proc: Arc<dyn ProcessRunner>,
    shell: Arc<dyn Shell>,
    host: Arc<dyn Host>,
    port_checker: LinuxPortChecker,
    dns_resolver: LinuxDnsResolver,
    cert_authority: MkcertAuthority,
}

impl Platform for LinuxPlatform {
    fn port_checker(&self) -> &dyn PortChecker { &self.port_checker }
    fn dns_resolver(&self) -> &dyn DnsResolver { &self.dns_resolver }
    fn cert_authority(&self) -> &dyn CertAuthority { &self.cert_authority }
    fn can_install_system_service(&self) -> bool {
        which("pkexec").is_some() && which("systemctl").is_some()
    }
}

impl PortChecker for LinuxPortChecker {
    async fn scan_all(&self) -> Result<PortMap, AppError> {
        let mut map = PortMap::default();
        let v4 = std::fs::read_to_string("/proc/net/tcp").unwrap_or_default();
        let v6 = std::fs::read_to_string("/proc/net/tcp6").unwrap_or_default();
        for line in v4.lines().chain(v6.lines()).skip(1) {
            if let Some((port, addr)) = parse_listen(line) {
                map.insert(port, classify(addr));
            }
        }
        Ok(map)
    }
}
```

### 7.2 `macos.rs`

```rust
pub struct MacOsPlatform { /* ... */ }

impl Platform for MacOsPlatform {
    fn port_checker(&self) -> &dyn PortChecker { &self.port_checker }
    fn dns_resolver(&self) -> &dyn DnsResolver { &self.dns_resolver }
    fn cert_authority(&self) -> &dyn CertAuthority { &self.cert_authority }
    fn can_install_system_service(&self) -> bool { false }
}

impl PortChecker for MacOsPortChecker {
    async fn scan_all(&self) -> Result<PortMap, AppError> {
        // `lsof -nP -iTCP -sTCP:LISTEN` parseado.
        let out = self.proc.run(ProcessCommand {
            program: "lsof".into(),
            args: vec!["-nP".into(), "-iTCP".into(), "-sTCP:LISTEN".into()],
            cwd: None,
            env: vec![],
        }).await?;
        parse_lsof_output(&out.stdout)
    }
}
```

### 7.3 `windows.rs`

```rust
pub struct WindowsPlatform { /* ... */ }

impl Platform for WindowsPlatform {
    fn port_checker(&self) -> &dyn PortChecker { &self.unimplemented_port_checker }
    fn dns_resolver(&self) -> &dyn DnsResolver { &self.unimplemented_dns_resolver }
    fn cert_authority(&self) -> &dyn CertAuthority { &self.unimplemented_cert_authority }
    fn can_install_system_service(&self) -> bool { false }
}

impl PortChecker for UnimplementedPortChecker {
    async fn scan_all(&self) -> Result<PortMap, AppError> {
        Err(AppError::Unsupported { what: "port checker on Windows".into() })
    }
}
```

`can_install_system_service: false` hace que `first-run` salte la
parte de `pkexec` y muestre un mensaje claro.

---

## 8. Eventos backend → frontend

### 8.1 Canales

| Canal | Payload | Audiencia |
|---|---|---|
| `op-log` | `{ opId, evt: OpEvent }` | OpConsole global |
| `log:{site_id}` | string | LogStream tab del sitio |
| `sites-changed` | `void` | Lista de proyectos |
| `drift-detected` | `{ drifts: Drift[] }` | Settings |

### 8.2 Capabilities

La capability `core:event:default` ya está en
`src-tauri/capabilities/default.json`. El rebuild añade, además:

- `op-log`: no requiere permisos (los comandos propios están fuera
  del ACL; pero `listen` requiere `core:event`).
- `log:{site_id}`: idem.
- `sites-changed`: idem.
- `drift-detected`: idem.

### 8.3 Auxiliar de TS

```ts
// src/lib/events.ts
import { listen, emit } from '@tauri-apps/api/event';

export const Events = {
  OpLog: 'op-log',
  LogStream: (id: string) => `log:${id}`,
  SitesChanged: 'sites-changed',
  DriftDetected: 'drift-detected',
} as const;

export const onOpLog = (cb: (opId: string, evt: OpEvent) => void) =>
  listen<{ opId: string, evt: OpEvent }>(Events.OpLog, (e) => cb(e.payload.opId, e.payload.evt));
```

---

## 9. Feature flags en `SiteConfig`

### 9.1 Flags actuales

| Flag | Significado |
|---|---|
| `flags.oneClickAdmin` | ¿El sitio soporta auto-login? |
| `flags.xdebugEnabled` | ¿Xdebug instalado? |
| `flags.headless` | ¿El sitio es headless (sin WP visible)? |
| `flags.frontendFramework` | Si headless, el framework. |
| `flags.migrationPending` | ¿La importación está pendiente? |
| `shared.minio` | ¿El sitio usa MinIO? |
| `shared.mailpit` | ¿El sitio enruta correo a Mailpit? |

### 9.2 Cómo se traducen

- `xdebugEnabled: true` → Step `EnableXdebug` en `create_site` (escribe
  `zz-xdebug.ini` y luego `StartContainer`).
- `shared.minio: true` → Step `EnsureMinio` en `start_site`.
- `flags.migrationPending: true` → UI bloquea «Start» y propone
  «Migrate». La cabecera muestra el aviso.

### 9.3 Nuevas flags (futuro)

- `flags.acme: bool` (certificado público vía ACME / Let's Encrypt).
- `flags.freezeSnapshots: bool` (no permitir nuevos snapshots).
- `flags.requiresOtp: bool` (auto-login con OTP obligatorio).

---

## 10. Errores irrecuperables

### 10.1 Clasificación

| Error | Severidad | Comportamiento |
|---|---|---|
| `AppError::Docker(BollardError::NotFound)` | `Error` | Drift → `OrphanContainer`. |
| `AppError::Docker(BollardError::Conflict)` | `Error` | Step retry con backoff. |
| `AppError::Docker(BollardError::Io)` | `Critical` | Stop op, journal, raise. |
| `AppError::Database("dump timeout")` | `Error` | Step retry con `ResetDatabase`. |
| `AppError::Schema` | `Critical` | Stop op, raise. Sugerir `import_disconnected_site` o `migrate`. |
| `AppError::Cancelled` | `Info` | Stop op, compensa. |

### 10.2 Reintentos

El coordinator soporta `Step::Retry { max: u32, backoff: Backoff }`:

```rust
pub enum Backoff {
    Fixed(Duration),
    Exponential { initial: Duration, factor: f32, max: Duration },
}
```

Por defecto:
- Network: exponential 3 intentos.
- Docker transient: exponential 2 intentos.
- DB dump: exponential 5 intentos.

### 10.3 Raise → `AppError::Internal`

Si un step no debería fallar, se loguea como `Critical` y se propaga
como `AppError::Internal(cause)`. El usuario ve «Internal error» en
la UI y un botón «Ver detalles» que abre el modal del journal.

---

## 11. Plan canónico: `start_site`

```rust
pub fn start_site_plan(site: &Site) -> Plan {
    let cname = site.container_name();
    let image = ImageTag::php_panel(site.services.php.version.clone());

    Plan::builder()
        .step(Step::EnsureNetwork { name: "panel-net" })
        .step_with_compensation(
            Step::EnsureDb { db: site.services.db.clone() },
            Step::Noop, // no compensar: otros sitios pueden seguir usándola
        )
        .step_with_compensation(
            Step::EnsureImage { image: image.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::RecreateContainerIfImageMismatch { name: cname.clone(), image: image.clone() },
            Step::RemoveContainer { name: cname.clone(), force: false },
        )
        .step_with_compensation(
            Step::CreateContainer { spec: ContainerSpec::php(site, &image) },
            Step::RemoveContainer { name: cname.clone(), force: false },
        )
        .step_with_compensation(
            Step::StartContainer { name: cname.clone() },
            Step::StopContainer { name: cname.clone(), timeout: Duration::from_secs(10) },
        )
        .step_with_compensation(
            Step::WriteVhost { site_id: site.id, config: site.clone() },
            Step::RemoveVhost { site_id: site.id },
        )
        .step_with_compensation(
            Step::EnsureNginx,
            Step::Noop,
        )
        .step_with_compensation(
            Step::ReloadNginx,
            Step::Noop,
        )
        .step(Step::EnsureDnsmasqWildcard)
        .step(Step::Emit { event: "sites-changed".into(), payload: json!({}) })
        .step(Step::Custom {
            label: "auto-dump".into(),
            action: Arc::new(WatchDatabaseChanges::new(site.clone())),
        })
        .build()
}
```

### 11.1 Cancelación

`cancel(op_id)` en cualquier momento:
- `StopContainer` espera `timeout: 10s` antes de matar.
- `WriteVhost` y `RemoveVhost` no son cancelables (son < 100 ms).
- `WatchDatabaseChanges` chequea `cancel` en cada loop.

### 11.2 Compensación

Si `StartContainer` falla, la compensación ejecuta:
- `StopContainer { name: cname, timeout: 10s }` (noop si no arrancó).
- `RemoveContainer { name: cname, force: false }`.
- `RemoveVhost { site_id: site.id }`.

NGINX queda intacto (no se toca). El `ReloadNginx` se reintentará en
el próximo `start_site`.

---

## 12. Plan canónico: `migrate_site`

```rust
pub fn migrate_plan(site: &Site) -> Plan {
    let dump_path = latest_dump(&site.sql_dir());
    let db_container = db_container_name(&site.services.db);

    Plan::builder()
        .step(Step::SyncMuPlugins { site_path: site.path.clone(), flags: site.flags.clone() })
        .step_with_compensation(
            Step::EnsureDb { db: site.services.db.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::WaitDbReady { container: db_container.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::CreateSchema { container: db_container.clone(), schema: site.services.db.db_name.clone(), charset: "utf8mb4" },
            Step::DropSchema { container: db_container.clone(), schema: site.services.db.db_name.clone() },
        )
        .step(Step::StartSitePlan { site_id: site.id })  // sub-plan
        .step_with_compensation(
            Step::WriteWpConfig { site: site.path.clone(), db: site.services.db.clone(), db_container: db_container.clone() },
            Step::Noop, // wp-config regenerable
        )
        .step_with_compensation(
            Step::GenerateSsl { site: site.path.clone(), domain: site.domain.clone() },
            Step::Noop, // ya generado es válido
        )
        .step_with_compensation(
            Step::ImportDump {
                container: db_container.clone(),
                schema: site.services.db.db_name.clone(),
                dump: dump_path.clone(),
                opts: ImportOpts::default(),
            },
            Step::ResetDatabase { container: db_container.clone(), schema: site.services.db.db_name.clone() },
        )
        .step(Step::FixSiteUrl { site: site.path.clone(), url: Endpoint::site_url(&site.domain, site.services.nginx.ssl) })
        .step(Step::MarkMigrationDone { site_id: site.id })
        .step(Step::Emit { event: "sites-changed".into(), payload: json!({}) })
        .build()
}
```

### 12.1 Cancelación del import

Si el usuario cancela en medio de `ImportDump`:

1. `Step::ResetDatabase` corre (compensación).
2. El sitio queda `migrationPending: true` (no se revierte el flag).
3. La UI muestra «Migración cancelada, vuelve a intentarlo».

### 12.2 Watchdog del import

```rust
Step::ImportDump { container, schema, dump, opts: ImportOpts {
    preamble: IMPORT_PREAMBLE.to_vec(),
    epilogue: IMPORT_EPILOGUE.to_vec(),
    chunk_size: 1 << 20,
    tick: Duration::from_secs(2),
    idle_timeout: Duration::from_secs(180),
    progress_on: ProgressSink::new(...),
} }
```

El watchdog mide el tamaño real de la DB (no el stdin), como hoy
en `migrate.rs::import_dump`. Si no crece en `idle_timeout`, aborta
y compensa con `ResetDatabase`.

---

## 13. Plan canónico: `create_site`

```rust
pub fn create_site_plan(req: NewSiteRequest) -> Plan {
    let path = projects_root().join(req.slug.clone());
    let site = Site::from_request(req.clone(), path.clone());

    Plan::builder()
        .step_with_compensation(
            Step::CreateDir { path: path.clone() },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("app/public") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("app/sql") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("conf/php") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("logs/php") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("ssl") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::WritePhpIni { site: path.clone(), template: php_ini_template() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::AtomicWrite { path: path.join("config.json"), bytes: site.to_json_bytes() },
            Step::RemoveFile { path: path.join("config.json") },
        )
        .step_with_compensation(
            Step::EnsureDb { db: site.services.db.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::WaitDbReady { container: db_container_name(&site.services.db) },
            Step::Noop,
        )
        .step_with_compensation(
            Step::CreateSchema { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone(), charset: "utf8mb4" },
            Step::DropSchema { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone() },
        )
        .step_with_compensation(
            Step::DownloadCore { version: req.wp_version.clone(), dest: path.join("app/public/wp-core.tar.gz") },
            Step::RemoveAll { path: path.join("app/public") },
        )
        .step_with_compensation(
            Step::ExtractCore { tarball: path.join("app/public/wp-core.tar.gz"), dest: path.join("app/public") },
            Step::RemoveAll { path: path.join("app/public") },
        )
        .step_with_compensation(
            Step::SyncMuPlugins { site_path: path.clone(), flags: site.flags.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::GenerateSsl { site: path.clone(), domain: site.domain.clone() },
            Step::Noop,
        )
        .step(Step::StartSitePlan { site_id: site.id }) // sub-plan
        .step_with_compensation(
            Step::WriteWpConfig { site: path.clone(), db: site.services.db.clone(), db_container: db_container_name(&site.services.db) },
            Step::Noop,
        )
        .step(Step::WpCoreInstall { site: path.clone(), request: req.clone() })
        .step(Step::Emit { event: "sites-changed".into(), payload: json!({}) })
        .build()
}
```

### 13.1 Cancelación

Si el usuario cancela en cualquier punto:
- Las compensaciones de steps previos se ejecutan en reversa.
- La carpeta completa se borra (`Step::RemoveAll`).
- El `config.json` no se persiste (compensación borra).
- El usuario ve «Creación cancelada, no quedó rastro».

### 13.2 `StartSitePlan` sub-plan

`Step::StartSitePlan { site_id }` no es un step real; es una
**instrucción del coordinator** que encola el plan de `start_site`
como parte del actual. El journal mantiene una sola `op_id` para
todo el flujo.

---

## 14. Plan canónico: `create_worktree`

```rust
pub fn create_worktree_plan(parent: &Site, target: &str, branch: &str, shared_db: bool) -> Plan {
    let site = derive_worktree_site(parent, target, branch, shared_db);
    let path = site.path.clone();
    let repo_dir = parent.public_dir().join(target);
    let target_name = path_basename(target);
    let wt_dest = site.worktree_root().join(&target_name);

    let mut builder = Plan::builder();

    builder = builder
        .step_with_compensation(
            Step::CreateDir { path: path.clone() },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::CreateDir { path: path.join("wt") },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::AtomicWrite { path: path.join("wp-config.php"), bytes: b"<?php\n".to_vec() },
            Step::RemoveFile { path: path.join("wp-config.php") },
        )
        .step_with_compensation(
            Step::AtomicWrite { path: path.join("config.json"), bytes: site.to_json_bytes() },
            Step::RemoveAll { path: path.clone() },
        )
        .step_with_compensation(
            Step::GitWorktreeAdd { repo: repo_dir.clone(), dest: wt_dest.clone(), branch: branch.to_string(), base: None },
            Step::GitWorktreeRemove { repo: repo_dir.clone(), dest: wt_dest.clone() },
        );

    if !shared_db {
        builder = builder
            .step_with_compensation(
                Step::CreateSchema { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone(), charset: "utf8mb4" },
                Step::DropSchema { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone() },
            )
            .step_with_compensation(
                Step::StreamDump { container: db_container_name(&parent.services.db), schema: parent.services.db.db_name.clone(), dest: site.sql_dir().join("from-parent.sql") },
                Step::RemoveFile { path: site.sql_dir().join("from-parent.sql") },
            )
            .step_with_compensation(
                Step::ImportDump { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone(), dump: site.sql_dir().join("from-parent.sql"), opts: ImportOpts::default() },
                Step::ResetDatabase { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone() },
            );
    }

    builder
        .step_with_compensation(
            Step::GenerateSsl { site: path.clone(), domain: site.domain.clone() },
            Step::Noop,
        )
        .step(Step::StartSitePlan { site_id: site.id })
        .step_with_compensation(
            Step::WriteWpConfig { site: path.clone(), db: site.services.db.clone(), db_container: db_container_name(&site.services.db) },
            Step::Noop,
        )
        .step(Step::FixSiteUrl { site: path.clone(), url: Endpoint::site_url(&site.domain, site.services.nginx.ssl) })
        .step(Step::Emit { event: "sites-changed".into(), payload: json!({}) })
        .build()
}
```

---

## 15. Plan canónico: `create_snapshot`

```rust
pub fn create_snapshot_plan(site: &Site, label: &str) -> Plan {
    let snap_id = SnapshotId::new();
    let snap_dir = site.snapshots_root().join(snap_id.to_string());

    Plan::builder()
        .step_with_compensation(
            Step::CreateDir { path: snap_dir.clone() },
            Step::RemoveAll { path: snap_dir.clone() },
        )
        .step_with_compensation(
            Step::EnsureDb { db: site.services.db.clone() },
            Step::Noop,
        )
        .step_with_compensation(
            Step::StreamDump { container: db_container_name(&site.services.db), schema: site.services.db.db_name.clone(), dest: snap_dir.join("db.sql") },
            Step::RemoveFile { path: snap_dir.join("db.sql") },
        )
        .step_with_compensation(
            Step::SnapshotCode { site: site.path.clone(), dest: snap_dir.join("code.tar.zst"), excludes: site.snapshot_excludes.clone() },
            Step::RemoveFile { path: snap_dir.join("code.tar.zst") },
        )
        .step_with_compensation(
            Step::WriteSnapshotMeta { snapshot_id: snap_id, dest: snap_dir.join("meta.json"), meta: SnapshotMeta::new(snap_id, label, &site) },
            Step::RemoveFile { path: snap_dir.join("meta.json") },
        )
        .step(Step::Emit { event: "snapshots-changed".into(), payload: json!({ "site_id": site.id }) })
        .build()
}
```

`SnapshotCode` se implementa con `tar --zstd --exclude` (como hoy).

### 15.1 `SnapshotCode` step

```rust
Step::SnapshotCode { site, dest, excludes } -> {
    let mut args = vec![
        "--zstd".into(), "-cf".into(), dest.to_string_lossy().into(),
        "--exclude=./wp-content/uploads".into(),
        "--exclude=./wp-content/cache".into(),
        "--exclude=./wp-config.php".into(),
        "--exclude=./*.log".into(),
    ];
    for ex in excludes {
        args.push(format!("--exclude=./{}", ex.to_string()));
    }
    args.push("-C".into());
    args.push(site.public_dir().to_string_lossy().into());
    args.push(".".into());
    run_process("tar", args, /* capture stderr */).await?;
    Ok(())
}
```

Exit 1 de `tar` sigue siendo aceptable (avisos no fatales).

---

## 16. CI de Docker: arranque sano

### 16.1 Escenario 1: cold start

```
1. Docker no está corriendo.
2. user corre `pnpm tauri dev`.
3. Tauri setup → AppContext.
4. Application::startup_recovery(ctx) → Docker no accesible.
5. Drifts: docker_ok: false, network_ok: false.
6. UI muestra "Install Docker" con CTA.
```

### 16.2 Escenario 2: warm start

```
1. Docker corre. panel-nginx fue apagado.
2. config.json tiene `last_started_at: 2026-07-23T11:00:00Z`.
3. Application::startup_recovery(ctx) → no drifts.
4. UI carga la lista. Sitios en `Stopped`.
```

### 16.3 Escenario 3: post-apagón sucio

```
1. WSL/kill -9.
2. Al volver, container `wp-abc` está "running" con namespaces zombies.
3. Application::startup_recovery(ctx) → drifts:
   - OrphanContainer? no, existe config.
   - DnsNotResolving? sí (NetworkManager no se restauró).
4. User: System → "Reconfigure DNS" → Fix::ReseedDnsmasq.
5. User enciende el sitio → ReloadNginx (recreate si falla el reload).
```

---

## 17. Mejoras específicas vs el código actual

### 17.1 `fix_site_url` ya no es especial

`migrate.rs::fix_site_url` se llama tras el `ImportDump`. En el
rebuild, es un step más (`FixSiteUrl`). El compensador no lo deshace
porque la URL es correcta (y si falla, ningún step atrás falló: el
sitio arranca con la URL vieja, lo cual es mejor que un sitio a
medias).

### 17.2 `WpCli` con timeout declarativo

`wpcli::run` tiene `WPCLI_TIMEOUT = 120s`. El rebuild lo modela en
el step:

```rust
Step::WpCli { site, args, user: Some("www-data"), timeout: Duration::from_secs(120) }
```

Y el compensador **no** deshace un WP-CLI exitoso (no tiene
sentido). El timeout se cuenta desde el step wrapper.

### 17.3 `BackupFile` antes de `AtomicWrite`

Toda mutación de `config.json` (en cualquier step) va precedida de
`Step::BackupFile { path: config.json, suffix: ".bak.{timestamp}" }`.
El compensador borra `.bak.{timestamp}` (no deja basura al éxito);
si el `AtomicWrite` falla, la rotación de `.bak.*` mantiene los
últimos 5.

### 17.4 `Step::Retry { max, backoff }` opcional

Steps como `DownloadCore`, `ImportDump`, `EnsureImage` aceptan retry.
El developer lo declara en el plan, no hardcoded en el adapter.

---

## 18. Próximo paso

El capítulo 05 (Seguridad, observabilidad y recuperación) detalla el
manejo de secretos, el path/build validation, los streaming backups,
la observabilidad estructurada, los platform ports y la estrategia
de startup_recovery.
