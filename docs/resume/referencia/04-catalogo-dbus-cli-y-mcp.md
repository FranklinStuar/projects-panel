# 04 · Catálogo D-Bus, CLI y MCP

> Referencia verificada contra el commit `373841c` (rama `main`, 2026-07-23).
> Une los tres puntos de entrada externos del panel (D-Bus para el plasmoid,
> CLI shell para el usuario, MCP para agentes IA) y los comandos equivalentes
> del backend Tauri. Cada método se contrasta contra `dbus.rs::Manager`,
> `scripts/wordpress-panel-cli.sh` y `mcp/server.mjs`.

## 1. Servicio D-Bus

Registrado por `dbus::serve` (`src-tauri/src/dbus.rs:416-428`):

| Parámetro                | Valor                                                          |
| ------------------------ | -------------------------------------------------------------- |
| `service` (bus name)     | `com.goldmediatech.WordpressPanel`                              |
| `path` (object path)     | `/com/goldmediatech/WordpressPanel`                             |
| `interface` (zbus)       | `com.goldmediatech.WordpressPanel.Manager`                      |
| `Builder::session()`     | sí (sesión de usuario, no sistema)                              |
| `feature` `tokio`        | el executor de zbus corre sobre el runtime Tokio del panel     |
| `Connection` mantenida   | `tauri::async_runtime::spawn(... std::future::pending().await)` |

Si la sesión D-Bus no está disponible, el panel **continúa funcionando** (no
falla). Solo el widget queda inactivo.

### 1.1 Catálogo de métodos D-Bus

| Método (`dbus.rs::Manager`) | Firma (entrada)                                                                  | Salida (eager)         | Equivalente IPC Tauri                          | Notas                                                                                                   |
| --------------------------- | -------------------------------------------------------------------------------- | ---------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `GetRunningSites`           | —                                                                                | `string` JSON          | (no directa — UI usa `get_sites`)              | Solo proyectos cuyo container `wp-{id}` está corriendo (vía `docker.is_running`).                       |
| `ListSites`                 | —                                                                                | `string` JSON          | `get_sites` (extendida con `group`, `running`) | Para que el CLI resuelva id↔nombre.                                                                    |
| `StopSite`                  | `id: string`                                                                     | `bool`                 | `stop_site`                                    | Detiene el watcher de auto-dump antes.                                                                  |
| `StopAll`                   | —                                                                                | `bool`                 | `stop_all_sites`                               | Itera y apaga.                                                                                          |
| `StartSite`                 | `id: string`                                                                     | `bool`                 | `start_site`                                   | Enciende y registra el watcher.                                                                          |
| `SetUploadLimit`            | `id: string`, `mb: string`                                                        | `string` JSON          | `set_php_upload_limit`                        | `{"ok": true}` o `{"ok": false, "error": "..."}`. Ajusta upload PHP y recarga en caliente.              |
| `OpenAdmin`                 | `id: string`                                                                     | `string` JSON          | `open_admin`                                   | `{"ok": true}` o `{"ok": false, "error": "..."}`.                                                       |
| `AdminUrl`                  | `id: string`, `user: string` (ID, `user_login` o vacío)                            | `string` JSON          | (no expuesta como IPC)                         | `{"ok": true, "url": ...}`. Devuelve la URL de auto-login sin abrir navegador (token 300 s, un solo uso).   |
| `OpenSite`                  | `id: string`                                                                     | `string` JSON          | `open_site`                                    | `{"ok": true, "url": ...}` o error.                                                                    |
| `ProjectContainers`         | `id: string`                                                                     | `string` JSON `[{name,role,running}]` | (no expuesta como IPC)              | Lista `wp-{id}` (php), `panel-{db}-{ver}` (db), `panel-nginx`, `panel-mailpit`, y `panel-minio` opcional. |
| `Quit`                      | —                                                                                | `void` (cierra la app) | (no expuesta)                                  | `app.exit(0)`.                                                                                          |
| `ListWorktrees`             | `parent_id: string`                                                               | `string` JSON `[{id,name,domain,branch,targetPath,sharedDb}]` | `list_worktrees`                | Filtra por `worktree_of.parent_id`.                                                                     |
| `CreateWorktree`            | `parent_id, target_path, branch, base_branch, shared_db`                          | `string` JSON          | `create_worktree_site`                         | `{"ok": true, "id": ..., "domain": ...}` o error.                                                       |
| `RemoveWorktree`            | `id: string`, `delete_branch: bool`                                              | `bool`                 | `remove_worktree_site`                         | Si `delete_branch`, también borra la rama (`git branch -D`).                                            |
| `CreateSnapshot`            | `id: string`, `label: string`                                                    | `string` JSON          | `create_snapshot`                              | `{"ok": true, "snapshot": SnapshotMeta}` o error.                                                       |
| `ListSnapshots`             | `id: string`                                                                     | `string` JSON `SnapshotMeta[]` | `list_snapshots`                          | `[]` si el proyecto no existe.                                                                          |
| `DeleteSnapshot`            | `id: string`, `snapshot_id: string`                                              | `bool`                 | `delete_snapshot`                              | `remove_dir_all`.                                                                                       |
| `CreateClone`               | `parent_id: string`, `snapshot_id: string`                                       | `string` JSON          | `create_clone`                                 | `{"ok": true, "id": ..., "domain": ...}` o error.                                                       |
| `GhScan`                    | `id: string`                                                                     | `string` JSON `DetectedRepo[]` | `gh_scan`                                   | —                                                                                                       |
| `GhPull`                    | `id: string`, `path: string`, `branch: string`                                   | `string` JSON          | `gh_pull`                                      | `{"ok": true, "output": "..."}` o error.                                                                |
| `GhBranchStatus`            | `id: string`, `path: string`, `branch: string`                                   | `string` JSON          | `gh_branch_status`                             | Serializa `BranchStatus` o `{"ok": false, "error": "..."}`.                                            |
| `GhBuildDirs`               | `id: string`, `path: string`                                                     | `string` JSON `string[]` | `gh_build_dirs`                                | `[]` si el proyecto no existe.                                                                          |
| `GhSetDeploy`               | `id: string`, `path: string`, `branch: string`, `build_cmd: string`, `build_dirs_csv: string` | `bool` | `gh_set_deploy`                              | `build_dirs_csv` se parsea por coma.                                                                    |
| `GhDeploy`                  | `id: string`, `path: string`                                                     | `string` JSON          | `gh_deploy`                                    | `{"ok": true}` o error.                                                                                 |

