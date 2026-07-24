# 01 · Objetivos, principios y no-objetivos

> Documento 1 de 7 de la serie **Reconstrucción desde cero**.
> Compilador: este fichero fija el «qué» y el «qué no». Sustenta las decisiones
> de arquitectura, contratos, orquestación, observabilidad, calidad, roadmap y
> migración que desarrollan los capítulos 02–07. Conviene releerlo antes de
> cualquier cambio de criterios.

---

## 1. Resumen ejecutivo

El Panel WP gestiona proyectos WordPress locales con Docker, sustituyendo a
LocalWP con tres reglas innegociables:

1. **Nada corre si no hace falta.** Un proyecto parado = 0 containers.
2. **Compartir antes que duplicar.** Nginx, DB por versión, mailpit y
   adminer son únicos por máquina y on-demand.
3. **Imágenes mínimas.** `alpine` cuando exista; PHP-fpm por proyecto sin
   publicar puertos al host, solo habla con `panel-nginx` por la red interna.

Estas tres reglas están consolidadas en el código actual (ver `CLAUDE.md`,
`docs/ARCHITECTURE.md`) y han permitido construir 4 fases estables (MVP core,
funcionalidades completas, servicios compartidos, configuración + migración +
snapshots + clones + worktrees). Un rebuild desde cero debe **conservarlas**
como axiomas y resolver la deuda técnica que han ido dejando:

- Contratos `Result<T, String>` sin modelo de error tipado.
- Acoplamiento entre `docker.rs`, `nginx.rs`, `wordpress.rs`, `backup.rs` y los
  comandos IPC (`lib.rs` mezcla orquestación con Tauri).
- Auto-dump como `JoinHandle` en estado Tauri: ciclo de vida frágil, sin
  reconciliador, sin journal de compensación.
- Snapshots/clones/worktrees como flujos «a medida» sin un «operation
  coordinator» común: cada uno reinventa su manera de arrancar/parar/limpiar.
- Falta de reconciliador desired-vs-actual: el estado real se reconstruye por
  `docker inspect` y el deseado por `config.json`; no hay un detector de drift
  que diga «el container `wp-X` existe pero su config no».
- Sin contratos de streaming/versionado/migrations para `config.json` ni para
  `meta.json`/`groups.json`/`dump-log.jsonl`.
- Pruebas: lógica pura sí, pero sin tests de unidad del coordinador de
  operaciones, ni tests de contrato IPC, ni tests de propiedades sobre la
  reconciliación.

Este documento fija **qué** debe ser el rebuild y **qué no**, no cómo.

---

## 2. Objetivos del rebuild

### 2.1 Funcionales

- **Encender / apagar** un proyecto WordPress con un click, dejando recursos
  en cero cuando está apagado (ya logrado, objetivo a **conservar**).
- **Crear proyectos nuevos** con WP/PHP/DB configurables, incluyendo presets
  para los flujos más comunes (PHP 8.3 + MySQL 8.0, SSL con mkcert, auto-login).
- **Importar proyectos** desde LocalWP y desde carpetas desconectadas
  (preservadas o reconstruidas). Sin pérdida y con migración reproducible.
- **Puntos de guardado (snapshots)** de un proyecto: tar del código sin
  uploads/cache + dump SQL completo + `meta.json`.
- **Clones temporales** desde un snapshot, compartiendo motor DB/nginx del
  padre (1 contenedor php + 1 esquema extra).
- **Worktrees de tema/plugin** aislados sobre una rama nueva, compartiendo el
  `public/` del padre por montaje Docker (cero copias de código).
- **Auto-dump** defensivo: vigila la DB de los proyectos activos y deja un
  volcado fresco en `app/sql/` cuando hay cambios (segunda línea de defensa
  tras el export-al-detener).
- **Servicios compartidos** on-demand: nginx, mailpit, adminer, minio (este
  último por feature flag).
- **D-Bus** para el plasmoid KDE (lista + detener proyectos), **CLI** en
  `~/.local/bin` para usar `wp` por terminal, **MCP** para agentes IA.
- **Observabilidad** mínima: progreso de operaciones largas (`op-log`),
  resource usage (`docker stats`), logs en vivo (`log:{id}`), estado del
  sistema (Docker, dnsmasq, mkcert, wrappers).

### 2.2 No funcionales

- **Portabilidad por proyecto**: una carpeta `~/panel-wp/{slug}/` con su
  `config.json` es autosuficiente. Copiada a otro sistema con el panel
  instalado: detectada, importable, `migrationPending` y reconducible.
