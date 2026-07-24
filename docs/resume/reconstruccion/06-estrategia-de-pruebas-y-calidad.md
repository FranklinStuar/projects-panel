# 06 · Estrategia de pruebas y calidad

> Documento 6 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este capítulo define la pirámide de tests, los mocks,
> los tests de propiedades, los goldens, los integration tests, los
> escenarios E2E, los criterios de cobertura y la estrategia de CI.

---

## 1. Pirámide de tests

```
                ┌──────────────────┐
                │      E2E         │  Playwright + dev:mock (manual)
                │     (1-2%)       │
                ├──────────────────┤
                │   Integration    │  Docker + tempdir (manual `--ignored`)
                │     (5-10%)      │
                ├──────────────────┤
                │   Component      │  unit por módulo (CI)
                │    (30-40%)      │
                ├──────────────────┤
                │     Unit         │  dominio + adapters (CI)
                │    (50-60%)      │
                └──────────────────┘
```

### 1.1 Por qué esta pirámide

- **Unit** cubre la lógica de negocio pura (Domain, Application,
  Policies). Sin Docker, sin red. Corre en `cargo test` en < 60 s.
- **Component** cubre cada módulo con su adapter mockeado. Detectan
  errores de integración temprano.
- **Integration** requiere Docker. Marcados `#[ignore]`. Se corren
  con `cargo test -- --ignored --test-threads=1`.
- **E2E** cubre la GUI con Playwright + dev:mock. Marcados `visual`
  (no bloquean CI).

### 1.2 Velocidad objetivo

| Test | Duración | Modo |
|---|---:|---|
| `cargo test` (unit) | ≤ 60 s | CI |
| `cargo test -- --ignored` (integration) | ≤ 10 min | CI nightly |
| `pnpm test:e2e` (Playwright) | ≤ 2 min | PR review |
| `cargo test --release` | ≤ 5 min | CI |

---

## 2. Tests unitarios

### 2.1 Domain puro

#### 2.1.1 `domain::value::slug`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_accepts_valid() {
        assert_eq!(Slug::parse("mi-sitio").unwrap().as_str(), "mi-sitio");
    }

    #[test]
    fn slug_rejects_uppercase() {
        assert!(matches!(Slug::parse("Mi-Sitio"), Err(AppError::Validation { .. })));
    }

    #[test]
    fn slug_rejects_leading_dash() {
        assert!(Slug::parse("-mi-sitio").is_err());
    }

    #[test]
    fn slug_rejects_too_long() {
        let s = "a".repeat(65);
        assert!(Slug::parse(&s).is_err());
    }
}
```

#### 2.1.2 `domain::entity::site`

```rust
#[test]
fn site_with_group_keeps_others() {
    let site = Site::sample();
    let updated = site.with_group(Some(GroupName::new("Clientes").unwrap()));
    assert_eq!(updated.group.as_ref().unwrap().as_str(), "Clientes");
    assert_eq!(updated.name, site.name); // other fields unchanged
}
```

#### 2.1.3 `domain::policy::teardown`

```rust
#[test]
fn teardown_stops_nginx_when_no_active() {
    let site = Site::sample();
    let decision = decide_teardown(&site, &[], &actual_running_none());
    assert!(decision.stop_nginx);
    assert!(decision.stop_mailpit);
    assert!(decision.stop_adminer);
    assert!(decision.stop_db.is_empty()); // db not touched if no use
}

#[test]
fn teardown_keeps_nginx_when_active() {
    let site = Site::sample();
    let active = vec![Site::sample_other()];
    let decision = decide_teardown(&site, &active, &actual_running_some());
    assert!(!decision.stop_nginx);
}

#[test]
fn teardown_stops_other_db_when_unique() {
    let site = Site::sample_db("mysql", "8.0");
    let active = vec![Site::sample_db("postgres", "16")];
    let decision = decide_teardown(&site, &active, &actual_running_short());
    assert!(decision.stop_db.contains(&db_container_name(&site.services.db)));
}