Todos los métodos D-Bus notifican `sites-changed` al frontend cuando terminan
con éxito y producen una mutación del set de proyectos (`notify_sites_changed`,
`dbus.rs:32-34`).

## 2. CLI shell — `wordpress-panel-cli`

Script: `scripts/wordpress-panel-cli.sh` (instala en `~/.local/bin/wordpress-panel-cli` por `cli::install_cli_wrapper`).

### 2.1 Comandos CLI

| Subcomando                                                  | Método D-Bus                         | Argumentos CLI                                                                                              | Notas                                                                                                                              |
| ----------------------------------------------------------- | ------------------------------------ | ----------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `detect-project <path>`                                     | (lee `config.json` directamente)     | `path`                                                                                                      | No requiere panel en ejecución. Usa `sed` para leer `id`/`path` de `~/panel-wp/*/config.json`.                                       |
| `snapshot list`                                             | `ListSnapshots`                      | autodetect (CWD)                                                                                             | TSV tabla.                                                                                                                          |
| `snapshot create <label>`                                   | `CreateSnapshot`                     | `label`                                                                                                     | —                                                                                                                                   |
| `snapshot delete <snapshotId>`                              | `DeleteSnapshot`                     | `snapshotId`                                                                                                | —                                                                                                                                   |
| `snapshot clone <snapshotId>`                               | `CreateClone`                        | `snapshotId`                                                                                                | —                                                                                                                                   |
| `git scan`                                                  | `GhScan`                             | —                                                                                                            | —                                                                                                                                   |
| `git status [--path <p>] [--branch <b>]`                    | `GhBranchStatus`                     | `--path`, `--branch`                                                                                        | Mensaje + `ahead/behind/dirty/canPull`.                                                                                              |
| `git pull [--path <p>] [--branch <b>]`                      | `GhPull`                             | `--path`, `--branch`                                                                                        | —                                                                                                                                   |
| `git set-deploy [--path <p>] --branch <b> [--build c] [--dirs a,b,c]` | `GhSetDeploy`               | `--path`, `--branch`, `--build`, `--dirs` (CSV)                                                              | —                                                                                                                                   |
| `git deploy [--path <p>]`                                   | `GhDeploy`                           | `--path`                                                                                                    | —                                                                                                                                   |
| `worktree list`                                             | `ListWorktrees`                      | autodetect (CWD)                                                                                             | —                                                                                                                                   |
| `worktree create <branch> [--target <p>] [--base <b>] [--copy-db]` | `CreateWorktree`         | `--target`, `--base`, `--copy-db` (invierte `shared_db` a `false`)                                            | CWD debe estar dentro del repo del padre.                                                                                            |
| `worktree remove <id> [--delete-branch]`                    | `RemoveWorktree`                     | `--delete-branch`                                                                                            | —                                                                                                                                   |
| `list` / `ls`                                               | `ListSites`                          | —                                                                                                            | TSV `'ESTADO','NOMBRE','DOMINIO','GRUPO','ID'`.                                                                                      |
| `start [proyecto]`                                          | `StartSite`                          | `proyecto` opcional (id, subcadena de nombre, o vacío=CWD)                                                    | `resolve_pid` (`scripts/wordpress-panel-cli.sh:92-112`).                                                                            |
| `stop [proyecto]`                                           | `StopSite`                           | `proyecto` opcional                                                                                          | —                                                                                                                                   |
| `open admin`                                                | `OpenAdmin`                          | —                                                                                                            | —                                                                                                                                   |
| `login-url [proyecto] [--user <id\|login>]`                  | `AdminUrl`                           | `proyecto` opcional (id, subcadena, o vacío=CWD); `--user` ID o `user_login`                                  | Imprime solo la URL (pipeable). Para revisiones en un navegador cualquiera (MCP `admin_login_url`).                                  |
| `open site` / `open front`                                  | `OpenSite`                           | —                                                                                                            | —                                                                                                                                   |
| `open folder`                                               | (no D-Bus)                           | —                                                                                                            | `xdg-open` directo al path.                                                                                                          |
| `containers`                                                | `ProjectContainers`                  | —                                                                                                            | TSV `'NAME','ROLE','RUNNING'`.                                                                                                       |
| `resources`                                                 | `ProjectContainers` + `docker stats` | —                                                                                                            | `docker inspect` por nombre para filtrar existentes, luego `docker stats --no-stream`.                                              |
| `logs [servicio] [-f] [-n N]`                               | (no D-Bus)                           | `service ∈ {php, db, nginx, mailpit, minio, <nombre_crudo>}`, `-f` (follow), `-n N` (tail)                  | Default `php`, `tail=200`. `docker logs`.                                                                                            |
| `-h`, `--help`                                              | —                                    | —                                                                                                            | `usage` definido en el script.                                                                                                       |