- **Cero recursos parado**: el walker/shared-service-shutdown apaga
  contenedores compartidos (`panel-nginx`, `panel-mailpit`, `panel-adminer`,
  `panel-minio`, DB por versión) que nadie use.
- **Idempotencia** de todos los flujos: `ensure_*`, `repair_*`, `sync_*`,
  `rotate_dumps`, `clean_dump_log`. Ninguna llamada debe fallar por haber
  corrido parcialmente.
- **Arranque recuperable**: tras un apagón sucio, `start_site` tolera
  containers huérfanos, redes ausentes, datadirs presentes.
- **Trazabilidad**: cada operación que toca el host deja un rastro
  recuperable (journal, log, snapshot).
- **Extensibilidad**: agregar un motor DB, una versión PHP, un servicio
  compartido, un comando IPC, un canal de progreso, debe ser una receta
  corta y tipada (ver `docs/EXTENDING.md`).
- **Seguridad local-first**: nada sale del host sin consentimiento; los
  secretos viven en el keyring; los puertos al host son solo loopback.
- **Aislamiento**: el `wp-{id}` no publica puertos. Solo `panel-nginx`
  expone 80/443 (o alternos) al loopback.

### 2.3 Arquitectura objetivo (resumen)

```
UI (SvelteKit SPA, master-detail)
   │  ipc tipado (contrato generado)
   ▼
Application API / use cases / operation coordinator
   │  políticas, validaciones, compensaciones
   ▼
Domain (entidades puras: Site, Service, Operation, Snapshot, Worktree)
   │  políticas (reconciliador desired vs actual)
   ▼
Ports (interfaces para infraestructura)
   │  adaptadores
   ▼
Adapters: Bollard/docker · host (mkcert, gh, dnsmasq) · Tauri ·
          D-Bus (zbus) · CLI (wordpress-panel-cli) · MCP (Node)
```

Lo importante:

- **El backend sigue siendo la única autoridad.** `docker.rs`, `worktree.rs`,
  `clone.rs`, `snapshot.rs`, `migrate.rs`, `localwp.rs` no se duplican en la
  UI ni en el CLI: ambos son adaptadores que llaman a la misma Application
  API (hoy encapsulada en `lib.rs`).
- **El contrato UI↔backend se genera.** Hoy cada `#[tauri::command]` se
  reescribe a mano en `api.ts` y `types.ts`. Sigue siendo válido, pero el
  rebuild debe poder derivarlo (al menos los TS types) desde los modelos
  serde para evitar drift.
- **El coordinador de operaciones es un módulo de primera clase.** Hoy cada
  flujo largo (`create_site`, `migrate`, `create_clone`, `create_worktree`,
  `create_snapshot`, `migrate`) tiene su propio «run → log → cleanup en
  error». En el rebuild se eleva a un `OperationCoordinator` que emite
  `operationId`, progreso tipado, cancel, y journal de compensación.

---

## 3. Principios rectores

Estos principios son la versión consolidada y ligeramente extendida de los
tres de `CLAUDE.md`. Los nuevos son consecuencia de la deuda detectada.

### 3.1 Recursos (axioma, no negociable)

- Todo servicio *compartido* arranca on-demand, **nunca por defecto**.
- Si N proyectos activos pueden compartir un container, **se comparte**.
- Al detener un proyecto, los compartidos que ya nadie use se apagan.
- N proyectos parados = 0 containers. Si lo viola, es un bug.

### 3.2 Fuente de verdad

- **Proyectos**: `~/panel-wp/{slug}/config.json` + carpeta del proyecto.
  No hay base de datos central. `load_all_sites()` escanea.
- **Estado del panel**: `~/.config/wordpress-panel/panel.json` (endpoint).
- **Grupos**: `~/.config/wordpress-panel/groups.json` (orden + conjunto).
- **DBs**: `~/.config/wordpress-panel/db-data/{container}/` (datadir
  bindeado, durable). No en volúmenes anónimos (huérfanos tras recreate).
- **Snapshots**: `~/panel-wp/{slug}/snapshots/{id}/`.
- **Dumps**: `~/panel-wp/{slug}/app/sql/`.
- **Log de volcados**: `~/.config/wordpress-panel/dump-log.jsonl` (JSONL).
- **Snippets**: `~/.config/wordpress-panel/nginx/conf.d/`, `dnsmasq-panel.conf`.