#[test]
fn teardown_keeps_shared_db_when_in_use() {
    let site_a = Site::sample_db("mysql", "8.0");
    let site_b = Site::sample_db("mysql", "8.0");
    let decision = decide_teardown(&site_a, &[site_b.clone()], &actual_running_both());
    assert!(decision.stop_db.is_empty());
}
```

#### 2.1.4 `domain::entity::operation`

```rust
#[test]
fn operation_starts_with_pending() {
    let op = Operation::new(OperationKind::StartSite { site_id: SiteId::new() }, 5);
    assert_eq!(op.status, OpStatus::Pending);
    assert_eq!(op.plan.steps.len(), 5);
}
```

#### 2.1.5 `domain::entity::drift`

```rust
#[test]
fn drift_severity_orders() {
    let info = Drift::new(DriftKind::StaleDumpLog, DriftSeverity::Info, Fix::Ignore);
    let warn = Drift::new(DriftKind::OrphanContainer { cname: "wp-x".into(), image: "img".into() }, DriftSeverity::Warn, Fix::RemoveContainer { name: "wp-x".into(), force: false });
    let err = Drift::new(DriftKind::EndpointConflict { port: 80, holder: None }, DriftSeverity::Error, Fix::ReconfigureEndpoint);
    assert!(info.severity < warn.severity);
    assert!(warn.severity < err.severity);
}
```

### 2.2 Application

#### 2.2.1 `application::lifecycle::start`

```rust
#[tokio::test]
async fn start_site_plan_has_required_steps() {
    let site = Site::sample();
    let plan = start_site_plan(&site);
    let labels: Vec<_> = plan.steps.iter().map(label_of).collect();
    assert!(labels.iter().any(|s| s.contains("EnsureNetwork")));
    assert!(labels.iter().any(|s| s.contains("EnsureDb")));
    assert!(labels.iter().any(|s| s.contains("StartContainer")));
    assert!(labels.iter().any(|s| s.contains("WriteVhost")));
    assert!(labels.iter().any(|s| s.contains("ReloadNginx")));
}

#[tokio::test]
async fn start_site_compensates_on_image_pull_failure() {
    let ctx = build_test_context_with_failing_image_pull().await;
    let op = start_site(ctx.clone(), StartSiteInput { site_id: Site::sample().id }).await.unwrap_err();
    assert!(matches!(op, AppError::Docker(_)));
    // Compensations ran.
    let sites = ctx.config.list_sites().await.unwrap();
    assert!(!ctx.docker.is_running(&sites[0].container_name()).await);
}
```

#### 2.2.2 `application::migrate::run`

```rust
#[tokio::test]
async fn migrate_dumps_when_no_dump_present() {
    let ctx = build_test_context().await;
    let site = Site::sample_pending();
    let op = migrate(ctx.clone(), MigrateSiteInput { site_id: site.id }).await.unwrap();
    assert_eq!(op.status, OpStatus::Succeeded);
    let after = ctx.config.find_site(&site.id).await.unwrap().unwrap();
    assert!(!after.flags.migration_pending);
}
```

#### 2.2.3 `application::reconcile::run`

```rust
#[tokio::test]
async fn reconcile_detects_orphan_container() {
    let ctx = build_test_context().await;
    // Inject a synthetic container via the mock.
    ctx.docker.as_mock().add_running_container("wp-orphan", "panel-php:8.3-r3");
    let report = reconcile(ctx.clone(), ReconcileConfig { autofix: false, include_info: true }).await.unwrap();
    assert!(report.drifts.iter().any(|d| matches!(d.kind, DriftKind::OrphanContainer { .. })));
}

