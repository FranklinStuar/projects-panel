# IPC, eventos y estado

## Dos canales Tauri distintos

Los comandos request/response y los eventos push no comparten el mismo mecanismo de autorización.

```text
Svelte api.ts ── invoke("start_site", {id}) ──► #[tauri::command]
                                                       │
Svelte listen("op-log") ◄──── app.emit("op-log") ─────┘
```

Los comandos propios se registran en `lib::run::generate_handler!` y devuelven `Result<T,String>`. `src/lib/api.ts::api` fija nombres y argumentos; `src/lib/types.ts` fija el contrato de payloads camelCase.

## Eventos implementados

- `op-log`: progreso de migración, importación, snapshots, clones, worktrees y deploy Git.
- `log:{siteId}`: líneas stdout/stderr del container PHP, últimas 200 y follow.
- `sites-changed`: notificación de mutaciones realizadas por D-Bus (CLI/MCP/plasmoid) para refrescar la página raíz.

`progress::PROGRESS_PREFIX` es SOH (`U+0001`): `OpConsole` reemplaza la última línea marcada, evitando que contadores periódicos llenen el buffer. Las emisiones son best-effort.

## Capability de Tauri 2

`src-tauri/capabilities/default.json` aplica a la ventana `main` e incluye `core:default` y `core:event:default`. Esta capability es necesaria para `listen()`/eventos. Si falta, `invoke` puede seguir funcionando mientras consolas y logs quedan vacíos, porque los comandos propios no pasan por ese ACL.

## Managed state

```text
Tauri managed state
├─ LogStreams
│  └─ siteId → tarea bollard logs(follow)
└─ AutoDump
   └─ siteId → tarea de vigilancia DB
```

`LogStreams` vive en `lib.rs`; `stream_logs` evita duplicación y `stop_logs` extrae/aborta el handle. `AutoDump` vive en su módulo; se activa al arrancar un sitio, se recupera durante setup si el PHP ya estaba ejecutándose y se aborta antes de parar.

## Estado no gestionado

`DockerManager` se construye por operación y no contiene cache durable. `SiteConfig` se relee desde disco. El frontend conserva estado efímero local por componente. Por tanto:

- filesystem = configuración autoritativa;
- Docker = estado runtime autoritativo;
- managed state = ownership de tareas en memoria;
- localStorage = preferencias UI no críticas.

## Orden y carreras

Para logs, el frontend hace `listen` antes de `streamLogs`; para `op-log`, el listener se instala al montar `OpConsole`, antes de abrir operaciones. La parada de sitio aborta AutoDump antes del dump final. Estas decisiones evitan pérdidas/rivalidad comunes.

No hay bus de eventos de dominio completo. Las mutaciones GUI refrescan explícitamente; solo el camino D-Bus emite `sites-changed`. Tampoco existe versionado/revisión de entidades en IPC. Dos acciones simultáneas pueden observar y escribir configuraciones sin control optimista.

## D-Bus como segundo adaptador

`dbus::Manager` llama los mismos módulos Rust y, cuando muta sitios, `notify_sites_changed` emite hacia Tauri. D-Bus devuelve JSON string para estructuras complejas, o booleanos para acciones simples. Esto evita duplicar tipos D-Bus pero sacrifica tipado de interfaz.

## Límites

La capability permite eventos de la ventana principal, no constituye autenticación de comandos de negocio. D-Bus está en la sesión del usuario y no implementa autorización adicional. El catálogo completo de comandos/eventos debe enlazarse en `../referencia/*`, no repetirse aquí.

## Deuda observable

Falta correlación por operación en `op-log`: todas comparten canal, de modo que operaciones simultáneas podrían mezclar salida. Los handles de logs no se eliminan automáticamente del mapa cuando el stream termina por error. La UI limita buffers, pero backend emite línea a línea sin backpressure explícita.

## Fuentes primarias

- `src-tauri/src/lib.rs::CmdResult`, `LogStreams`, `stream_logs`, `run`
- `src-tauri/src/progress.rs::EVENT`, `log_progress`
- `src-tauri/src/logs.rs::event_name`, `spawn_stream`
- `src-tauri/src/autodump.rs::AutoDump`
- `src-tauri/src/dbus.rs::notify_sites_changed`
- `src/lib/components/OpConsole.svelte::onMount`
- `src/lib/components/ProjectDetail.svelte::startLogs`
- `src-tauri/capabilities/default.json::permissions`