### 3.3 Aislamiento y autoridad del backend

- El backend es la **única** autoridad sobre los recursos. La UI y el CLI
  son adaptadores. Si necesitan un dato, lo piden.
- **Bollard es el camino por defecto** en runtime. Las excepciones (CLI
  `docker` ya documentadas: `docker build` de la imagen php, `docker exec -i`
  para el import de dumps grandes, `docker cp` para la migración legacy de
  datadir) deben quedar reducidas a comentarios / concerns circumscriptions
  explícitos, no a convenciones implícitas.
- Container por proyecto = **sin puertos host**. El puerto nuevo del stack
  (Adminer, Mailpit, MinIO) se publica solo a `127.0.0.1`.

### 3.4 Idempotencia y compensaciones

- Cada flujo expone entradas idempotentes. `ensure_*`, `sync_*`,
  `repair_*`, `migrate_*`, `register_*` se pueden llamar dos veces sin
  efectos destructivos.
- Los flujos largos que **mutan** (crear, clonar, worktree, snapshot,
  importar) llevan un **journal de compensación**: si un paso falla, se
  ejecuta la «vía de salida» automática (stop de container, drop de schema,
  `rm -rf` del slug, `git worktree remove`). El `worktree.rs::run_create`
  ya lo hace inline; el objetivo es estandarizarlo.

### 3.5 Cancelación y progreso

- Las operaciones largas exponen `operationId` (UUID v4) en el momento en
  que se las invoca. El frontend guarda `operationId → {status, cancel}`
  y la consola pinta en base a él.
- El progreso se emite por un canal **tipado**:
  `OpEvent::Plan { steps }` → `Step { i, n, label, status }` →
  `Progress { i, n, ratio, units }` → `Line { text }` → `Done { result } |
  Failed { error, compensation }`. El actual `progress.rs::log` (string
  libre) se conserva como API de salida, pero el contrato interno es
  estructurado.
- Cancelación: el coordinador expone `cancel(operationId)` que aborta de
  forma cooperativa (cooperate-cancel: el watcher de loops chequea un
  `CancellationToken`). El UI ya tiene un ejemplo en `DeleteProjectModal`
  (cuenta atrás de 5 s antes de `delete_site`); ese patrón debe ser
  general.

### 3.6 Tipos y versionado

- **Modelos serde en `camelCase`** (`#[serde(rename_all="camelCase")]`).
  Los tipos TS de `src/lib/types.ts` son el espejo, pero el rebuild
  introduce generación automática de tipos TS desde un IDL mínimo (`tsc`
  strict + `zod` en runtime para frontera IPC).
- `config.json` de cada proyecto lleva `schema_version: u32` y existe
  `migrate::config_schema::migrate_vN_to_vN+1()` para cada paso. Cargar
  una versión más alta es **error cerrable** (no se carga ni se sobrescribe).
- Los `meta.json` de snapshots, los logs JSONL, y los `groups.json` siguen
  el mismo principio: versionar el campo raíz cuando cambian (p. ej.
  `schemaVersion` en `dump-log.jsonl`).

### 3.7 Persistencia, atomicidad y locks

- Escrituras en JSON del disco: **write-temp + rename atómico**. Hoy
  `serde_json::to_string_pretty + fs::write` es no-atómico y un kill a
  mitad deja configs truncadas. El rebuild obliga a `atomic_write(path, bytes)`.
- Para `groups.json` y `dump-log.jsonl`, **locks cooperativos por nombre**
  `flock` o `fs2::FileExt::lock_exclusive` con timeout corto.
- Migración de schema atómica: escribir `config.json.new`, validar parse,
  `rename` sobre `config.json`. Si el parse falla, no se aplica.
- Backup antes de mutar destructiva: `delete_site` con `deleteFolder=true`
  ya deja un dump por el export-al-detener; el rebuild lo garantiza también
  en `delete_snapshot` (preserva el último) y `remove_worktree_site` con
  `deleteBranch=true`.

### 3.8 Reconciliador desired-vs-actual

- **Estado deseado**: lo que dicen los `config.json`, los `meta.json` de
  snapshots, el `groups.json`, el `panel.json`.
- **Estado actual**: lo que dice `docker inspect` (containers, redes,
  imágenes), `/proc/net/tcp` (puertos), `/etc/NetworkManager/dnsmasq.d/`
  (DNS), el árbol `~/panel-wp/`.
