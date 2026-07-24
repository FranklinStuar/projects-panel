# 07 · Roadmap, backlog y migración

> Documento 7 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este capítulo cierra la serie con un roadmap incremental,
> entregables por etapa, criterios de aceptación, estrategia de
> migración y rollback, y el backlog explícito (IA, deploy VPS,
> plugin S3).

---

## 1. Filosofía del rollout

### 1.1 Compatibilidad estricta con el código existente

El rebuild **no rompe** los `~/panel-wp/{slug}/config.json` actuales.
Un panel v2 (rebuild) abierto en una instalación v1:

- Lee los configs como `schemaVersion: 1` legacy.
- Los migra silenciosamente a `schemaVersion: 2` al primer write.
- Mantiene `~/panel-wp/`y `~/.config/wordpress-panel/` en los mismos
  lugares.

### 1.2 Política de error cero

Cada release cumple:

- `cargo test` exit 0.
- `cargo test -- --ignored --test-threads=1` exit 0.
- `pnpm test:e2e` exit 0.
- `cargo tarpaulin` por encima de los gates.

### 1.3 Rollback en cualquier commit

El rebuild usa **git worktrees** (ya aprendido en el código del
panel) para que el usuario pueda volver a la versión anterior con
`pnpm tauri dev --branch v1.x` o, en producción, manteniendo el
binario anterior como respaldo.

---

## 2. Roadmap incremental

### 2.1 Etapas

```
E0 (semana 0)     ─ pre-flight: refundar el árbol, definir contratos
E1 (semana 1-2)   ─ domain + schemas + ports
E2 (semana 3-4)   ─ application + operation coordinator
E3 (semana 5-6)   ─ adapters (bollard + host)
E4 (semana 7)     ─ tauri + dbus + cli + mcp
E5 (semana 8)     ─ reconcile + recovery
E6 (semana 9)     ─ migrar código de los módulos existentes
E7 (semana 10)    ─ hardening + pruebas E2E
E8 (semana 11)    ─ beta + métricas
E9 (semana 12)    ─ release 1.0
```

### 2.2 Compatibilidad con rama principal

Mientras E0–E6 se ejecutan, la rama `main` sigue recibiendo fixes
del panel actual. El **merge** entre `main` y `rebuild` se hace al
final de E6, con un script de importación que reutiliza los
módulos viejos.

### 2.3 Flags de funcionalidad

```rust
// Cargo features
[features]
default = []
rebuild = []     // compila el rebuild
v1_compat = []   // preserva expone API v1 (deprecada)
```

`v1_compat` se quita en release 2.0.

---

## 3. Detalle por etapa

### 3.1 E0 — pre-flight

**Objetivo**: refundar el árbol, definir contratos, separar Rust puro.

**Entregables**:

- `Cargo.toml` con `[[bin]]` separados.
- `xtask/` con `gen-contracts` (genera `src/lib/contracts/*.ts`).
- `src-tauri/src/{domain,application,ports,adapters,platform,config}/`
  con `mod.rs` y stubs.
- `docs/CHANGELOG.md` con la entrada inicial.

**Criterios de aceptación**:

- `cargo build` exit 0 con la estructura nueva.
- `pnpm build` exit 0.
- `cargo test` exit 0 con stubs vacíos.

**Riesgo**: ninguno. Es solo reorganización.

### 3.2 E1 — domain + schemas

**Objetivo**: domain puro, schemas versionados, value objects, policies.

**Entregables**:

- `domain/entity/{site,service,operation,drift,clone,worktree,snapshot}.rs`.
- `domain/value/{slug,domain,endpoint,paths,exclude}.rs`.
- `domain/policy/{allowed_slug,allowed_domain,autoselect_endpoint,teardown}.rs`.
- `domain/error.rs` con `AppError` + variantes.
- `config/schema.rs` con `migrate_v1_to_v2`.
- `config/persist.rs` con `Config::load_or_init`.

**Criterios de aceptación**:

- `cargo test` ≥ 95 % cobertura en `domain/`.
- `cargo test` ≥ 90 % cobertura en `config/`.
- Tests de propiedades para `Slug`, `migrate_to_current`, `decide_teardown`.

