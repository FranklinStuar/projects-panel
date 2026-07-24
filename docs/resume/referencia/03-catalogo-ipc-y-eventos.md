# 03 · Catálogo IPC (Tauri) y eventos

> Referencia verificada contra el commit `373841c` (rama `main`, 2026-07-23).
> Cada comando se contrasta triple: `#[tauri::command]` en
> `src-tauri/src/lib.rs`, el llamante en `src/lib/api.ts` y el uso UI
> correspondiente. Los eventos se verifican contra `logs.rs`, `progress.rs`
> y `dbus.rs`.

## 1. Capability y ACL

`src-tauri/capabilities/default.json` declara:

```json
{
  "permissions": ["core:default", "core:event:default"]
}
```

- `core:default` — permisos por defecto de Tauri (necesario para la ventana).
- `core:event:default` — **necesario** para que `listen()` funcione fuera del proceso
  Rust. Sin esto, `OpConsole` (`src/lib/components/OpConsole.svelte:42-…`) sale
  vacío aunque el backend emita. Los `#[tauri::command]` propios NO pasan por
  el ACL (Tauri 2), por eso funcionan sin permiso.

## 2. Convención

- Todos los comandos viven en `src-tauri/src/lib.rs` y se registran en el
  `tauri::generate_handler!` (líneas 1011-1077).
- Alias `CmdResult<T> = Result<T, String>` (`lib.rs:43`).
- Errores mapeados vía `e<E: Display>` (`lib.rs:45-47`).
- Inputs en el wire = `camelCase` (Tauri los convierte desde el JSON
  `snake_case` del frontend automáticamente), pero el frontend **usa camelCase**
  en sus `args` (`api.ts`).
- Helpers reutilizables: `lib::load_site(id)`, `wpcli_json`, `wpcli::run`.

## 3. Catálogo exhaustivo de `#[tauri::command]`

> "Frontend" = el nombre camelCase tal como se usa en `api.ts`.
> "Backend" = el símbolo en `lib.rs`.
> "Tipo devuelto" = el primer parámetro genérico de `invoke<T>`.

