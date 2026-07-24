# Matriz de trazabilidad

> **Estado:** `CURRENT-CONFIRMED` para implementación; columnas `Target` apuntan al rebuild.  
> **Objetivo:** enlazar capacidad → superficie → núcleo → estado → efecto → prueba → operación → decisión futura.

## Convenciones

- `—`: superficie no expuesta o no aplica.
- `Mock`: test UI con IPC simulado; no prueba Tauri/Docker.
- `Int`: test Rust `#[ignore]` o validación Docker.
- Catálogos exactos evitan repetir firmas: [IPC](../referencia/03-catalogo-ipc-y-eventos.md), [D-Bus/CLI/MCP](../referencia/04-catalogo-dbus-cli-y-mcp.md).

## Lifecycle y WordPress

| Capacidad | UI / IPC | Núcleo actual | Estado/artefacto | Efecto externo | Interfaces externas | Prueba actual | Runbook | Target rebuild |
|---|---|---|---|---|---|---|---|---|
| Listar proyectos | `/`, `get_sites` | `config::load_all_sites`, `docker::site_status` | `config.json` + Docker | Lee FS/socket Docker | D-Bus, CLI, MCP, plasmoid parcial | Mock dashboard | [Proyectos](../operacion/03-runbook-proyectos.md) | `ProjectRepository` + `RuntimeStatusPort` |
| Crear proyecto | `/site/new`, `create_site` | `wordpress::create_site` | carpeta, config, schema, cert | HTTP wp.org, Docker, mkcert | — | Mock + Int | [Proyectos](../operacion/03-runbook-proyectos.md) | `CreateProject` journaled |
| Encender | acción primaria, `start_site` | `DockerManager::start_site` | containers/vhost | Docker, DNS | D-Bus, CLI, MCP | Mock/Int indirecta | [Proyectos](../operacion/03-runbook-proyectos.md) | desired-state reconciler |
| Detener | acción primaria, `stop_site` | `DockerManager::stop_site` | dump, containers, vhost | Docker/FS | D-Bus, CLI, MCP, plasmoid | Mock | [Proyectos](../operacion/03-runbook-proyectos.md) | `StopProject` + compensation |
| Detener todos | UI, `stop_all_sites` | loop de comandos + autodump | todos sitios | Docker/FS | D-Bus, CLI, plasmoid | parcial | [Proyectos](../operacion/03-runbook-proyectos.md) | batch operation con progreso |
| Borrar/desconectar | modal, `delete_site` | `delete_site_impl` | schema, carpeta/sidecar | Docker/FS | — | Mock e2e | [Proyectos](../operacion/03-runbook-proyectos.md) | dry-run + plan destructivo |
| Versiones WP | form, `list_wp_versions` | `wordpress::fetch_versions` | cache JSON | wordpress.org | — | unit parcial | [Proyectos](../operacion/03-runbook-proyectos.md) | `WordPressReleaseProvider` |
| WP-CLI | UI/terminal, `exec_wpcli` | `wpcli::run` | código/DB WP | Docker exec | wrapper `wp` | Int indirecta | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | typed `WpCliPort` + timeout |
| Auto-login | selector, `open_admin` | `autologin::open_admin` | transient WP, mu-plugin | browser + WP-CLI | D-Bus, CLI, MCP default | mock parcial/live histórico | [Proyectos](../operacion/03-runbook-proyectos.md) | capability aislada + auditoría |
| Upload PHP | Info, `set_php_upload_limit` | config/php helper | config + php.ini | signal php-fpm | D-Bus, CLI, MCP | serde unit | [Proyectos](../operacion/03-runbook-proyectos.md) | typed config policy |

## Infraestructura y datos