**Riesgo**: bajo. Lógica pura.

### 3.3 E2 — application + operation coordinator

**Objetivo**: coordinador de operaciones, planes, compensación.

**Entregables**:

- `application/operation/mod.rs` con `OperationCoordinator`.
- `application/operation/plan.rs` con `PlanBuilder`, `Step`.
- `application/operation/journal.rs` con `OperationJournal`.
- `application/operation/progress.rs` con `ProgressSink`.
- `application/operation/compensation.rs` con compensación declarativa.
- `application/lifecycle/{start,stop,create}.rs` con `start_site_plan`,
  `stop_site_plan`, `create_site_plan`.
- `application/migrate.rs` con `migrate_plan`.
- `application/snapshot.rs` con `create_snapshot_plan`.
- `application/clone.rs` con `create_clone_plan`.
- `application/worktree.rs` con `create_worktree_plan`.
- `application/reconcile.rs` con `reconcile`.
- `application/backup.rs` con `stream_dump`.

**Criterios de aceptación**:

- `cargo test ≥ 90 %` en `application/`.
- Plan canónico de `start_site` pasa `assert_plan_invariants`.
- `cancel(op_id)` aborta antes de 100 ms en un test si el step
  está en `sleep`.

**Riesgo**: medio. Hay que decidir el shape del `Step` con cuidado.

### 3.4 E3 — adapters

**Objetivo**: implementaciones reales de los ports.

**Entregables**:

- `adapters/bollard/container.rs` con `BollardContainer`.
- `adapters/bollard/db.rs` con `Mysql`, `Mariadb`, `Postgres`, `Sqlite`.
- `adapters/bollard/image.rs` con `BollardImage`.
- `adapters/host/fs.rs` con `RealFileSystem`.
- `adapters/host/mkcert.rs` con `MkcertAuthority`.
- `adapters/host/dnsmasq.rs` con `DnsmasqConfig`.
- `adapters/host/netcheck.rs` con `LinuxPortChecker`.
- `adapters/host/process.rs` con `RealProcessRunner`.
- `adapters/host/shell.rs` con `RealShell`.
- `adapters/host/paths.rs` con `RealHost`.
- `adapters/host/keyring.rs` con `LibsecretKeyring`.
- `adapters/host/pkexec.rs` con `Pkexec`.

**Criterios de aceptación**:

- `cargo test` ≥ 70 % en `adapters/`.
- `cargo test -- --ignored --test-threads=1` exit 0.

**Riesgo**: medio. Bollard API ha cambiado; requiere verificar.

### 3.5 E4 — tauri + dbus + cli + mcp

**Objetivo**: exponer la Application API a través de las cuatro
superficies.

**Entregables**:

- `adapters/tauri/mod.rs` con `setup`, `invoke_handler`.
- `adapters/tauri/commands.rs` con un comando por `#[tauri::command]`.
- `adapters/tauri/error.rs` mapeo `AppError` → `AppErrorDto`.
- `adapters/tauri/events.rs` con bindings `op-log`, `log:{id}`,
  `sites-changed`, `drift-detected`.
- `adapters/dbus/server.rs` con `Manager` interface.
- `adapters/cli/wp.rs` con generación del wrapper.
- `adapters/cli/cli.rs` con generación del CLI.
- `bin/wordpress-panel-cli.rs` con el binario real.
- `mcp/server.mjs` actualizado con la nueva herramienta.

**Criterios de aceptación**:

- `pnpm build` exit 0.
- `pnpm test:e2e` exit 0.
- `gdbus call --session ...` funciona.
- `wp --version` funciona en un proyecto.

**Riesgo**: bajo. Ya hay precedente.

### 3.6 E5 — reconcile + recovery

**Objetivo**: reconciliador, startup_recovery, tests de recovery.

**Entregables**:

- `application/reconcile.rs` con `reconcile`, `detect_drifts`,
  `apply_fix`.
- `application/lifecycle/startup_recovery.rs` con `startup_recovery`.
- `domain/entity/drift.rs` con `Drift`, `DriftKind`, `Fix`.
- `domain/policy/reconcile.rs` con `should_autofix`.
- `src/routes/settings/+page.svelte` con `DriftList`.