### 2.2 Reglas del CLI

- `require_panel` (línea 73-82): ping a `GetRunningSites` antes de cualquier
  comando que no sea `detect-project` o `open folder`.
- `project_or_die` (línea 85-87): deduce `id` + `path` del CWD.
- `resolve_pid <arg>` (línea 92-112): si `arg` está vacío → CWD; si no → id
  exacto, o coincidencia por subcadena `name` (case-insensitive). Si no
  encuentra o hay ambigüedad, sale con `exit 1` o `exit 2`.
- `git_target_path <ppath>` (línea 116-123): deduce el path relativo a
  `public/` del repo en CWD.
- `dbus_call` (línea 37-48) y `dbus_json` (línea 52-71): helpers. Prefieren
  `gdbus`, caen a `qdbus6`. `dbus_json` quita el envoltorio `('…',)` de
  `gdbus` con `python3 -c ast.literal_eval` y embellece con `jq`.

### 2.3 Códigos de salida

| Código | Significado                                                                                  |
| ------ | -------------------------------------------------------------------------------------------- |
| 0      | ok                                                                                            |
| 1      | fallo (proyecto no encontrado, import cancelado, etc.)                                       |
| 2      | uso incorrecto / ambigüedad                                                                   |
| 3      | `gdbus` y `qdbus6` no disponibles                                                            |
| 4      | panel no en ejecución (`require_panel` falló)                                                  |