- El reconciliador detecta **drift** y propone remedio:
  - Container `wp-{id}` huérfano → ofrece «borrar» o «re-attach» al
    `config.json` que coincida por nombre/path.
  - DB `panel-mysql-80` corriendo pero ningún proyecto activo la necesita
    → apagado.
  - Vhost en `conf.d/{id}.conf` sin `config.json` → borrado.
  - `config.json` cuyo container NO existe → marca `stopped` (no error).
- En el código actual esto está implícito en `get_sites` (mezcla
  `load_all_sites` + `docker.site_status`) y en `teardown_unused_shared`.
  El rebuild lo separa como caso de uso propio: `reconcile(state) → Drift`.

### 3.9 Observabilidad y recuperabilidad

- **Logs estructurados** en el backend (`tracing` con `json` layer en
  `dev`, `compact` en prod). Hoy se usa `eprintln` y `log(app, …)` para
  emitir al frontend; el rebuild unifica.
- **Métricas operativas** mínimas: contador de operaciones, histograma de
  duración por tipo, éxito/error por `operationId`. Sin Prometheus: archivo
  JSONL con cap rotada.
- **Recuperación al arranque**: `startup_recovery` mide containers
  huérfanos, datadirs sin container, archivos `.disconnected.json` y
  `.tmp` huérfanos, y los integra en el estado.

### 3.10 Seguridad local-first

- Puertos host siempre `127.0.0.1` (mailpit, minio, adminer, nginx).
- Secretos DB (`root` pass `panel`) **no** son secretos: son contraseña de
  uso local; documentado y aceptable. La **API key del proveedor de IA**
  sí va al keyring (`libsecret` / `Keychain`).
- mkcert CA local: explícita y documentada; primer uso asistida por
  `scripts/first-run.sh`.
- Validación de paths: `slugify`, normalización de `snapshot_excludes`,
  desambiguación de `find_free_slot`. El rebuild debe centralizar en
  `path_util` y nunca escribir fuera de `~/panel-wp/{slug}/` ni de
  `config_dir` sin chequeo explícito.

---

## 4. Deuda técnica que el rebuild debe resolver

Lista cerrada de problemas identificados durante la revisión del código que
desaparecen en el rebuild. **No son bugs abiertos**, son el motivo de la
reconstrucción.

### 4.1 Acoplamiento cruzado en `lib.rs`

`lib.rs` (1080 líneas) mezcla:
- comandos IPC `#[tauri::command]`,
- helpers (`wpcli_json`, `load_site`, `reconstruct_config`),
- side-effects (`delete_site` apaga + borra vhost + DROP DATABASE + teardown +
  borra carpeta + sidecar rename + emite `op-log`),
- registro manual del `invoke_handler!`.

El rebuild lo escinde en **Application API** (casos de uso) y **Tauri
adapter** (sólo `#[tauri::command]` que delega, una línea por comando).

### 4.2 `Result<T, String>` y errores sin tipar

Todos los comandos devuelven `Result<T, String>` mapeados por el helper
`e()`. Cualquier JSON para el frontend requiere empacar el error en un
string. El rebuild lo reemplaza por un **`AppError`** con variantes
(`AppError::Docker`, `AppError::Conflict`, `AppError::NotFound`,
`AppError::Validation`, `AppError::Io`, `AppError::Permission`, …) que
serializa a un shape estable (`{code, message, hint?, cause?}`). El
frontend lo tipa en TS con `zod` y muestra el hint.

### 4.3 Auto-dump como `JoinHandle` en estado Tauri

`AutoDump(Mutex<HashMap<String, JoinHandle<()>>>)` vive en estado Tauri,
lo que obliga a `start/stop` en cada comando y duerme al cerrar el panel.
El rebuild lo desacopla: el coordinador de operaciones agenda
`WatchDatabaseChanges` como un job cancelable que sobrevive el ciclo de
iniciar/parar el comando.

### 4.4 Operación = función con cleanup inline

`worktree.rs::run_create`, `clone.rs::run`, `migrate_site` tienen patrones
similares (steps, log, on-error cleanup). El rebuild los unifica detrás
de `OperationCoordinator::execute(plan)`.

### 4.5 Sin validación a priori de paths y nombres

`slugify` y `find_free_slot` están en `wordpress.rs` y `clone.rs`, pero
falta un módulo cohesivo de validación. El rebuild introduce `domain/`
carpeta con `slug.rs`, `path.rs`, `dns.rs` (resolución), `endpoint.rs`.