**Criterios de aceptación**:

- Test `reconcile_detects_orphan_container` exit 0.
- Test `recovery_migrates_v1_config` exit 0.

**Riesgo**: bajo.

### 3.7 E6 — migrar código de los módulos existentes

**Objetivo**: sustituir los módulos actuales por los nuevos.

**Entregables**:

- `docker.rs` viejo → `BollardContainer` + `BollardDb`.
- `nginx.rs` viejo → `nginx::render_vhost` + `Step::WriteVhost`.
- `wordpress.rs` viejo → `application::lifecycle::create`.
- `migrate.rs` viejo → `application::migrate`.
- `snapshot.rs` viejo → `application::snapshot`.
- `clone.rs` viejo → `application::clone`.
- `worktree.rs` viejo → `application::worktree`.
- `backup.rs` viejo → `application::backup`.
- `autodump.rs` viejo → `application::lifecycle::WatchDatabaseChanges`.
- `lib.rs` viejo → `adapters/tauri/mod.rs` (≤ 200 líneas).

**Criterios de aceptación**:

- `cargo build --release` exit 0.
- `pnpm test:e2e` exit 0.
- Smoke test manual: crear proyecto, encender, parar, dump, snapshot,
  clone, worktree, importar LocalWP.

**Riesgo**: alto. Aquí es donde pueden aparecer regresiones.

### 3.8 E7 — hardening

**Objetivo**: pulir, documentar, añadir fallbacks.

**Entregables**:

- `docs/ARCHITECTURE.md` reescrito.
- `docs/EXTENDING.md` actualizado.
- `docs/CHANGELOG.md` completo.
- `docs/TESTING.md` actualizado.
- `scripts/first-run.sh` mantenible.
- `docs/MIGRATION.md` con guía v1 → v2.

**Criterios de aceptación**:

- `cargo doc --no-deps` exit 0 con todos los públicos documentados.
- `docs/CHANGELOG.md` cubre todas las etapas.

**Riesgo**: bajo.

### 3.9 E8 — beta

**Objetivo**: usar el panel 1.0 en desarrollo activo durante 2 semanas.

**Criterios de aceptación**:

- 0 pérdidas de datos.
- 0 ciclos de recover manual.
- Métricas de duración < 2x vs el panel v1.

**Riesgo**: medio. La beta descubre bugs.

### 3.10 E9 — release 1.0

**Objetivo**: tag y publicación.

**Entregables**:

- Tag `v2.0.0`.
- Binarios firmados.
- Notas de release.
- `docs/MIGRATION.md` con la checklist de upgrade.

**Criterios de aceptación**:

- Tag en repositorio.
- Binario disponible en CI artifacts.

**Riesgo**: bajo.

---

## 4. Estrategia de migración

### 4.1 Visión general

```
Panel v1.x ──┬──► Panel v2.0 (rebuild) ──► Panel v2.x
             │           ▲
             │           │
         coexistencia ───┘
```

El panel v1 sigue funcionando mientras el rebuild se valida. El
panel v2 abre las mismas carpetas `~/panel-wp/`.

### 4.2 Pasos de upgrade

1. **Backup**: `cp -r ~/panel-wp ~/panel-wp.bak.{timestamp}`.
2. **Parar v1**: cerrar la ventana / `pkill wordpress-panel`.
3. **Instalar v2**: `cargo install --path src-tauri` o `pnpm tauri dev`.
4. **Primer arranque**: v2 detecta configs v1, los migra a v2.
5. **Validar**: mismas URLs, mismos dumps, mismos snapshots.

### 4.3 Rollback

1. **Parar v2**: cerrar la ventana.
2. **Restaurar backup**: `cp -r ~/panel-wp.bak.{timestamp}/* ~/panel-wp/`.
3. **Reinstalar v1**: `cargo install --path src-tauri --version 1.x`.

### 4.4 Compatibilidad de configs

- v1 sin `schemaVersion` → v2 lee como `v1`, migra a `v2`.
- v2 con `schemaVersion: 2` → v1 no entiende; v2 lo rechaza si
  alguien intenta downgrade (chequeo en `Config::load_or_init`).