| #  | Comando (lib.rs)                       | Frontend (api.ts)              | Args (frontend → backend)                                            | Tipo devuelto                | Notas                                                                                         |
| -- | -------------------------------------- | ------------------------------ | -------------------------------------------------------------------- | ---------------------------- | --------------------------------------------------------------------------------------------- |
| 1  | `get_sites`                            | `api.getSites()`               | —                                                                    | `SiteState[]`                | Escanea `~/panel-wp/*/config.json` y mide estado vía docker.                                  |
| 2  | `start_site`                           | `api.startSite(id)`            | `id: string`                                                         | `void`                       | Enciende `wp-{id}` + DB + nginx + mailpit [+ minio]. Engancha `AutoDump`.                     |
| 3  | `stop_site`                            | `api.stopSite(id)`             | `id: string`                                                         | `void`                       | `autodump.stop` + `docker.stop_site` (export-al-detener + teardown).                          |
| 4  | `stop_all_sites`                       | `api.stopAllSites()`           | —                                                                    | `void`                       | Itera todos y apaga.                                                                          |
| 5  | `exec_wpcli`                           | `api.execWpcli(id, args)`      | `id: string`, `args: string[]`                                       | `string`                     | `wp <args>` como `www-data`. Timeout 120 s.                                                  |
| 6  | `create_site`                          | `api.createSite(req)`          | `req: NewSiteRequest`                                                | `SiteConfig`                 | Crea estructura, DB, mu-plugins, instala WP, enciende.                                         |
| 7  | `list_wp_versions`                     | `api.listWpVersions()`         | —                                                                    | `WpVersion[]`                | Cache 24 h.                                                                                  |
| 8  | `panel_endpoint`                       | `api.panelEndpoint()`          | —                                                                    | `Endpoint`                   | `config::endpoint_or_default()`.                                                              |
| 9  | `system_status`                        | `api.systemStatus()`           | —                                                                    | `SystemStatus`               | best-effort.                                                                                  |
| 10 | `create_panel_network`                 | `api.createPanelNetwork()`     | —                                                                    | `void`                       | Idempotente.                                                                                  |
| 11 | `reset_endpoint`                       | `api.resetEndpoint()`          | —                                                                    | `void`                       | Olvida `Endpoint` persistido.                                                                 |
| 12 | `migrate_site`                         | `api.migrateSite(id)`          | `id: string`                                                         | `Migration`                  | DB + dump + SSL + start. Emite `op-log`.                                                     |
| 13 | `delete_site`                          | `api.deleteSite(id, del)`      | `id: string`, `deleteFolder: boolean`                                | `void`                       | Stop + drop DB + (rename sidecar / `remove_dir_all`). Emite `op-log`.                        |
| 14 | `list_localwp_sites`                   | `api.listLocalwpSites()`       | —                                                                    | `LocalSite[]`                | Lee `~/.config/Local/sites.json`.                                                              |
| 15 | `import_localwp_site`                  | `api.importLocalwpSite(id)`    | `id: string`                                                         | `ImportResult`               | Deja `migrationPending`. Emite `op-log`.                                                     |
| 16 | `list_disconnected_sites`              | `api.listDisconnectedSites()`  | —                                                                    | `DisconnectedSite[]`         | Escanea `~/panel-wp/` (sin `config.json`).                                                    |
| 17 | `import_disconnected_site`             | `api.importDisconnectedSite(folderName)` | `folderName: string`                                       | `ImportResult`               | Restaura sidecar o reconstruye best-effort.                                                   |
| 18 | `open_admin`                           | `api.openAdmin(id, userId?)`   | `id: string`, `userId?: number`                                      | `void`                       | Auto-login si `oneClickAdmin`.                                                               |
| 19 | `list_wp_users`                        | `api.listWpUsers(id)`          | `id: string`                                                         | `WpUser[]`                   | `wp user list --format=json`.                                                                |
| 20 | `repair_autologin`                     | `api.repairAutologin(id)`      | `id: string`                                                         | `SiteConfig`                 | Reinyecta mu-plugins. Para imports de LocalWP.                                                |
| 21 | `repair_all_php_ini`                   | `api.repairAllPhpIni()`        | —                                                                    | `string`                     | Mensaje: `php.ini actualizado en X/Y`. Devuelve aunque haya errores (string).               |
| 22 | `set_php_upload_limit`                 | `api.setPhpUploadLimit(id, mb)` | `id: string`, `mb: number`                                          | `SiteConfig`                 | Ajusta `upload_max_filesize` + `post_max_size`. Recarga php-fpm en caliente.                  |
| 23 | `open_site`                            | `api.openSite(id)`             | `id: string`                                                         | `void`                       | Abre `{endpoint.site_url(domain, ssl)}`.                                                       |
| 23 | `open_folder`                          | `api.openFolder(id)`           | `id: string`                                                         | `void`                       | `tauri-plugin-opener.opener.open_path`.                                                       |
| 24 | `open_terminal`                        | `api.openTerminal(id)`         | `id: string`                                                         | `void`                       | Instala wrappers (`cli::install_cli_wrapper`) y abre konsole/gnome-terminal/etc.               |
| 25 | `stream_logs`                          | `api.streamLogs(id)`           | `id: string`                                                         | `void`                       | Inicia task de streaming → emite `log:{id}`.                                                   |
| 26 | `stop_logs`                            | `api.stopLogs(id)`             | `id: string`                                                         | `void`                       | Aborta la task.                                                                               |
| 27 | `list_plugins`                         | `api.listPlugins(id)`          | `id: string`                                                         | `string`                     | `wp plugin list --format=json`.                                                              |
| 28 | `list_themes`                          | `api.listThemes(id)`           | `id: string`                                                         | `string`                     | `wp theme list --format=json`.                                                               |
| 29 | `gh_status`                            | `api.ghStatus()`               | —                                                                    | `GhStatus`                   | Lee `gh --version` y `gh auth status`.                                                     |
| 30 | `gh_clone`                             | `api.ghClone(id, kind, repo, branch, path?)` | `id: string`, `kind: 'theme'\|'plugin'\|'muplugin'`, `repo: string`, `branch: string`, `path?: string` | `SiteConfig`                 | Clona un repo (lo registra en `github.repos`).                                                  |
| 31 | `gh_pull`                              | `api.ghPull(id, path, branch)` | `id: string`, `path: string`, `branch: string`                       | `string`                     | `git pull` directo a la carpeta.                                                              |
| 32 | `gh_pull_all`                          | `api.ghPullAll(id)`            | `id: string`                                                         | `string`                     | Itera `site.github.repos`.                                                                    |
| 33 | `gh_remove`                            | `api.ghRemove(id, path)`       | `id: string`, `path: string`                                         | `SiteConfig`                 | Borra carpeta + desregistra.                                                                  |
| 34 | `gh_scan`                              | `api.ghScan(id)`               | `id: string`                                                         | `DetectedRepo[]`             | Recorre `wp-content` buscando `.git`.                                                        |
| 35 | `gh_register`                          | `api.ghRegister(id, path)`     | `id: string`, `path: string`                                         | `SiteConfig`                 | Lee `origin` y rama de un repo huérfano.                                                     |
| 36 | `gh_branch_status`                     | `api.ghBranchStatus(id, path, branch)` | `id: string`, `path: string`, `branch: string`               | `BranchStatus`               | `git fetch` + rev-list + status.                                                             |
| 37 | `gh_set_deploy`                        | `api.ghSetDeploy(id, path, branch, buildCmd, buildDirs)` | `id: string`, `path: string`, `branch: string`, `buildCmd: string \| null`, `buildDirs: string[]` | `SiteConfig`                 | Guarda config de deploy.                                                                     |
| 38 | `gh_build_dirs`                        | `api.ghBuildDirs(id, path)`    | `id: string`, `path: string`                                         | `string[]`                   | Candidatos de build_dirs.                                                                     |
| 39 | `gh_deploy`                            | `api.ghDeploy(id, path)`       | `id: string`, `path: string`                                         | `void`                       | Checkout + pull + build. Emite `op-log`.                                                     |
| 40 | `open_vscode`                          | `api.openVscode(id)`           | `id: string`                                                         | `void`                       | Genera `.code-workspace` y abre `code`/`codium`/...                                          |
| 41 | `regenerate_ssl`                       | `api.regenerateSsl(id)`        | `id: string`                                                         | `void`                       | `mkcert` + `reload_nginx`.                                                                   |
| 42 | `repair_nginx`                         | `api.repairNginx()`            | —                                                                    | `string`                     | Poda vhosts huérfanos y recrea `panel-nginx` si está zombie.                                  |
| 43 | `set_site_group`                       | `api.setSiteGroup(id, group)`  | `id: string`, `group: string \| null`                                | `SiteConfig`                 | Asigna (vacío = null). Asegura que el grupo esté en `groups.json`.                            |
| 43 | `list_groups`                          | `api.listGroups()`             | —                                                                    | `string[]`                   | `groups.json::order`.                                                                        |
| 44 | `create_group`                         | `api.createGroup(name)`        | `name: string`                                                       | `void`                       | Idempotente.                                                                                  |
| 45 | `rename_group`                         | `api.renameGroup(old, new)`    | `old: string`, `new: string`                                         | `void`                       | Reescribe `groups.json` + reasigna `site.group`.                                              |
| 46 | `delete_group`                         | `api.deleteGroup(name)`        | `name: string`                                                       | `void`                       | Saca del `order` y resetea `site.group = null`.                                              |
| 47 | `reorder_groups`                       | `api.reorderGroups(order)`     | `order: string[]`                                                    | `void`                       | Sobrescribe el orden en `groups.json`.                                                       |
| 48 | `set_site_minio`                       | `api.setSiteMinio(id, enabled)` | `id: string`, `enabled: boolean`                                    | `SiteConfig`                 | Persiste `site.minio`; si activo y encendido, `ensure_minio`.                                 |
| 49 | `export_db`                            | `api.exportDb(id)`             | `id: string`                                                         | `string`                     | `db-{stamp}.sql` + `dumplog::append(.., "manual")`.                                          |
| 50 | `dump_log`                             | `api.dumpLog()`                | —                                                                    | `DumpLogEntry[]`             | Más nuevas primero.                                                                          |
| 51 | `clean_dump_log`                       | `api.cleanDumpLog(before, db)` | `before: string \| null`, `dbName: string \| null`                    | `number`                     | N entradas borradas. Solo el log, no toca `.sql`.                                            |
| 52 | `install_cli_wrapper`                  | `api.installCliWrapper()`      | —                                                                    | `string`                     | Mensaje de instalación. Idempotente.                                                          |
| 53 | `open_mailpit`                         | `api.openMailpit()`            | —                                                                    | `void`                       | `http://127.0.0.1:8025/`.                                                                    |
| 54 | `open_minio`                           | `api.openMinio()`              | —                                                                    | `void`                       | `http://127.0.0.1:9101/`.                                                                    |
| 55 | `open_adminer`                         | `api.openAdminer(id)`          | `id: string`                                                         | `void`                       | `http://127.0.0.1:8088/?{driver}=…&username=…&db=…`.                                       |
| 56 | `feature_stub`                         | `api.featureStub(feature)`     | `feature: string`                                                    | `string`                     | **Devuelve error** (preparado para `cloudflare`/`deploy`/`package`).                          |
| 57 | `create_snapshot`                      | `api.createSnapshot(id, label)` | `id: string`, `label: string`                                       | `SnapshotMeta`               | tar + mysqldump. Emite `op-log`.                                                              |
| 58 | `list_snapshots`                       | `api.listSnapshots(id)`        | `id: string`                                                         | `SnapshotMeta[]`             | Lee `snapshots/*/meta.json`.                                                                 |
| 59 | `delete_snapshot`                      | `api.deleteSnapshot(id, snapshotId)` | `id: string`, `snapshotId: string`                              | `void`                       | `remove_dir_all` del snapshot.                                                                |
| 60 | `detect_excludable`                    | `api.detectExcludable(id)`     | `id: string`                                                         | `ExcludableEntry[]`          | Carpetas candidatas a excluir.                                                                |
| 61 | `set_snapshot_excludes`                | `api.setSnapshotExcludes(id, excludes)` | `id: string`, `excludes: string[]`                       | `void`                       | Persiste `site.snapshotExcludes`.                                                            |
| 62 | `create_clone`                         | `api.createClone(parentId, snapshotId)` | `parentId: string`, `snapshotId: string`                     | `SiteConfig`                 | Extrae tar, importa dump, SSL, enciende. Emite `op-log`.                                    |
| 63 | `create_worktree_site`                 | `api.createWorktreeSite(parentId, targetPath, branch, sharedDb, baseBranch?)` | `parentId: string`, `targetPath: string`, `branch: string`, `sharedDb: boolean`, `baseBranch?: string` | `SiteConfig`                 | `git worktree add` + (DB copia / DB compartida). Emite `op-log`.                             |
| 64 | `remove_worktree_site`                 | `api.removeWorktreeSite(id, deleteBranch)` | `id: string`, `deleteBranch: boolean`                       | `void`                       | `stop_site` + `git worktree remove` + drop DB (si copia) + `remove_dir_all`. Emite `op-log`. |
| 65 | `list_worktrees`                       | `api.listWorktrees(parentId)`  | `parentId: string`                                                   | `SiteConfig[]`               | Filtra `worktree_of.parent_id == parentId`.                                                  |