## 3. Wrapper `wp`

`scripts/wp-wrapper.sh` (instalado en `~/.local/bin/wp` por `cli::install_cli_wrapper`):

```bash
PROJECT_ID="$(wordpress-panel-cli detect-project "$PWD" 2>/dev/null || true)"
exec docker exec -i --user www-data "wp-${PROJECT_ID}" \
    php /usr/local/bin/wp --path=/var/www/html "$@"
```

- Convierte `wp <args>` en `docker exec ... wp <args>` dentro del container del
  proyecto del CWD.
- Como `www-data` (paridad con `apache2::wpcli::run`).
- Si no detecta proyecto, sale con `exit 1` y mensaje claro.

## 4. Servidor MCP — `mcp/server.mjs`

Sin dependencias externas (`mcp/package.json` dice `"type": "module"` y
`"private": true`). Protocolo JSON-RPC 2.0 por stdio, una línea por mensaje.

### 4.1 Catálogo de herramientas MCP

| Herramienta (nombre)    | Args (inputSchema)                                                                              | Equivalente CLI                                  | Notas                                                                                                |
| ----------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| `list_projects`         | `{}`                                                                                             | `list`                                          | —                                                                                                    |
| `start_project`         | `{project: string}`                                                                              | `start <project>`                               | —                                                                                                    |
| `stop_project`          | `{project: string}`                                                                              | `stop <project>`                                | —                                                                                                    |
| `project_containers`    | `{project: string}`                                                                              | `containers`                                    | —                                                                                                    |
| `project_resources`     | `{project: string}`                                                                              | `resources`                                     | —                                                                                                    |
| `project_logs`          | `{project: string, service?: 'php'\|'db'\|'nginx'\|'mailpit'\|'minio', lines?: number}`            | `logs <service> -n <lines>`                     | Default `service=php`, `lines=200`.                                                                  |
| `set_php_upload_limit`  | `{project: string, mb: integer}`                                                               | (vía D-Bus, no CLI directo)                     | Ajusta `upload_max_filesize` + `post_max_size` y recarga php-fpm.                                    |
| `open_project`          | `{project: string, what: 'admin'\|'site'\|'folder'}`                                              | `open <what>` (con `project` deriv. del CWD)     | —                                                                                                    |
| `list_snapshots`        | `{project: string}`                                                                              | `snapshot list`                                 | —                                                                                                    |
| `create_snapshot`       | `{project: string, label: string}`                                                               | `snapshot create <label>`                       | —                                                                                                    |
| `delete_snapshot`       | `{project: string, snapshotId: string}`                                                          | `snapshot delete <snapshotId>`                  | —                                                                                                    |
| `clone_snapshot`        | `{project: string, snapshotId: string}`                                                          | `snapshot clone <snapshotId>`                   | —                                                                                                    |
| `git_scan`              | `{project: string}`                                                                              | `git scan`                                      | —                                                                                                    |
| `git_status`            | `{project: string, path: string, branch?: string}`                                                | `git status --path <p> --branch <b>`            | —                                                                                                    |
| `git_pull`              | `{project: string, path: string, branch?: string}`                                                | `git pull --path <p> --branch <b>`              | —                                                                                                    |
| `git_set_deploy`        | `{project: string, path: string, branch: string, build?: string, dirs?: string}`                  | `git set-deploy --path <p> --branch <b> --build c --dirs a,b,c` | —                                                                                |
| `git_deploy`            | `{project: string, path: string}`                                                                | `git deploy --path <p>`                         | —                                                                                                    |
| `worktree_list`         | `{project: string}`                                                                              | `worktree list`                                 | —                                                                                                    |
| `worktree_create`       | `{project: string, branch: string, target?: string, base?: string, copyDb?: boolean}`             | `worktree create <branch> --target … --base … [--copy-db]` | —                                                                                    |
| `worktree_remove`       | `{project: string, worktreeId: string, deleteBranch?: boolean}`                                  | `worktree remove <id> [--delete-branch]`        | —                                                                                                    |

### 4.2 Métodos JSON-RPC soportados

`mcp/server.mjs:290-313` (`handle`):