### 4.6 Tipos TS a mano

`src/lib/types.ts` está sincronizado a mano con `config.rs`. Cualquier
nuevo campo exige tocar ambos. El rebuild genera `types.ts` desde un IDL
JSON-Schema exportado por `serde` (con `schemars`).

### 4.7 Sin reconciliador

El estado real y el deseado se cruzan solo en `get_sites` y
`teardown_unused_shared`. No hay un panel de salud que diga «tienes 1
container huérfano y 1 vhost fantasma» (ver `git log` del fix
`feat(nginx): autocura de vhosts huérfanos + comando repair_nginx`).

### 4.8 Sin streaming backups

`backup::dump_bytes` carga en memoria. Está OK para proyectos típicos
(WordPress pequeño), pero un sitio grande (200 MB dump) tensiona la RAM.
El rebuild introduce `backup::stream_dump(writer)` que escribe chunks al
disco (snapshot-friendly).

### 4.9 Sin paths de plataforma

`dirs::config_dir()`, `dirs::home_dir()` ya se usan, pero la lectura de
`/proc/net/tcp` es Linux-only. El rebuild declara plataforma-aliases
`host::ports`, `host::dns`, `host::mkcert_ca_path` con fallbacks
(documentados como no-soportados en macOS/Windows).

### 4.10 Sin pruebas de integración de la reconciliación

`integration_tests.rs` cubre creación, migración, import local, snapshots,
but no `reconcile`. Pasa a `tests/reconcile.rs` con escenarios
deterministas.

### 4.11 Sin pruebas de contrato IPC

No hay golden tests de los `invoke` argumentos/retornos. El rebuild
añade `tests/contract/*.snap` por comando.

### 4.12 Inconsistencias en errores entre sites-changed y reload

`emit("sites-changed")` lo hace `dbus.rs` cuando muta por D-Bus, pero
los `t::command` no siempre lo emiten (p. ej. `set_site_group` no emite,
porque la UI ya refresca). El rebuild centraliza en el coordinator.

### 4.13 D-Bus con tipos simples

Para simplificar `gdbus`, `dbus.rs` devuelve JSON strings. El rebuild
mantiene esa decisión pero formaliza un `JsonCommand` por método para
que MCP/CLI consuman el mismo envelope.

---

## 5. No-objetivos

Decisiones de_scope_ que **no** forman parte del rebuild. Sirven para que
los siguientes capítulos no se desvíen.

### 5.1 IA / agente (`agent.rs`, Fase 5)

Diferido. La `Feature flag: ai` queda en backlog junto con:
- `Provider` trait con `openai`, `anthropic`, `ollama`.
- Keyring (`libsecret`).
- Tool approval flow con diff preview.
- Aislamiento de la sesión WebView/sandbox.

**Por qué se separa**: la IA introduce superficie de ataque (keys,
muestras, scopes), dependencias de red pesadas, y complejidad de
evaluación. Desecharlo del rebuild evita que las decisiones de
arquitectura (operationId, cancel, journal) se contaminen con requisitos
específicos de prompt-tooling.

### 5.2 Deploy a VPS / plugin S3 / container headless

Diferido. La UI ya tiene botones `feature_stub` para Cloudflare Tunnel,
Deploy, Empaquetado. Se mantienen como stubs; el rebuild **no** los
convierte en funciones reales. La integración WP↔MinIO (plugin S3) y
los containers de frontend headless también son diferidos: solo se
guardan los flags en `SiteConfig` si la UI lo ofreció.

### 5.3 Multi-host / cluster

Diferido. No hay plan de ejecutar el panel en un host y los workers en
otro. El modelo es un host = un panel. Esto descarta un montón de
complejidad (locks distribuidos, raft, etc.).

### 5.4 Multi-tenancy / cuentas

Diferido. No hay users. El panel corre como el usuario de la sesión y
comparte su `~/panel-wp/`. El D-Bus expone su servicio al usuario
actual. No hay login.

### 5.5 Plataformas no-Linux

La build y los binarios son Linux. macOS soportado a nivel de
compilación, pero el binario `mkcert` y la lógica de dnsmasq están
escritos para Linux-first. El rebuild introduce `platform/` con traits
para que al menos la compilación a otros targets no rompa, pero el
soporte funcional sigue siendo Linux.

### 5.6 Webview empaquetable / installer .deb/.rpm