| Capacidad | UI / IPC | Núcleo actual | Estado/artefacto | Efecto externo | Interfaces externas | Prueba actual | Runbook | Target rebuild |
|---|---|---|---|---|---|---|---|---|
| Endpoint | Settings, `panel_endpoint/reset_endpoint` | `docker::select_endpoint`, `netcheck` | `panel.json` | puertos host | — | unit | [Instalación](../operacion/01-instalacion-y-primera-ejecucion.md) | versioned global settings |
| DNS wildcard | Settings/first-run | `domain` + script | snippet dnsmasq | `pkexec`, NetworkManager | — | unit helper/live histórico | [Instalación](../operacion/01-instalacion-y-primera-ejecucion.md) | `DnsProvider` platform adapter |
| SSL | crear/regenerar | `ssl::generate` | cert/key | mkcert, nginx reload | — | sin integración | [Diagnóstico](../operacion/07-diagnostico-y-mantenimiento.md) | `CertificateProvider` |
| DB compartida durable | implícito | `DockerManager::ensure_db` | host datadir | Docker/container DB | visible CLI/MCP | Int lifecycle | [Recuperación](../operacion/04-runbook-importacion-migracion-y-recuperacion.md) | resource reconciler + storage policy |
| Migración dump | consola, `migrate_site` | `migrate::run_migration/import_dump` | schema, config, cert | docker CLI + Docker API | — | Int e2e | [Migración](../operacion/04-runbook-importacion-migracion-y-recuperacion.md) | resumable operation journal |
| Dump manual | Servicios, `export_db` | `backup::export_db` | `app/sql`, JSONL | DB exec/FS | — | Int | [Recuperación](../operacion/04-runbook-importacion-migracion-y-recuperacion.md) | streaming `BackupPort` |
| Dump al detener | implícito | `docker::stop_site` | SQL + JSONL | DB exec/FS | todas vía stop | indirecta | [Proyectos](../operacion/03-runbook-proyectos.md) | resultado explícito, no best-effort oculto |
| Auto-dump | Settings muestra log | `AutoDump`, `backup`, `dumplog` | SQL + JSONL | task Tokio/DB | — | sin integración | [Diagnóstico](../operacion/07-diagnostico-y-mantenimiento.md) | scheduler + retention policy |
| Mailpit | Servicios/open | `ensure_mailpit` | container | Docker/browser | logs CLI/MCP | sin e2e | [Proyectos](../operacion/03-runbook-proyectos.md) | shared resource policy |
| MinIO | toggle/open | `ensure_minio` | flag + data dir | Docker/browser | logs CLI/MCP | sin e2e | [Proyectos](../operacion/03-runbook-proyectos.md) | optional capability adapter |
| Adminer | abrir DB | `ensure_adminer/open_adminer` | container/sesión | Docker/browser | visible containers | live histórico | [Proyectos](../operacion/03-runbook-proyectos.md) | DB admin adapter, secrets aislados |
| Logs live | tab, `stream_logs` | `logs::spawn_stream` | `LogStreams` memoria | Docker stream/evento | CLI/MCP consulta | Mock no runtime | [Diagnóstico](../operacion/07-diagnostico-y-mantenimiento.md) | structured event bus |
| Progreso largo | `OpConsole`, `op-log` | `progress::log` | canal global | Tauri event | indirecto | Mock | runbooks varios | `operationId` + typed progress |

## Importación, Git y entornos derivados

| Capacidad | UI / IPC | Núcleo actual | Estado/artefacto | Efecto externo | Interfaces externas | Prueba actual | Runbook | Target rebuild |
|---|---|---|---|---|---|---|---|---|
| Import LocalWP | Settings, `import_localwp_site` | `localwp::import_site` | copia + config pending | lectura LocalWP/FS | — | Int hermética + Mock | [Migración](../operacion/04-runbook-importacion-migracion-y-recuperacion.md) | importer adapter + preview |
| Import disconnected | modal, `import_disconnected_site` | `config` + `import_disconnected` | config restaurada/reconstruida | FS | — | Int hermética + Mock | [Migración](../operacion/04-runbook-importacion-migracion-y-recuperacion.md) | schema-aware discovery |
| Git scan/register/clone | Git tab | `github` | checkout + config repos | git/gh host | scan CLI/MCP | parcial | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | `GitProvider`, validated paths |
| Pull/status | Git tab | `github::pull/branch_status` | working tree | git network/host | D-Bus, CLI, MCP | sin integración | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | typed repository use cases |
| Deploy directo local | Git tab | `github::deploy` | checkout/build outputs | git + login shell | D-Bus, CLI, MCP | sin integración | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | policy/approval sandbox; no mezclar VPS |
| VS Code | botón, `open_vscode` | `github::ensure_workspace/open_vscode` | `.code-workspace` | editor host | — | sin test | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | `EditorAdapter` |
| Terminal | botón, `open_terminal` | `cli::open_terminal_at` | proceso host | terminal host | — | sin test | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | `TerminalAdapter` |
| Snapshot | tab, create/list/delete | `snapshot` | tar.zst, SQL, meta | tar/DB/FS | D-Bus, CLI, MCP | sin Int | [Derivados](../operacion/05-runbook-snapshots-clones-y-worktrees.md) | manifest/checksum/retention |
| Clone | tab, `create_clone` | `clone::create_clone` | sitio/schema derivado | Docker/FS/DB | D-Bus, CLI, MCP | sin Int | [Derivados](../operacion/05-runbook-snapshots-clones-y-worktrees.md) | explicit derived-project aggregate |
| Worktree shared DB | Git tab | `worktree::create_worktree` | git WT, config, wp-config | git/Docker/nginx | D-Bus, CLI, MCP | unit helpers | [Derivados](../operacion/05-runbook-snapshots-clones-y-worktrees.md) | mount plan + DB safety policy |
| Worktree copied DB | Git tab | mismo + import | anterior + schema propio | git/Docker/DB | D-Bus, CLI, MCP | unit helpers | [Derivados](../operacion/05-runbook-snapshots-clones-y-worktrees.md) | journal/compensation complete |