> Hay comandos definidos en `lib.rs` que **no aparecen en `api.ts`** (esto es
> deuda intencional: están pensados para invocarse desde el CLI/D-Bus, no
> desde la UI). Ver §11.

## 4. Comandos **internos** (no en `generate_handler!`)

| Símbolo                                        | Ubicación               | Notas                                                                                |
| ---------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------ |
| `autodump::AutoDump::start`                    | `autodump.rs:36-44`     | Llamado desde `start_site` y `dbus::Manager::start_site`.                            |
| `autodump::AutoDump::stop`                     | `autodump.rs:47-51`     | Desde `stop_site`, `stop_all_sites` y `dbus::Manager::stop_site/stop_all`.           |
| `logs::spawn_stream`                           | `logs.rs:20-47`         | Tauri `JoinHandle` guardado en `LogStreams` (estado).                                |
| `progress::log` / `progress::log_progress`     | `progress.rs:22-31`     | Emite `op-log`. Usado por `delete_site`, `migrate_site`, `localwp::import_site`, `clone::create_clone`, `worktree::create_worktree`, `worktree::remove_worktree`, `snapshot::create_snapshot`, `github::deploy`. |
| `dbus::notify_sites_changed`                   | `dbus.rs:32-34`         | Emite `sites-changed` desde operaciones del CLI/MCP.                                  |