Diferido. El binario se distribuye vía `cargo run` / `pnpm tauri dev`,
no vía paquete. El script `package-plasmoid.sh` se mantiene para KDE.

### 5.7 Internacionalización de la UI

Diferido. La UI está en español; no hay i18n. Si en el futuro se decide
i18n, debe ser un solo `i18n.ts` central, no strings sueltos.

### 5.8 Persistencia de snapshots en S3/MinIO

Diferido. Los snapshots son locales (en `~/panel-wp/{slug}/snapshots/`).
La integración con MinIO vive en el contenedor del sitio (plugin WP
externo), no en el backend del panel.

---

## 6. Criterios de éxito del rebuild

El rebuild se considera *aceptable* cuando se cumplen **todos** los
siguientes. Estructurados como criterios de aceptación, no como promesa
de versión.

### 6.1 Criterios de comportamiento

- `pnpm tauri dev` levanta la UI; `start` / `stop` de un proyecto en
  menos de 2 s cuando los containers ya están creados.
- 0 proyectos activos = 0 containers corriendo (`docker ps` lo confirma).
- Apagón de máquina con 1 proyecto activo: al volver, el panel arranca,
  el reconciliador detecta el container huérfano, y el contenedor next
  `start` lo integra; el último dump en `app/sql/` no tiene más de 20 s
  de antigüedad (polling del auto-dump).
- Un proyecto importado desde LocalWP, con un dump de 50 MB, se migra
  end-to-end en menos de 3 minutos en hardware objetivo.
- Cancelar una migración (Ctrl+C / botón «Cancelar» en la consola) deja
  el sistema en un estado **consistente**: el container huérfano, si
  existe, se apaga; el `config.json` mantiene `migrationPending: true`;
  el slug no aparece en `get_sites` hasta que el usuario reintente.

### 6.2 Criterios de calidad

- Lógica pura en `#[cfg(test)] mod tests` con > 80 % de cobertura en
  `domain/`, `application/`, `backup/`, `migrate/`, `config_schema/`.
- `tests/contract/` con golden tests para cada comando IPC.
- `tests/integration_reconcile.rs` con los siguientes escenarios:
  - container huérfano + config válido,
  - vhost huérfano + config válido,
  - vhost huérfano + config ausente (auto-borrado),
  - datadir bindeado vs container recreado vs datadir legado.
- `cargo test` (sin red, sin Docker) corre en menos de 60 s.
- `cargo test -- --ignored --test-threads=1` con todos los `zztest-*` en
  verde, documentado en `docs/TESTING.md`.

### 6.3 Criterios de plataforma

- `dnsmasq` wildcard `*.test` instalado por `scripts/first-run.sh`
  (idempotente) en una instalación limpia.
- Panel-nginx zombie tras apagón: el `reload_nginx` del futuro hace
  `recreate` si el `exec` falla (mismo patrón que el fix actual).
- `paths` con `dirs::config_dir()` y `dirs::home_dir()` siempre;
  ninguna ruta hardcoded a `/home/user/panel-wp/` o similar.

### 6.4 Criterios de extensibilidad

- Agregar un motor DB (p. ej. SQLite) requiere:
  - una línea en `config::DbType`,
  - un método `DbEngine::create_database/dump/import`,
  - un selector en la UI.
  Documentado en `docs/EXTENDING.md`.
- Agregar un servicio compartido (p. ej. un proxy SMTP) sigue la
  receta `Ensure::ensure_X → SharedLifecycle::teardown_if_unused`.
- Una operación larga nueva (p. ej. «duplicar proyecto») usa el
  `OperationCoordinator` con un `plan` de steps declarativo.

### 6.5 Criterios de mantenimiento

- `lib.rs` ≤ 200 líneas: solo registrador de comandos y `setup()`.
- Cada `#[tauri::command]` ≤ 8 líneas: delega a un caso de uso.
- `docker.rs` ≤ 600 líneas: solo `DockerManager` (API Docker) +
  `Ensure*` para contenedores.
- `nginx.rs` ≤ 200 líneas: solo `render_vhost` + `write_vhost` +
  `remove_vhost`.
- Ningún `unsafe { … }` salvo en `libc_getuid/getgid` (que ya está).

---

## 7. Decisiones y trade-offs

### 7.1 Mantener Tauri como host del proceso

- **Pro**: el ciclo de vida del panel (ventana, IPC, capability, plugins)
  ya está aprendido en el código actual. Tauri 2 maduró su ACL y su
  sistema de eventos.
