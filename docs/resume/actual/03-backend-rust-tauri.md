# Backend Rust y Tauri

## Composición

`src-tauri/src/lib.rs` declara módulos, adapta errores a `Result<T, String>`, define comandos `#[tauri::command]` y compone Tauri. `run` instala plugins opener/shell, registra managed state, ejecuta setup y enumera comandos en `generate_handler!`.

```text
invoke(name,args)
      │
      ▼
lib.rs::comando ── carga SiteConfig ── módulo de caso de uso
      │                                    │
      ├─ DockerManager/bollard             ├─ filesystem
      ├─ app.emit                          └─ procesos host
      └─ Result<T,String>
```

Los handlers son adaptadores finos en muchos casos (`exec_wpcli`, snapshots, SSL), pero algunos flujos coordinadores viven en `lib.rs`, como borrado/desconexión e importación reconstruida. El catálogo exacto debe consultarse en `../referencia/*`; la regla para extender es registrar backend, wrapper `api.ts` y tipo espejo.

## Módulos y responsabilidades

- `config`: modelos y filesystem.
- `docker`: red, imágenes, containers, exec y teardown.
- `wordpress`/`wpcli`/`autologin`: instalación y operación WordPress.
- `nginx`/`domain`/`ssl`/`php`: publicación y runtime.
- `backup`/`autodump`/`dumplog`/`migrate`/`localwp`: movilidad y protección de datos.
- `snapshot`/`clone`/`worktree`: entornos derivados.
- `github`: repos, comparación, pull, build y workspace.
- `logs`/`progress`: streams backend→frontend.
- `dbus`/`cli`: superficies fuera de la GUI.

## Managed state y tareas asíncronas

`run` gestiona dos estados Tauri:

```text
LogStreams: Mutex<HashMap<siteId, JoinHandle>>
AutoDump:   Mutex<HashMap<siteId, JoinHandle>>
```

`LogStreams` evita streams duplicados y permite abortarlos con `stop_logs`. `AutoDump` mantiene un watcher por proyecto; `start` es idempotente y `stop` aborta el handle. `DockerManager` no es estado global: se reconecta por comando/tarea.

En `setup` se lanzan dos tareas largas:

1. Reconstruir watchers de auto-dump para containers PHP que ya estaban activos.
2. Servir D-Bus y mantener viva la conexión mediante `pending()`.

Además, `logs::spawn_stream` y `autodump::watch` son tareas Tokio. El watcher sondea cada 20 segundos, usa contador InnoDB como gate y compara hash del dump antes de escribir.

## Concurrencia y cancelación

Los mapas están protegidos por `std::sync::Mutex`, pero el lock se mantiene solo en operaciones breves, no a través de `.await`. La cancelación es cooperativa por `JoinHandle::abort`. La parada ordenada aborta AutoDump antes del dump final para evitar competencia conceptual.

No existe un coordinador por sitio que serialice todos los comandos: dos invocaciones concurrentes podrían competir sobre un mismo proyecto. Docker y varias operaciones son idempotentes, pero esta no es una garantía transaccional general.

## Integraciones de proceso

El backend usa bollard como vía normal. Excepciones verificables:

- `php::ensure_php_image`: `docker image inspect/build`.
- `migrate::import_dump`: `docker exec -i` por el bloqueo observado con stdin bollard.
- `DockerManager::migrate_db_to_volume`: `docker cp`.
- `ssl::generate`: `mkcert`.
- `github`: `gh`, `git` y shell de login para builds.
- `cli::open_terminal_at` y `github::open_vscode`: procesos detached del escritorio.

## Arranque y ventana

La app habilita opener y shell, usa decoraciones nativas y una única ventana principal. `GTK_CSD=0` **no** se configura: fue revertido. La posición de botones queda documentada como pendiente. La SPA se sirve desde `build/`; en desarrollo Vite corre en `localhost:1420`.

## Manejo de error y best-effort

El helper `e` conserva el texto de errores. Operaciones destructivas importantes suelen propagar error, mientras tareas accesorias como Mailpit/MinIO, reload tras stop, rotación o logging de dump se tratan best-effort. Esa asimetría es intencional, pero exige leer el símbolo concreto para saber qué garantía ofrece un comando.

## Deuda observable

`lib.rs` es un módulo grande con coordinación y adaptadores mezclados. No hay cierre explícito común para abortar todos los handles al salir. Varias rutas silencian errores con `.ok()`, lo que prioriza continuidad pero reduce diagnóstico. IA no está implementada: no existe `agent.rs` ni managed state de proveedor.

## Fuentes primarias

- `src-tauri/src/lib.rs::run`, `LogStreams`, `get_sites`, `delete_site`
- `src-tauri/src/autodump.rs::AutoDump`, `watch`
- `src-tauri/src/logs.rs::spawn_stream`
- `src-tauri/src/docker.rs::DockerManager`
- `src-tauri/src/php.rs::ensure_php_image`
- `src-tauri/src/github.rs::deploy`
- `src-tauri/tauri.conf.json::build`, `app.windows`