## 5. Estado gestionado (`manage`)

| Estado    | Tipo                          | Donde se registra                              | Operaciones                                                       |
| --------- | ----------------------------- | ---------------------------------------------- | ----------------------------------------------------------------- |
| `LogStreams` | `Mutex<HashMap<String, JoinHandle<()>>>` | `lib.rs:51, 966`                               | `stream_logs` inserta, `stop_logs` aborta.                      |
| `AutoDump`   | `Mutex<HashMap<String, JoinHandle<()>>>` | `lib.rs:32 (struct), 967`                       | `start_site` inserta, `stop_site`/`stop_all_sites`/`dbus::stop_*` quitan. |

## 6. Eventos backend→frontend

| Evento (canal) | Origen (símbolo)        | Payload (raw)                                                                  | Suscriptor (frontend)                                                  | Notas                                                                                                       |
| -------------- | ----------------------- | ------------------------------------------------------------------------------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `op-log`       | `progress::log`         | `string` (línea de texto)                                                       | `src/lib/components/OpConsole.svelte:42` (`listen('op-log', …)`)        | `PROGRESS_PREFIX = '\u{1}'` (símbolo SOH) marca líneas "vivas" que se reescriben en sitio.                   |
| `op-log`       | `progress::log_progress`| `string` prefijo `\u{1}`                                                        | (mismo)                                                                | Para contadores live (import del dump).                                                                     |
| `op-log`       | `DeleteProjectModal.svelte:73-…` (frontend) | `string` (preparativos de borrado)                                | (mismo)                                                                | El modal emite preparativos antes de invocar `delete_site`.                                                  |
| `log:{id}`     | `logs::spawn_stream`    | `string` (línea de log de `wp-{id}`)                                            | `src/lib/components/ProjectDetail.svelte:338` (`listen(\`log:${id}\`, …)`) | `stream_logs` arranca la tarea; `stop_logs` la aborta. `tail: 200`, `follow: true`.                         |
| `sites-changed`| `dbus::notify_sites_changed` | `()` (objeto vacío)                                                          | `src/routes/+page.svelte:181` (`listen('sites-changed', …)`)            | Lo dispara el backend cuando el CLI/MCP muta el set de proyectos (start/stop/worktree/clone).              |