- **Contra**: limita la اختبار-ability end-to-end (necesita la GUI).
- **Decisión**: se mantiene. La «testeabilidad pura» se cubre con
  `tauri::test::mock_app()` (ya en uso) y la separación
  `application/port` permite mockear el adapter Tauri.

### 7.2 Mantener el binario único (no separar CLI/paneles)

- **Pro**: simplifica el despliegue y la coherencia.
- **Contra**: el CLI habla con el panel por D-Bus; cuando el panel no
  está, el CLI falla.
- **Decisión**: se mantiene. Ya hay un patrón claro: CLI detecta el
  proyecto por CWD, invoca D-Bus, devuelve JSON. El MCP sigue siendo un
  envoltorio del CLI. **No** se separa en procesos.

### 7.3 Schema versionado en `config.json`

- **Pro**: migrar hacia adelante es seguro; el panel sabe qué versiones
  entiende.
- **Contra**: introduce código de migración que solo se ejecuta una vez.
- **Decisión**: adopción. Coste bajo, valor alto. La regla es:
  `schemaVersion` es obligatorio para configs nuevos; las configs legacy
  sin el campo se asumen `v1` (la actual).

### 7.4 OperationCoordinator tipado (no string crudo)

- **Pro**: serializa un plan declarativo (`plan.steps: Vec<Step>`) que
  la UI puede mostrar como checklist.
- **Contra**: más código upfront; los steps son tipos, no funciones.
- **Decisión**: adopción. La consola de progreso (`OpConsole.svelte`)
  puede pintar exactamente el plan («[3/7] Importando dump…») sin
  parsers regex del `log()`.

### 7.5 Reconciliador con acción manual y automática

- **Pro**: detecta drift antes de que el usuario sufra.
- **Contra**: el reconciliador puede proponer acciones destructivas.
- **Decisión**: el reconciliador reporta. La **mayoría** de las acciones
  son automáticas (apagar compartido no usado, borrar vhost huérfano
  sin config). Las acciones que tocan datos del usuario (borrar
  container huérfano con config válida) requieren confirmación.

### 7.6 No introducir Prometheus/tracing-opentelemetry

- **Pro**: evita dependencias pesadas y un collector que mantener.
- **Contra**: menos visibilidad externa.
- **Decisión**: se mantiene el panel offline. Métricas operativas en
  JSONL rotado. Tracing estructurado en `tracing-subscriber` para
  archivos (no Live).

### 7.7 IA y deploy a VPS/complementos como backlog

- **Pro**: el rebuild se enfoca en lo que diferencia al panel
  (recursos, portabilidad, portabilidad de carpeta).
- **Contra**: hay que resistir la tentación de meter features.
- **Decisión**: separación dura. Las features IA y deploy están
  explícitamente fuera del alcance. `agent.rs` ni siquiera existe en
  el rebuild; se reintroduce en una fase posterior.

### 7.8 D-Bus y CLI con JSON strings

- **Pro**: `gdbus` y `wp-cli` funcionan sin tipo de Broker.
- **Contra**: la API queda implícita en strings.
- **Decisión**: se mantiene. La transformación a JSON-RPC formal es
  un nice-to-have posterior; el contrato real es el `dbus.rs` actual,
  la documentación del comando, y los tests de integración.

### 7.9 Persistencia basada en archivos, no SQLite

- **Pro**: una carpeta `~/panel-wp/{slug}/` es autosuficiente;
  depurable con `cat`.
- **Contra**: load_all escanea cada vez; sin índices.
- **Decisión**: se mantiene. El scan es O(N) sobre el `read_dir`
  (decenas de carpetas típico), aceptable. Si crece a miles, se
  introduce un índice lazy en `~/.config/wordpress-panel/index.json`
  con TTL, no SQLite.

---

## 8. Citas / fuentes actuales que motivan decisiones

Lista cerrada de archivos/líneas que muestran por qué una decisión del
rebuild es la correcta. Sirven como puente entre este documento y el
código que el rebuild hereda.

- **Aislamiento de contenedores por proyecto sin puertos host**:
  `src-tauri/src/docker.rs::create_php_container` (sin `port_bindings`
  en `HostConfig`); CLAUDE.md «Container por proyecto NO publica puertos
  al host».