| Método                         | Soporte                                                                                  |
| ------------------------------ | ---------------------------------------------------------------------------------------- |
| `initialize`                   | sí (protocolo `2024-11-05`, capabilities `{tools: {}}`, serverInfo `wordpress-panel 0.1.0`) |
| `ping`                         | sí (`{}`)                                                                                |
| `tools/list`                   | sí                                                                                       |
| `tools/call`                   | sí                                                                                       |
| `notifications/initialized`    | sí (no devuelve, `null`)                                                                 |
| `notifications/cancelled`      | sí (no devuelve, `null`)                                                                 |
| Otros                          | error `-32601` "método no soportado"                                                     |

### 4.3 Resolución del proyecto en MCP

`resolveProject(arg)` (`mcp/server.mjs:53-63`):

1. Lee `~/panel-wp/*/config.json` (todos los directorios).
2. Busca id exacto.
3. Si no, busca `name` que contenga `arg` (case-insensitive).
4. Si no hay match o hay ambigüedad, lanza con mensaje claro.

### 4.4 Resolución del CLI

`runCli(args, cwd)` (`mcp/server.mjs:66-77`):

- `CLI` se resuelve por `process.env.WORDPRESS_PANEL_CLI` → `~/.local/bin/wordpress-panel-cli` → `scripts/wordpress-panel-cli.sh` del repo → fallback `wordpress-panel-cli` (PATH).
- `cwd` = la carpeta del proyecto (de `resolveProject`) si la tool lo requiere
  (`needProject: true`).
- Devuelve `{ code, out, err }`. El `text` se concatena filtrando vacíos.
- `isError: code !== 0`.

## 5. Cobertura cruzada: ¿qué comandos existen en cada capa?

| Funcionalidad                          | IPC (`api.ts`) | D-Bus (`dbus.rs`) | CLI (`scripts/wordpress-panel-cli.sh`) | MCP (`mcp/server.mjs`) |
| -------------------------------------- | -------------- | ----------------- | -------------------------------------- | ---------------------- |
| Listar proyectos                       | sí (`get_sites`) | sí (`ListSites`, `GetRunningSites`) | sí (`list`)                              | sí (`list_projects`)   |
| Start/Stop                             | sí             | sí                | sí (`start`/`stop`)                      | sí                     |
| Open admin/site/folder                 | sí             | `OpenAdmin`/`OpenSite` | sí (`open admin`/`open site`/`open folder`) | parcialmente (`open_project`) |
| Containers                            | (no directa)   | `ProjectContainers` | sí (`containers`)                       | sí (`project_containers`) |
| Resources                              | (no)           | (no)              | sí (`resources`)                        | sí (`project_resources`) |
| Logs                                   | vía `stream_logs` (UI) | (no)         | sí (`logs`)                             | sí (`project_logs`)    |
| WP-CLI                                 | `exec_wpcli`   | (no)              | `scripts/wp-wrapper.sh` (separado)      | (no)                   |
| System status                          | sí             | (no)              | (no)                                    | (no)                   |
| PHP upload limit                       | sí (`set_php_upload_limit`) | sí (`SetUploadLimit`) | (no — solo vía UI/D-Bus/MCP)      | sí (`set_php_upload_limit`) |
| Migrate                                | sí             | (no)              | (no)                                    | (no)                   |
| Delete                                 | sí             | (no)              | (no)                                    | (no)                   |
| LocalWP import                         | sí             | (no)              | (no)                                    | (no)                   |
| Disconnected import                    | sí             | (no)              | (no)                                    | (no)                   |
| Snapshots (create/list/delete)         | sí             | sí                | sí (`snapshot ...`)                     | sí                     |
| Clones                                 | sí             | sí                | sí (`snapshot clone`)                   | sí (`clone_snapshot`)  |
| Worktrees (create/list/remove)         | sí             | sí                | sí (`worktree ...`)                     | sí                     |
| GitHub: clone / pull / pull_all / remove | sí          | (no)              | (no)                                    | (no)                   |
| GitHub: scan / register / build_dirs   | sí             | parcialmente (`GhScan`) | `git scan`                          | `git_scan`             |
| GitHub: branch_status / set_deploy / deploy | sí       | sí                | `git status` / `git set-deploy` / `git deploy` | `git_status` / `git_set_deploy` / `git_deploy` |
| Open VSCode                            | sí             | (no)              | (no)                                    | (no)                   |
| Regenerate SSL                         | sí             | (no)              | (no)                                    | (no)                   |
| Groups                                 | sí             | (no)              | (no)                                    | (no)                   |
| MinIO toggle / open                    | sí             | (no)              | (no)                                    | (no)                   |
| Mailpit open                           | sí             | (no)              | (no)                                    | (no)                   |
| Adminer open                           | sí             | (no)              | (no)                                    | (no)                   |
| Install CLI wrapper                    | sí             | (no)              | (no — el propio script instala)         | (no)                   |
| Quit                                   | (no)           | sí (`Quit`)       | (no)                                    | (no)                   |