#[tokio::test]
async fn reconcile_autofix_removes_orphan_vhost() {
    let ctx = build_test_context().await;
    let vhost = ctx.config.config_dir().await.unwrap().join("nginx/conf.d/ghost.conf");
    tokio::fs::write(&vhost, "...").await.unwrap();
    let report = reconcile(ctx.clone(), ReconcileConfig { autofix: true, include_info: false }).await.unwrap();
    assert!(!vhost.exists());
    assert!(report.fixed.iter().any(|d| matches!(d.kind, DriftKind::OrphanVhost { .. })));
}
```

### 2.3 Adapters

#### 2.3.1 `adapters::host::fs`

```rust
#[test]
fn atomic_write_creates_no_partial() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    atomic_write(&path, b"{\"a\":1}").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"a\":1}");
    let mut entries = std::fs::read_dir(tmp.path()).unwrap();
    entries.next(); // config.json
    assert!(entries.next().is_none()); // no .tmp leftover
}

#[test]
fn atomic_write_overwrites_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    atomic_write(&path, b"old").unwrap();
    atomic_write(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
}
```

#### 2.3.2 `adapters::host::netcheck`

```rust
#[test]
fn port_status_free_when_no_listener() {
    let status = parse_proc_net("/proc/net/tcp", 8080).unwrap();
    assert_eq!(status, PortStatus::Free);
}

#[test]
fn port_status_wildcard_with_0_0_0_0() {
    // Synthetic input.
    let input = "sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
0: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1 1 1 10 -1";
    let status = parse_proc_net_content(input, 80).unwrap();
    assert_eq!(status, PortStatus::Wildcard);
}
```

### 2.4 Mocks

#### 2.4.1 `ports::mocks::docker`

```rust
pub struct MockDocker {
    pub running: Arc<Mutex<HashMap<String, MockContainer>>>,
    pub images: Arc<Mutex<HashSet<String>>>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub exec_results: Arc<Mutex<HashMap<Vec<String>, String>>>,
    pub failures: Arc<Mutex<HashMap<String, AppError>>>,
}

#[async_trait]
impl ContainerEngine for MockDocker {
    async fn is_running(&self, name: &str) -> bool {
        self.running.lock().await.contains_key(name)
    }
    async fn exec(&self, name: &str, cmd: ExecSpec) -> Result<ExecOutput, AppError> {
        if let Some(err) = self.failures.lock().await.get(name).cloned() {
            return Err(err);
        }
        let key = cmd.to_vec();
        let stdout = self.exec_results.lock().await.get(&key).cloned().unwrap_or_default();
        Ok(ExecOutput { stdout, stderr: String::new(), exit_code: 0 })
    }
    // ... resto
}
```

`MockDocker` se inyecta en `AppContext` para los tests.

#### 2.4.2 `ports::mocks::fs`

```rust
pub struct MockFileSystem {
    pub files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    pub locks: Arc<Mutex<HashMap<PathBuf, usize>>>,
}