> Los eventos `app.emit(...)` requieren `core:event:default` en
> `capabilities/default.json`. Si se añade un evento nuevo y el listener ve
> vacío, revisar primero la capability.

## 7. `Manager` D-Bus — equivalencia con IPC

`dbus::Manager` (interface zbus `com.goldmediatech.WordpressPanel.Manager`)
ofrece una versión **serializada a JSON** de la mayoría de operaciones. Es la
fachada que consume el CLI shell y el servidor MCP. Tabla de correspondencia
(completada por inspección de `dbus.rs` y `lib.rs`):

| Método D-Bus                     | Equivalente IPC                 | Args D-Bus                                                   | Devolución (JSON)                              |
| -------------------------------- | ------------------------------- | ------------------------------------------------------------ | ---------------------------------------------- |
| `GetRunningSites`                | (no hay equivalente directo)    | —                                                            | `[{id,name,domain}]`                            |
| `ListSites`                      | `get_sites` (versión extendida) | —                                                            | `[{id,name,domain,group,running}]`              |
| `StopSite(id)`                   | `stop_site`                     | `id: string`                                                 | `bool`                                         |
| `StopAll`                        | `stop_all_sites`                | —                                                            | `bool`                                         |
| `StartSite(id)`                  | `start_site`                    | `id: string`                                                 | `bool`                                         |
| `OpenAdmin(id)`                  | `open_admin`                    | `id: string`                                                 | `string` JSON `{ok,error?}`/`{ok,url?}`         |
| `OpenSite(id)`                   | `open_site`                     | `id: string`                                                 | `string` JSON `{ok,url,error?}`                 |
| `ProjectContainers(id)`          | (no hay equivalente)            | `id: string`                                                 | `string` JSON `[{name,role,running}]`           |
| `Quit`                           | `app.exit(0)`                   | —                                                            | `void`                                         |
| `ListWorktrees(parentId)`        | `list_worktrees`                | `parentId: string`                                           | `string` JSON `[{id,name,domain,branch,targetPath,sharedDb}]` |
| `CreateWorktree(parentId, target, branch, base, sharedDb)` | `create_worktree_site` | `parentId: string`, `targetPath: string`, `branch: string`, `baseBranch: string`, `sharedDb: bool` | `string` JSON `{ok,id,domain,error?}` |
| `RemoveWorktree(id, deleteBranch)` | `remove_worktree_site`       | `id: string`, `deleteBranch: bool`                            | `bool`                                         |
| `CreateSnapshot(id, label)`      | `create_snapshot`               | `id: string`, `label: string`                                | `string` JSON `{ok,snapshot,error?}`            |
| `ListSnapshots(id)`              | `list_snapshots`                | `id: string`                                                 | `string` JSON `SnapshotMeta[]`                  |
| `DeleteSnapshot(id, snapshotId)` | `delete_snapshot`               | `id: string`, `snapshotId: string`                           | `bool`                                         |
| `CreateClone(parentId, snapshotId)` | `create_clone`               | `parentId: string`, `snapshotId: string`                     | `string` JSON `{ok,id,domain,error?}`            |
| `GhScan(id)`                     | `gh_scan`                       | `id: string`                                                 | `string` JSON `DetectedRepo[]`                  |
| `GhPull(id, path, branch)`       | `gh_pull`                       | `id: string`, `path: string`, `branch: string`               | `string` JSON `{ok,output,error?}`              |
| `GhBranchStatus(id, path, branch)` | `gh_branch_status`           | `id: string`, `path: string`, `branch: string`               | `string` JSON `BranchStatus`/`{ok:false,error?}` |
| `GhBuildDirs(id, path)`          | `gh_build_dirs`                 | `id: string`, `path: string`                                 | `string` JSON `string[]`                        |
| `GhSetDeploy(id, path, branch, buildCmd, buildDirsCsv)` | `gh_set_deploy` | `id: string`, `path: string`, `branch: string`, `buildCmd: string`, `buildDirsCsv: string` (CSV) | `bool`                                  |
| `GhDeploy(id, path)`             | `gh_deploy`                     | `id: string`, `path: string`                                 | `string` JSON `{ok,error?}`                     |