## 6. Cuando el CLI/MCP llama a D-Bus, ¿el panel se entera?

Sí. `dbus::Manager` siempre llama a `notify_sites_changed(&self.app)` que
emite `sites-changed` (`dbus.rs:32-34`). El frontend escucha este canal en
`src/routes/+page.svelte:181` y recarga la lista (`api.getSites()`).

> Por tanto, **no hace falta polling**: cualquier mutación del set de
> proyectos (start/stop/worktree/clone) que pase por D-Bus se refleja en la
> UI sin interacción del usuario.

## 7. Estado de deuda / Diferido

- `feature_stub` (Tauri) cubre `cloudflare`/`deploy`/`package`. NO tiene
  equivalente D-Bus / CLI / MCP porque es UI-only.
- `OpenAdmin` vía D-Bus no acepta `user_id` (la IPC sí: `open_admin(id, user_id?)`).
- `Quit` solo se expone por D-Bus. La GUI cierra por la X de la ventana.
- `adminer` no se lista en `ProjectContainers` (es un servicio "bajo demanda").
- `mcp/server.mjs` no implementa `resources` ni `prompts` (solo `tools`).
- El servidor MCP hereda la dependencia del CLI: requiere `~/.local/bin/wordpress-panel-cli`
  o `WORDPRESS_PANEL_CLI` o el script del repo. No hay modo "fully standalone".

## 8. Restricciones y entorno

| Variable/Herramienta  | Origen                                                                                  |
| --------------------- | --------------------------------------------------------------------------------------- |
| `PANEL_WP_ROOT`       | `mcp/server.mjs:21`, `scripts/wordpress-panel-cli.sh:14` (override de `~/panel-wp`).     |
| `WORDPRESS_PANEL_CLI` | `mcp/server.mjs:25` (override explícito del CLI).                                       |
| `gdbus`               | Preferido en CLI (`scripts/wordpress-panel-cli.sh:39, 47, 58, 75`).                       |
| `qdbus6`              | Fallback (KDE/qt6dbus).                                                                  |
| `jq`                  | Necesario en CLI para `json` parseo (`dbus_json`).                                       |
| `python3`             | `ast.literal_eval` para des-escapar `gdbus`.                                             |
| `docker`              | CLI: `wp-wrapper.sh` (`docker exec -i`), `php::ensure_php_image` (`docker build`), `docker::migrate_db_to_volume` (`docker cp`), `migrate::import_dump` (`docker exec -i`). |
| `tar`                 | `tar --zstd -cf/-xf` (snapshot).                                                        |
| `gh` / `git`          | `github::*`.                                                                            |
| `mkcert`              | `ssl::generate`. Necesita CA (`mkcert -install`) hecho por `scripts/first-run.sh`.       |
| `pkexec`              | `domain::install_wildcard` (instalación privilegiada de dnsmasq).                        |
| `mysqldump` / `mysql` / `psql` / `pg_isready` | Binarios del container DB, invocados vía bollard.                       |

## Fuentes primarias

- `src-tauri/src/dbus.rs` (interface zbus).
- `scripts/wordpress-panel-cli.sh` (CLI completo).
- `scripts/wp-wrapper.sh` (wrapper `wp`).
- `mcp/server.mjs` (servidor MCP).
- `mcp/package.json` (sin deps).
- `src-tauri/src/lib.rs` (equivalentes IPC).
- `src-tauri/src/cli.rs` (instalación de wrappers).
- `src-tauri/src/system.rs` (estado del plasmoid).
- `docs/CHANGELOG.md`, `docs/TESTING.md`.