impl FileSystem for MockFileSystem {
    fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<(), AppError> {
        self.files.lock().await.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn try_lock(&self, path: &Path, _timeout: Duration) -> Result<FileLock, AppError> {
        let mut guard = self.locks.lock().await;
        let count = guard.entry(path.to_path_buf()).or_insert(0);
        *count += 1;
        Ok(FileLock { _guard: MutexGuardWrapper { /* releases */ } })
    }
}
```

#### 2.4.3 `ports::mocks::observability`

```rust
pub struct MockObservability {
    pub events: Arc<Mutex<Vec<RecordedEvent>>>,
}

impl Observability for MockObservability {
    fn oplog(&self, op: OperationId, evt: OpEvent) -> Result<(), AppError> {
        self.events.lock().await.push(RecordedEvent { op, evt });
        Ok(())
    }
}
```

#### 2.4.4 `ports::mocks::process`

```rust
pub struct MockProcessRunner {
    pub responses: Arc<Mutex<HashMap<Vec<String>, ProcessOutput>>>,
    pub failures: Arc<Mutex<HashMap<Vec<String>, AppError>>>,
}

impl ProcessRunner for MockProcessRunner {
    async fn run(&self, c: &ProcessCommand) -> Result<ProcessOutput, AppError> {
        let key = std::iter::once(c.program.clone()).chain(c.args.clone()).collect();
        if let Some(err) = self.failures.lock().await.get(&key).cloned() { return Err(err); }
        Ok(self.responses.lock().await.get(&key).cloned().unwrap_or_default())
    }
}
```

---

## 3. Tests de propiedades

### 3.1 `domain::value::slug`

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn slug_roundtrip(s in "[a-z0-9-]{1,64}") {
        let slug = Slug::parse(&s).unwrap();
        prop_assert_eq!(slug.as_str(), s);
    }

    #[test]
    fn slug_rejects_invalid_chars(s in "[A-Z0-9_]{1,64}") {
        let result = Slug::parse(&s);
        prop_assert!(result.is_err());
    }
}
```

### 3.2 `config::schema::migrate`

```rust
proptest! {
    #[test]
    fn migrate_v1_to_v2_preserves_unique_id(id in "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}") {
        let v1 = json!({
            "id": id,
            "name": "Test",
            "path": "/tmp/test",
            "domain": "test.test",
            "createdAt": "2026-01-01T00:00:00Z",
            "services": {
                "php": "8.3",
                "db": { "type": "mysql", "version": "8.0", "dbName": "test_db" },
                "nginx": { "ssl": true }
            },
            "oneClickAdmin": true,
            "xdebugEnabled": false,
            "headless": false,
            "frontendFramework": null,
        });
        let v2 = migrate_to_current(v1).unwrap();
        prop_assert_eq!(v2.id.to_string(), id);
    }
}
```

### 3.3 `domain::policy::teardown`

```rust
proptest! {
    #[test]
    fn teardown_idempotent(site_count in 0usize..10) {
        let sites: Vec<Site> = (0..site_count).map(|i| Site::sample_n(i)).collect();
        let active = sites.clone();
        let decision1 = decide_teardown(&sites[0], &active, &actual_running_some());
        let decision2 = decide_teardown(&sites[0], &active, &actual_running_some());
        prop_assert_eq!(decision1.stop_nginx, decision2.stop_nginx);
    }
}
```

### 3.4 `domain::entity::operation`

```rust
proptest! {
    #[test]
    fn operation_status_machine(start in 0u32..3, action in 0u32..5) {
        let mut op = Operation::new(OperationKind::StartSite { site_id: SiteId::new() }, 5);
        let next = op.apply_event(action_to_event(action));
        // El estado debe ser monotónico: Pending → Running → Succeeded/Failed/Cancelled.
        prop_assert!(op.status <= next.status);
    }
}
```

---

## 4. Tests de contrato (golden)

### 4.1 Snapshot

```rust
// tests/contract/start_site.rs
use insta::*;

#[test]
fn start_site_request_valid() {
    let v = serde_json::to_value(StartSiteInput { site_id: SiteId::parse("00000000-0000-0000-0000-000000000001").unwrap() }).unwrap();
    assert_yaml_snapshot!(v);
}

#[test]
fn start_site_response_success() {
    let op = Operation::succeeded_sample();
    let v = serde_json::to_value(op).unwrap();
    assert_yaml_snapshot!(v);
}

#[test]
fn start_site_response_failure() {
    let op = Operation::failed_sample();
    let v = serde_json::to_value(op).unwrap();
    assert_yaml_snapshot!(v);
}
```

### 4.2 Catálogo de contratos

Cada comando IPC tiene un golden:
- `tests/contract/{command}/request.json`
- `tests/contract/{command}/response_success.json`
- `tests/contract/{command}/response_failure.json`
- `tests/contract/{command}/error_not_found.json`
- `tests/contract/{command}/error_validation.json`

```rust
#[test]
fn create_site_schema_unchanged() {
    let request_json = include_str!("contract/create_site/request.json");
    let v: serde_json::Value = serde_json::from_str(request_json).unwrap();
    let parsed: CreateSiteInput = serde_json::from_value(v).unwrap();
    let serialized = serde_json::to_value(&parsed).unwrap();
    let original: serde_json::Value = serde_json::from_str(request_json).unwrap();
    assert_eq!(serialized, original);
}
```

### 4.3 Sustituir `assert_eq!` por goldens

A medida que los schemas cambian, los goldens se invalidan
explícitamente con `cargo insta review`. CI rechaza cambios
involuntarios.

---

## 5. Tests de integración

### 5.1 `tests/integration_docker.rs`

```rust
#[tokio::test]
#[ignore = "requires docker"]
async fn start_site_creates_container() {
    let ctx = build_test_context_real().await.unwrap();
    let site = Site::sample();
    start_site(ctx.clone(), StartSiteInput { site_id: site.id }).await.unwrap();
    assert!(ctx.docker.is_running(&site.container_name()).await);
    let _ = ctx.docker.stop_container(&site.container_name()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn reload_nginx_recreates_when_zombie() {
    let ctx = build_test_context_real().await.unwrap();
    // Simulate: start, then kill docker daemon (we cannot, so mark with namespaces)
    // Better: ensure_nginx, then exec a known-bad command, then reload.
    ctx.docker.ensure_nginx().await.unwrap();
    let result = ctx.docker.exec("panel-nginx", vec!["/bin/sh", "-c", "exit 1"]).await;
    assert!(result.is_err());
    // Reload should recreate.
    ctx.docker.reload_nginx().await.unwrap();
    assert!(ctx.docker.is_running("panel-nginx").await);
}
```

### 5.2 `tests/integration_recovery.rs`

```rust
#[tokio::test]
#[ignore = "requires docker + fs"]
async fn recovery_migrates_v1_config() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = build_test_context_with_root(tmp.path()).await.unwrap();
    let cfg_path = tmp.path().join("site").join("config.json");
    tokio::fs::create_dir_all(cfg_path.parent().unwrap()).await.unwrap();
    tokio::fs::write(&cfg_path, include_str!("fixtures/config-v1.json")).await.unwrap();
    let report = startup_recovery(ctx.clone()).await.unwrap();
    assert!(report.configs_migrated.iter().any(|s| s.contains("config.json")));
    let after = tokio::fs::read_to_string(&cfg_path).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(parsed["schemaVersion"], 2);
}

#[tokio::test]
#[ignore = "requires docker + fs"]
async fn recovery_removes_orphan_vhost() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = build_test_context_with_root(tmp.path()).await.unwrap();
    let vhost_dir = ctx.config.config_dir().await.unwrap().join("nginx/conf.d");
    tokio::fs::create_dir_all(&vhost_dir).await.unwrap();
    let vhost = vhost_dir.join("ghost.conf");
    tokio::fs::write(&vhost, "server { listen 80; }").await.unwrap();
    startup_recovery(ctx.clone()).await.unwrap();
    assert!(!vhost.exists());
}
```

### 5.3 `tests/integration_migrate.rs`

```rust
#[tokio::test]
#[ignore = "requires docker + LocalWP dump"]
async fn migrate_real_localwp_dump() {
    let dump_path = std::env::var("PANEL_TEST_LOCALWP_DUMP").unwrap_or_else(|_| {
        eprintln!("SKIP: set PANEL_TEST_LOCALWP_DUMP");
        return String::new();
    });
    if dump_path.is_empty() { return; }
    let ctx = build_test_context_real().await.unwrap();
    let dump = PathBuf::from(dump_path);
    let site = Site::sample_pending();
    let op = migrate(ctx.clone(), MigrateSiteInput { site_id: site.id }).await.unwrap();
    assert_eq!(op.status, OpStatus::Succeeded);
}
```

### 5.4 `tests/integration_import.rs`

```rust
#[tokio::test]
#[ignore = "requires real LocalWP install"]
async fn import_localwp_roundtrip() {
    let localwp_id = std::env::var("PANEL_TEST_LOCALWP_ID").unwrap_or_default();
    if localwp_id.is_empty() { return; }
    let ctx = build_test_context_real().await.unwrap();
    let result = localwp::import(&ctx, &localwp_id).await.unwrap();
    assert!(result.site.migration_pending);
    migrate(ctx.clone(), MigrateSiteInput { site_id: result.site.id }).await.unwrap();
}
```

### 5.5 Aislamiento

Todos los integration tests:

- Usan `--test-threads=1` obligatorio.
- Trabajan en `tempfile::tempdir()`.
- NUNCA tocan `~/panel-wp/` ni `~/.config/wordpress-panel/`.
- Crean containers con `IMAGE_REV` propia (`panel-php:8.3-r3-test`).
- Marcan `PANEL_TEST_*` para los que requieren estado externo.

---

## 6. Tests E2E (Playwright)

### 6.1 Stack

- `pnpm dev:mock` → arranca Vite con `lib/dev/mock-ipc.ts` que
  responde a `invoke` con snapshots.
- `pnpm test:e2e` → Playwright arranca `dev:mock` y corre specs.

### 6.2 Mocks actualizados

```ts
// src/lib/dev/mock-ipc.ts
export const mockIpc = {
  start_site: async (args: { id: string }) => {
    await sleep(800);
    const site = mockSites.find(s => s.id === args.id);
    if (!site) throw { code: 'not_found', message: 'site not found' };
    site.state = 'running';
    return { ok: true, opId: crypto.randomUUID() };
  },
  // ...
};
```

El mock debe ser **lo más cercano** al backend real para que los
tests E2E sean significativos.

### 6.3 Escenarios

```ts
// e2e/01-dashboard.spec.ts
test('master-detail shows selected site', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Proyectos' })).toBeVisible();
  await page.getByRole('listitem').first().click();
  await expect(page.getByTestId('project-detail')).toBeVisible();
});

test('start site emits progress', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Encender' }).click();
  await expect(page.getByRole('dialog', { name: 'Operación' })).toBeVisible();
  await expect(page.getByText('Plan')).toBeVisible();
  await expect(page.getByText('Succeeded')).toBeVisible({ timeout: 10_000 });
});

test('reconcile shows drifts', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Reconciliar' }).click();
  await expect(page.getByTestId('drift-list')).toBeVisible();
});

test('import localwp workflow', async ({ page }) => {
  await page.goto('/import-localwp');
  await page.getByRole('listitem').first().click();
  await page.getByRole('button', { name: 'Importar' }).click();
  await expect(page.getByText('migrationPending')).toBeVisible();
});
```

### 6.4 Determinismo

- Las operaciones async en los mocks se completan en < 100 ms.
- Las fechas se mocke (`clock.ts`).
- Los ids de UUID son deterministas cuando se requiera.

### 6.5 Visual regression

`@playwright/test` con `screenshot: 'on'` en CI. Threshold del 5 %.
Los componentes críticos (master-detail, modales, OpConsole) tienen
baseline.

---

## 7. Cobertura

### 7.1 Objetivos

| Carpeta | Cobertura mínima |
|---|---:|
| `domain/` | 95 % |
| `application/` | 90 % |
| `ports/` | 80 % (traits 100 %, mocks 0 %) |
| `adapters/` | 70 % |
| `config/` | 90 % |

### 7.2 CI

```yaml
# .github/workflows/ci.yml
- name: Test
  run: cargo test --no-fail-fast

- name: Coverage
  run: cargo tarpaulin --out Xml --output-dir coverage

- name: Coverage gate
  run: |
    python scripts/check_coverage.py coverage/cobertura.xml \
      --domain-min 95 \
      --application-min 90 \
      --adapters-min 70
```

### 7.3 Criterio de exclusión

`#[cfg(not(tarpaulin_include))]` se usa para excluir:

- Constructores triviales.
- Mocks.
- Código generado (`schemars`).

---

## 8. Linting

### 8.1 `cargo clippy`

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Reglas activas:
- `#![warn(missing_docs)]` en `domain/`, `application/`.
- `#![warn(clippy::all, clippy::pedantic, clippy::nursery)]`.
- `#![warn(clippy::cognitive_complexity)]` con threshold 15.

### 8.2 `cargo fmt`

```bash
cargo fmt --check
```

Sin diferencias. `rustfmt.toml` con `max_width = 100`, `imports_granularity = "Crate"`.

### 8.3 `cargo deny`

```bash
cargo deny check
```

- `bans`: permite solo crates específicas.
- `licenses`: solo MIT, Apache-2.0, BSD-3.
- `sources`: solo crates.io.

### 8.4 `tsc --noEmit` strict

```json
{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true
  }
}
```

### 8.5 ESLint + Prettier

```json
{
  "rules": {
    "@typescript-eslint/no-floating-promises": "error",
    "@typescript-eslint/no-misused-promises": "error"
  }
}
```

---

## 9. CI/CD

### 9.1 Pipeline

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Lint      │    │  Build      │    │   Test      │
│  fmt + clippy│    │  cargo + pnpm│    │  unit + e2e │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
       └──────────────────┼──────────────────┘
                          ▼
                  ┌─────────────┐
                  │  Coverage   │
                  │  + gate     │
                  └──────┬──────┘
                         ▼
                  ┌─────────────┐
                  │   Merge     │
                  └─────────────┘
```

### 9.2 PR checks

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- `pnpm build`
- `pnpm test:e2e` (con Playwright)
- `cargo deny check`

### 9.3 Nightly

- `cargo test -- --ignored --test-threads=1` (integration).
- `pnpm test:integration` (Long-running E2E).

### 9.4 Release

- Tag `vX.Y.Z`.
- `cargo build --release`.
- Publica binario + tarball.
- Genera el manifest de `latest.json` para auto-update (futuro).

---

## 10. Métricas de calidad

### 10.1 Internas

- **LOC por archivo**: `lib.rs ≤ 200`, `docker.rs ≤ 600`, etc.
- **Complejidad ciclomática**: `cargo cyclomatic` avg < 10.
- **Acoplamiento**: `cargo modules` no debe tener ciclos.

### 10.2 Externas

- **Adopción**: número de proyectos creados (futuro).
- **Robustez**: MTBF entre fallos recuperables.
- **Velocidad**: `panel.op.duration_seconds` p95.

### 10.3 Code review

Cada PR requiere:

- 1 rev. de un contribuidor.
- 1 rev. de un agente IA (auto-review).
- Pasa la CI.

---

## 11. Observabilidad de tests

### 11.1 Flaky tests

```rust
#[tokio::test]
async fn reconcile_with_real_docker() {
    for _ in 0..3 {
        let result = reconcile(...).await;
        if result.is_ok() { return; }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    panic!("flaky after 3 retries");
}
```

### 11.2 Reporter

```rust
pub struct QualityReporter {
    pub passed: AtomicUsize,
    pub failed: AtomicUsize,
    pub flaky: AtomicUsize,
}
```

Reportes en `target/quality.html`.

### 11.3 Sincronización con CI

Si un test pasa en local y falla en CI, se marca como `flaky` y se
reporta en el siguiente PR.

---

## 12. Lista de verificación por PR

```
[ ] cargo fmt --check
[ ] cargo clippy --all-targets --all-features -- -D warnings
[ ] cargo test
[ ] cargo test -- --ignored --test-threads=1 (if docker available)
[ ] pnpm build
[ ] pnpm test:e2e
[ ] cargo tarpaulin --target-lines 90
[ ] docs/CHANGELOG.md actualizado
[ ] docs/ARCHITECTURE.md actualizado (si cambia arquitectura)
[ ] docs/EXTENDING.md actualizado (si cambia API)
[ ] Sin atribución a Claude/Anthropic en commits
```

---

## 13. Próximo paso

El capítulo 07 (Roadmap, backlog y migración) cierra la serie con
una ruta de migración desde el código actual, los entregables por
etapa, la estrategia de rollback, los criterios de aceptación
incrementales y el backlog explícito (IA, deploy VPS, plugin S3).