## 8. Plugin `tauri-plugin-opener` y `shell`

- `app.opener().open_url(url, None::<&str>)` se usa en `open_admin`,
  `open_site`, `open_mailpit`, `open_minio`, `open_adminer`.
- `app.opener().open_path(path, None::<&str>)` en `open_folder`.
- `tauri-plugin-shell` está añadido en `lib.rs:964` por consistencia, pero el
  código no usa `app.shell()` directamente.

## 9. Modelo de errores

- `Result<T, String>` por convención.
- `e<E: Display>` convierte cualquier error en `String` (`lib.rs:45-47`).
- `wpcli::run` (`wpcli.rs:18`): timeout `WPCLI_TIMEOUT = 120s` para evitar
  wp-cli colgado por un plugin/mu-plugin que llame a la red al cargar.
- `docker::exec_as` (`docker.rs:901-945`): chequea `inspect_exec.exit_code` y
  propaga el error si `!= 0`. Esencial para no devolver `Ok` engañoso tras
  un `wp config create` fallido.
- `migrate::import_dump` (`migrate.rs:235-409`): watchdog a
  `IMPORT_IDLE_TIMEOUT = 180s` con indicador de vida basado en el tamaño real
  de la DB (no en el flujo de stdin), porque el pipe de OS (~64 KB) bloquea
  `write_all` aunque el import esté avanzando.