## UI, grupos e interfaces

| Capacidad | UI / IPC | Núcleo actual | Estado/artefacto | Efecto externo | Interfaces externas | Prueba actual | Runbook | Target rebuild |
|---|---|---|---|---|---|---|---|---|
| Master-detail | `/`, `ProjectDetail` | estado Svelte | selectedId/localStorage | — | — | Mock e2e/a11y | — | feature components + stores typed |
| Grupos | lista/DnD | `groups` | `groups.json`, `config.group` | FS | — | Mock parcial | [Proyectos](../operacion/03-runbook-proyectos.md) | repository transaction multi-file |
| System status | Settings | `system::status` | lectura derivada | Docker/host commands | — | Mock | [Diagnóstico](../operacion/07-diagnostico-y-mantenimiento.md) | typed health checks |
| CLI | — | shell→D-Bus | sin estado propio | gdbus/qdbus/docker | CLI | manual | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | generated client/manifest |
| MCP | — | Node→CLI→D-Bus | sin estado propio | stdio/process | MCP | prueba manual | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | generated tool schemas |
| Plasmoid | widget | D-Bus | polling 3 s | qdbus6/QML | plasmoid | visual pendiente | [Instalación](../operacion/01-instalacion-y-primera-ejecucion.md) | event-driven platform adapter |
| UI reactiva externa | listener | `sites-changed` | evento sin payload | Tauri core:event | D-Bus mutators | sin Tauri e2e | [Git/CLI](../operacion/06-runbook-git-cli-y-mcp.md) | domain events versionados |

## Funciones no trazadas como actuales

| Tema | Estado | Motivo | Destino |
|---|---|---|---|
| Asistente IA interno | Deferred | `agent.rs` no existe | Backlog posterior a núcleo estable |
| Container frontend headless | Deferred | solo flags | Diseñar como capability separada |
| Plugin S3 WP↔MinIO | Deferred | MinIO existe sin integración | Plugin/adaptador opcional |
| Cloudflare/package | Stub | `feature_stub` | Revalidar necesidad |
| Deploy VPS/cPanel/Bitnami | Idea | solo `ideas-cambios.md` | Producto remoto separado/adapter futuro |
| Restore snapshot in-place | No implementado | flujo actual crea clone | Definir semántica y rollback antes de añadir |

## Regla de aceptación

Cada fila seleccionada para rebuild necesita:

1. requisito versionado;
2. caso de uso;
3. contrato de entrada/salida/error;
4. dueño de estado;
5. adapter externo;
6. test automático;
7. runbook;
8. rollback/compensación si muta datos;
9. evidencia en [prueba de aceptación](02-prueba-de-aceptacion-de-la-reconstruccion.md).

## Fuentes primarias

- `src-tauri/src/lib.rs::run`
- `src-tauri/src/config.rs`
- `src-tauri/src/docker.rs`
- `src-tauri/src/dbus.rs`
- `src/lib/api.ts`
- `scripts/wordpress-panel-cli.sh`
- `mcp/server.mjs`
- `docs/TESTING.md`