- **DB datadir bindeado al host**:
  `src-tauri/src/docker.rs::db_data_dir` + `DbType::datadir` + el módulo
  `migrate_db_to_volume` (la migración una-sola-vez de containers sin
  bind legados via `docker cp`).
- **Auto-dump como defense in depth**:
  `src-tauri/src/autodump.rs::watch` (polling 20 s, gate por
  `Innodb_rows_*`, dedup por hash), enganchado en
  `lib.rs::start_site`/`setup`, abortado en `lib.rs::stop_site`.
- **Export al detener**:
  `src-tauri/src/docker.rs::stop_site` (`backup::export_db` + `rotate_dumps`).
- **Dump log para revisión**:
  `src-tauri/src/dumplog.rs` (JSONL en `config_dir/dump-log.jsonl`), UI
  en `/dumps`.
- **Worktrees por montajes (no copias)**:
  `src-tauri/src/docker.rs::create_php_container` (rama `if let Some(wt)
  = &site.worktree_of` con binds en orden de profundidad); nginx
  `alias /srv/projects/{dirname}/wt/{basename}/$1` en
  `src-tauri/src/nginx.rs::render_vhost`.
- **Snapshots como tar + dump**:
  `src-tauri/src/snapshot.rs::run` (tar con `--exclude` controlados +
  dump vía `backup::export_db_to`).
- **Clones temporales**:
  `src-tauri/src/clone.rs` (comparte motor DB/nginx del padre; uploads
  viejos vía fallback nginx).
- **Migración entre sistemas**:
  `src-tauri/src/migrate.rs` (auto-dump + export al detener + carpeta
  portable + `migrationPending` importable).
- **Snapshots con `tar` exit 1 no fatal**:
  `src-tauri/src/snapshot.rs::run` (código 1 = avisos no fatales;
  típico «file changed as we read it» en un WP activo).
- **Import dump con `docker exec -i … mysql` (no bollard)**:
  `src-tauri/src/migrate.rs::import_dump` (comentario explícito del
  por qué: el `exec` con stdin de bollard se cuelga con dumps grandes).
- **Watchdog del import por tamaño real de DB, no stdin**:
  `src-tauri/src/migrate.rs::import_dump` (midiendo
  `information_schema` por sonda periódica).
- **panel-nginx zombie tras apagón sucio**:
  `src-tauri/src/docker.rs::reload_nginx` (recreate si el exec falla).
- **Detección de puertos del host sin netstat**:
  `src-tauri/src/netcheck.rs` (`/proc/net/tcp{,6}`, distinción de
  `Free` / `Wildcard` / `Specific(IPs)`).
- **Endpoint del panel con auto-selección**:
  `src-tauri/src/docker.rs::autoselect_endpoint` (cede 80/443 a LocalWP
  y elige puerto alto).
- **Dnsmasq wildcard `*.test`**:
  `src-tauri/src/domain.rs` (`wildcard_rule`, `resolves_to`,
  `install_wildcard` vía `pkexec`).
- **Capability para `core:event`**:
  `src-tauri/capabilities/default.json` («Si un listener (p. ej. OpConsole
  con op-log) sale vacío, revísala primero»).
- **Código de barras: tipos serde en camelCase, espejo TS**:
  `CLAUDE.md` «Modelos serde en camelCase»; `src/lib/types.ts` espejo
  de `src-tauri/src/config.rs`.
- **Scripts `wp`/`wordpress-panel-cli` en `~/.local/bin`**:
  `src-tauri/src/cli.rs::install_cli_wrapper` (idempotente, en el
  `setup()` de Tauri).
- **MCP envoltorio sobre el CLI**:
  `mcp/server.mjs` (Node sin deps, protocolo MCP por stdio, delega a
  `wordpress-panel-cli` por D-Bus).
- **D-Bus para el plasmoid KDE**:
  `src-tauri/src/dbus.rs` + `plasma/applets/wordpress-panel-plasmoid/`.
- **Recarga reactiva de la UI en mutaciones CLI/MCP**:
  `src-tauri/src/dbus.rs::notify_sites_changed` + `+page.svelte`
  suscribe `sites-changed`.

---

## 9. Próximo paso

El capítulo 02 (Arquitectura objetivo) desarrolla en detalle la separación
`UI → application → domain → ports → adapters`, los límites de cada
módulo, los schemas versionados, la composición de operaciones, los
ports de Tauri/D-Bus/CLI/MCP/host, los platform-ports, y los criterios
para mantener la deuda documentada en §4 fuera del código nuevo.