## 10. Helpers en `lib.rs` que **no son comandos**

| Símbolo             | Función                                                                                                     |
| ------------------- | ----------------------------------------------------------------------------------------------------------- |
| `helper::e`         | `e<E: Display>(err: E) -> String` (línea 45).                                                               |
| `helper::load_site` | `fn load_site(id: &str) -> CmdResult<SiteConfig>` (línea 799). Carga `find_site` o error.                    |
| `helper::wpcli_json`| `async fn wpcli_json(id: &str, args: &[&str]) -> CmdResult<String>` (línea 500). Ejecuta `wp` y devuelve el stdout. |

## 11. Paridad `lib.rs` ↔ `api.ts`

Verificación punto por punto: los **67 comandos** listados en §3 están todos
en `api.ts` (chequeo manual: `grep -n "'<cmd>'" api.ts`). Los
`autodump::AutoDump::start|stop` y `logs::spawn_stream` no son comandos
expuestos; se manejan como efectos secundarios dentro de `start_site` /
`stop_site` / `stream_logs`.

`Drop/feature_stub` no se invoca desde la UI (`api.featureStub` está exportado
pero ningún componente lo usa); queda como placeholder visible.

## 12. Divergencias notables

- `frontendFramework` (`SiteConfig`) se mantiene en el modelo y se setea en
  `NewSiteRequest`, pero **no se envía desde la UI** hoy (no hay campo en
  `ProjectDetail`/`new`).
- `featureStub` es un comando "stub" que devuelve `Err` — la UI no debería
  mostrar errores repetitivos por estos botones; ver `KNOWN_ISSUES.md`.
- `FeatureStub` se invoca desde `api.ts` (`api.featureStub`) pero no se
  consume en la UI; queda DEFERRED en la rama de Fase 5.
- `ProjectContainers` (D-Bus) lista roles `php`, `db`, `nginx`, `mailpit` y
  (opcional) `minio`. No incluye `adminer` porque su rol es "UI bajo
  demanda", no un container prendido por un proyecto. La CLI/MCP, sin
  embargo, no usa este dato directamente.

## 13. Estado de deuda / Diferido

- `feature_stub` (Fase 5): la UI nunca lo invoca.
- `clone_of` no aparece en `applyClone` automático: la UI exige al usuario
  prender el clon manualmente.
- `frontendFramework`/`headless`: soportados en el modelo, sin implementar
  en la imagen ni en la UI.
- `command::gh_*` — el deploy directo (staging) existe pero no hay un comando
  `feature_stub("deploy")` que abra un wizard; queda como botón en
  `ProjectDetail.svelte` con `github.deploy`.

## Fuentes primarias

- `src-tauri/src/lib.rs` (registro completo en `generate_handler!`).
- `src-tauri/src/capabilities/default.json` (capability Tauri).
- `src-tauri/src/dbus.rs` (interface zbus).
- `src-tauri/src/logs.rs` (`log:{id}`).
- `src-tauri/src/progress.rs` (`op-log`).
- `src-tauri/src/autodump.rs` (gestión de watchers).
- `src-tauri/src/wpcli.rs` (timeout).
- `src-tauri/src/docker.rs` (exec / exec_capture).
- `src-tauri/src/migrate.rs` (watchdog del import).
- `src/lib/api.ts`, `src/lib/types.ts`, `src/lib/components/OpConsole.svelte`,
  `src/lib/components/ProjectDetail.svelte`, `src/routes/+page.svelte`.
- `docs/CHANGELOG.md`, `docs/KNOWN_ISSUES.md`.