### 4.5 Compatibilidad de carpetas

| Carpeta | v1 | v2 |
|---|---|---|
| `~/panel-wp/{slug}/config.json` | sí | sí (migrado) |
| `~/panel-wp/{slug}/app/public/` | sí | sí |
| `~/panel-wp/{slug}/app/sql/` | sí | sí |
| `~/panel-wp/{slug}/ssl/` | sí | sí |
| `~/panel-wp/{slug}/snapshots/` | sí | sí |
| `~/.config/wordpress-panel/panel.json` | sí | sí (extendido) |
| `~/.config/wordpress-panel/groups.json` | sí | sí (migrado) |
| `~/.config/wordpress-panel/dump-log.jsonl` | sí | sí (migrado) |
| `~/.config/wordpress-panel/operations/` | n/a | sí (creado por v2) |
| `~/.config/wordpress-panel/audit.jsonl` | n/a | sí (creado por v2) |
| `~/.config/wordpress-panel/metrics.jsonl` | n/a | sí (creado por v2) |
| `~/.config/wordpress-panel/nginx/conf.d/` | sí | sí |

### 4.6 Compatibilidad de containers

| Container | v1 | v2 |
|---|---|---|
| `panel-net` | sí | sí |
| `panel-nginx` | sí | sí |
| `panel-mysql-{ver}` | sí | sí |
| `panel-mariadb-{ver}` | sí | sí |
| `panel-postgres-{ver}` | sí | sí |
| `panel-mailpit` | sí | sí |
| `panel-minio` | sí | sí |
| `panel-adminer` | sí | sí |
| `wp-{id}` | sí | sí (mismo nombre) |

El único cambio: `wp-{id}` puede re-taggarse con `IMAGE_REV` nuevo
mientras v2 introduce cambios en la imagen. Esto fuerza recreate la
primera vez.

### 4.7 Compatibilidad de D-Bus

- Nombre del servicio: `com.goldmediatech.WordpressPanel` (idéntico).
- Path: `/com/goldmediatech/WordpressPanel` (idéntico).
- Interface: `com.goldmediatech.WordpressPanel.Manager` (idéntico).
- Métodos nuevos son **aditivos** (no se renombran los viejos).
- El plasmoid KDE v1 sigue funcionando si solo usa los métodos
  documentados.

### 4.8 Compatibilidad de MCP

- `mcp/server.mjs` v1 ya es solo un envoltorio del CLI. La nueva
  versión **añade** herramientas sin romper las existentes.
- Handshake MCP sin cambios.

---

## 5. Backlog explícito

### 5.1 IA / agente (`agent.rs`)

**Estado**: diferido al rebuild.

**Por qué se difiere**:

- Introduce dependencias de red pesadas (clientes HTTP de OpenAI,
  Anthropic, Ollama).
- Añade surface de ataque (API keys, scopes, prompts).
- Complejidad de evaluación (testing de LLM es estocástico).
- El usuario decide cuándo activarlo.

**Diseño futuro** (no en este rebuild):

```rust
// application/agent/mod.rs
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AppError>;
    async fn tool_use(&self, req: ToolUseRequest) -> Result<ToolUseResponse, AppError>;
}

pub struct Agent {
    pub provider: Arc<dyn Provider>,
    pub keyring: Arc<dyn KeyringAccessor>,
    pub history: Arc<Mutex<ChatHistory>>,
    pub sandbox: Box<dyn Sandbox>,
    pub approver: Arc<Approver>,
}
```

Cada `tool_use` es un **step** de `OperationCoordinator`. La
aprobación es un UI step explícito.

### 5.2 Deploy a VPS / Cloudflare Tunnel

**Estado**: stubs visibles (`feature_stub` con `cloudflare`, `deploy`,
`package`).

**Por qué se difiere**:

- Cloudflare Tunnel requiere un binario `cloudflared` y credenciales.
- Deploy a VPS requiere definir el target (rsync, ssh, fly.io, etc.).
- Multi-host destruye la regla "1 host = 1 panel".

**Diseño futuro**:

```rust
// application/deploy/mod.rs
pub enum DeployTarget {
    Ssh { host: String, path: String },
    CloudflareTunnel { token: String },
    FlyIo { app: String },
    PackageOnly { dest: PathBuf },
}

pub struct DeployPlan {
    pub target: DeployTarget,
    pub steps: Vec<Step>,
}
```

### 5.3 Plugin S3 (WP ↔ MinIO)

**Estado**: MinIO soportado en infraestructura, plugin WP no incluido.

**Por qué se difiere**:

- El plugin WP es código WordPress (no Rust).
- Mantener un plugin en el repo del panel no es sostenible.
- Alternativa: instalable vía `wp-cli`.

**Diseño futuro**:

- Subcomando `wordpress-panel-cli minio install` que:
  1. Arranca MinIO.
  2. Crea bucket por defecto.
  3. Genera `wp-config.php` con constantes.
  4. Sugiere instalar "WP Offload Media Lite" desde el admin.

### 5.4 Container de frontend headless

**Estado**: flags guardados, container no aprovisionado.

**Por qué se difiere**:

- Build de Next/Nuxt/Astro requiere node + lockfile.
- El usuario ya tiene su frontend en su repo.

**Diseño futuro**:

- Step `EnsureFrontendContainer` opcional.
- `build_dirs` y `build_cmd` ya existen para `gh_deploy`.

### 5.5 Backup remoto de snapshots

**Estado**: backups locales.

**Por qué se difiere**:

- Requiere un destino (S3, rsync, etc.).
- Cifrado en tránsito es un reto.
- Snapshots ya tienen buen manejo local.

**Diseño futuro**:

- `Step::PushSnapshot { snapshot_id, target: RemoteTarget }`.
- `application::backup::remote_upload`.

### 5.6 Multi-host / cluster

**Diferido a futuro lejano**. No encaja con el modelo de panel
individual.

### 5.7 Notificaciones del sistema

**Estado**: solo op-log.

**Por qué se difiere**:

- Requiere `notify-rust` o DBus `org.freedesktop.Notifications`.
- No es prioritario.

**Diseño futuro**:

- `application::notify::send(title, body, urgency)`.

### 5.8 Mejoras de UI

- [ ] Búsqueda en la lista de proyectos.
- [ ] Filtros por servicio / estado.
- [ ] Modo split horizontal/vertical del master-detail.
- [ ] Rearrastrable de cards en la lista.
- [ ] Tema claro (no solo dark).

### 5.9 Auto-update

**Estado**: no implementado.

**Por qué se difiere**:

- Requiere un canal de update (GitHub releases, custom server).
- Necesita firma de binarios.

**Diseño futuro**:

- `application::update::check()` con `crc32` y `ed25519` sig.
- `application::update::apply()` con rollback.

### 5.10 Compatible con macOS

**Estado**: estructura de platform ports preparada, soporte real
**diferido**.

**Por qué se difiere**:

- macOS tiene sus propios quirks (Gatekeeper, sudo, brew).
- El usuario objetivo es Linux.

**Diseño futuro**:

- `adapters/host/macos/` con `lsof`, `scutil`, `dns`.
- `scripts/first-run-macos.sh`.

---

## 6. Criterios de aceptación por release

### 6.1 v2.0.0

- Cumple todos los criterios de 03-06.
- Tests de migración v1 → v2 verdes.
- Rollback documentado y probado.
- Beta de 2 semanas sin pérdida de datos.

### 6.2 v2.1.0

- Macrofoco: integrar el reconciliador en el flujo principal.
- Métricas expuestas en una nueva ruta `/metrics`.
- Schemas v2 → v3 (si corresponde).

### 6.3 v2.2.0

- Soporte macOS beta.
- Theme light.

### 6.4 v3.0.0

- IA (Fase 5 de PLAN.md).
- Deploy a VPS (beta).
- Tema multilingüe.

---

## 7. Señales de éxito

### 7.1 Internas

- `panel.op.duration_seconds{ kind="start_site" } p95 < 2s`.
- `panel.reconcile.drifts{ severity="error" } count == 0` en steady state.
- `panel.autodump.bytes{}` suma coherente con el tráfico.
- Tests de integración verdes nightly.

### 7.2 Externas

- Sin pérdidas de datos reportadas.
- Cancelación de operaciones funciona.
- Recover tras apagón sucio sin intervención.

### 7.3 Negativas (anti-señales)

- `panel.op.duration_seconds p99 > 30s`.
- Drift `error` recurrentes.
- Tasa de compensación fallida > 5 %.

---

## 8. Riesgos globales

### 8.1 Compatibilidad con configs actuales

**Mitigación**: tests de migración v1 → v2 con fixtures reales.

### 8.2 Cambios en bollard API

**Mitigación**: pin de versión en `Cargo.toml`. Test de integración
detecta breakage antes de release.

### 8.3 Rendimiento del rebuild

**Mitigación**: smoke test con 10 proyectos, midiendo `get_sites` < 200 ms.

### 8.4 Pérdida de datos durante el upgrade

**Mitigación**: `startup_recovery` con `RecoveryReport` detallado.
Backup automático antes de cualquier `write_site`.

### 8.5 Sobrecarga de la operación de compensación

**Mitigación**: compensadores idempotentes, logueados, limitados
(compensación falla → drift).

### 8.6 Edge cases del reconciliador

**Mitigación**: `decide_teardown` y `should_autofix` son **puros**,
testeables con property-based tests.

### 8.7 Saturación del journal

**Mitigación**: rotación por tamaño (5 MB por op, 50 MB total).

### 8.8 Saturación de `metrics.jsonl`

**Mitigación**: agregación en memoria + flush cada minuto.

---

## 9. Plan de comunicación

### 9.1 Documentación

- `docs/MIGRATION.md`: guía paso a paso v1 → v2.
- `docs/CHANGELOG.md`: cada cambio relevante.
- `docs/RELEASE.md`: notas de release v2.0.0.
- `docs/ARCHITECTURE.md`: reescritura según el rebuild.

### 9.2 Comunicación

- Anuncio en el readme principal.
- Tag en el repo.
- Actualización del `package.json` y `Cargo.toml`.

### 9.3 Soporte

- Issues con label `v1-deprecation`.
- FAQ en `docs/FAQ.md`.

---

## 10. Cierre de la serie

Esta serie de 7 documentos define el rebuild del Panel WP desde cero,
manteniendo lo que funciona (recursos, portabilidad, principio rector,
auto-dump, dumps segunda defensa, worktrees por overlays, archivos
versionados, configs autosuficientes, backend autoridad, adaptadores
finos) y resuelviendo la deuda (acoplamiento, errores sin tipo,
operaciones dispersas, sin reconciliador, sin validación, sin
contract tests, sin platform ports).

Los capítulos están diseñados para ser leídos en orden:

1. **Objetivos**: el qué.
2. **Arquitectura**: la forma.
3. **Contratos**: la frontera.
4. **Orquestación**: el motor.
5. **Seguridad**: los límites.
6. **Pruebas**: la confianza.
7. **Roadmap**: el cuándo.

El rebuild es un **proyecto de 12 semanas** con etapas bien
delimitadas, cada una con criterios de aceptación verificables y
riesgos acotados. La compatibilidad v1 → v2 es una preocupación
central, no un nice-to-have.

Las decisiones tomadas aquí se basan en el código actual:

- `src-tauri/src/{docker,nginx,wordpress,migrate,snapshot,clone,worktree,backup,autodump,config,dbus,cli}.rs`
- `scripts/wordpress-panel-cli.sh`, `scripts/wp-wrapper.sh`,
  `scripts/first-run.sh`.
- `mcp/server.mjs`.
- `src/routes/+page.svelte`, `src/routes/+layout.svelte`,
  `src/lib/{components,api,types}.ts`.

El backlog explícito (IA, deploy VPS, plugin S3, headless frontend,
auto-update) es deliberadamente **fuera** del alcance del rebuild.
Reentrar en él es responsabilidad de un proyecto posterior, con sus
propios criterios de aceptación y sus propios riesgos.

La última frase es la regla de oro del rebuild: **si una función
rompe las tres reglas de CLAUDE.md, se rediseña o se pospone**. Las
tres reglas permanecen.
