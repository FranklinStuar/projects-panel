# Runbook de Git, CLI y MCP

Este runbook cubre las tres superficies externas del panel que hablan D-Bus con el backend en ejecución:

- `scripts/wp-wrapper.sh` + `scripts/wordpress-panel-cli.sh` (instalados por el panel en `~/.local/bin/`).
- `mcp/server.mjs` (envoltorio MCP para agentes IA como Claude Code u opencode).
- `src-tauri/src/github.rs::deploy` y comandos relacionados (deploy directo por repo).

Cubre uso, integración con cada host (terminal, Claude Code, opencode) y los riesgos asociados.

## Matriz rápida

| Operación | Wrapper `wp` | CLI `wordpress-panel-cli` | MCP `mcp/server.mjs` | D-Bus (servicio `com.goldmediatech.WordpressPanel`) |
|---|---|---|---|---|
| WP-CLI en el container del proyecto | `wp <args>` | n/d | n/d | n/d |
| Listar proyectos | n/d | `list` / `ls` | `list_projects` | `ListSites` |
| Encender / detener | n/d | `start [proyecto]` / `stop [proyecto]` | `start_project` / `stop_project` | `StartSite(id)` / `StopSite(id)` |
| Apagar todo | n/d | n/d | n/d | `StopAll` |
| Abrir admin / site / folder | n/d | `open {admin,site,folder}` | `open_project` | `OpenAdmin(id)` / `OpenSite(id)` |
| Containers / recursos / logs | n/d | `containers` / `resources` / `logs [servicio] [-f] [-n N]` | `project_containers` / `project_resources` / `project_logs` | `ProjectContainers(id)` |
| Snapshots | n/d | `snapshot {list,create,delete,clone}` | `list/create/delete/clone_snapshot` | `ListSnapshots` / `CreateSnapshot` / `DeleteSnapshot` / `CreateClone` |
| Git scan / status / pull | n/d | `git {scan,status,pull}` | `git_{scan,status,pull}` | `GhScan` / `GhBranchStatus` / `GhPull` |
| Deploy directo | n/d | `git {set-deploy,deploy}` | `git_{set_deploy,deploy}` | `GhSetDeploy` / `GhDeploy` |
| Worktrees | n/d | `worktree {list,create,remove}` | `worktree_{list,create,remove}` | `ListWorktrees` / `CreateWorktree` / `RemoveWorktree` |
| Tope de subida PHP | n/d | `php upload <MB>` | `set_php_upload_limit` | `SetUploadLimit(id, mb)` |
| Cerrar el panel | n/d | n/d | n/d | `Quit` |

`wp` es local al CWD (detecta el proyecto por la ruta actual). El resto trabaja con un proyecto identificado por **id** o **nombre** (subcadena case-insensitive), con resolución `cwd` por defecto. La UI es la fuente de verdad para crear, migrar, importar y borrar; el CLI/MCP están pensados para inspección y automatización.

## 1. Precondiciones universales

- El panel está abierto (`dbus.rs::serve` está dentro del `setup` de Tauri).
- El bus de sesión D-Bus está disponible (no se necesita un Plasma corriendo: `dbus-daemon` de sesión basta).
- `gdbus` o `qdbus6` para que el CLI hable con el panel.
- `~/.local/bin` está en `PATH` (recomendado: `export PATH="$HOME/.local/bin:$PATH"` en tu shell).
- `node` ≥ 18 para el MCP.

Si el panel no está abierto, el CLI responde `error: el panel WordPress no está en ejecución (ábrelo para usar el panel)` y sale con código 4. Las herramientas MCP devuelven `isError: true` con el mismo mensaje.

## 2. Wrapper `wp` (WP-CLI en terminal)

### Instalación y detección

`cli::install_cli_wrapper` se ejecuta al arrancar el panel (`lib.rs::run::setup`) y copia `scripts/wp-wrapper.sh` a `~/.local/bin/wp` con permisos `755`. Es idempotente; lo puedes forzar desde **Configuración → Servicios → "Solo instalar wrapper `wp`"** o con `wordpress-panel-cli install-cli-wrapper` desde la D-Bus.

`scripts/wp-wrapper.sh`:

1. Llama a `wordpress-panel-cli detect-project "$PWD"` para resolver el `id` del proyecto cuya carpeta contiene el CWD. Si no detecta, sale con `wp: no se detectó ningún proyecto Panel WP en $PWD` y código 1.
2. Ejecuta `docker exec -i --user www-data "wp-${PROJECT_ID}" php /usr/local/bin/wp --path=/var/www/html "$@"`.
   - `--user www-data`: paridad con el comando in-app `exec_wpcli` (que también es www-data). WP-CLI rechaza root (`YIKES`); el wrapper de terminal usaba root antes del fix de `docs/CHANGELOG.md::Fix — el wrapper wp de terminal corría como root`.
   - `--path=/var/www/html`: ruta dentro del container donde está montado `app/public/`.
   - El binario `wp` dentro del container es el phar de WP-CLI descargado por `php::wp_cli_phar_path`.

### Procedimiento

```bash
cd ~/panel-wp/mi-sitio/app/public/wp-content/themes/mi-theme
wp plugin list
wp post list --post_type=page
wp user list
```

### Cambio esperado y evidencia

- La salida es la de WP-CLI dentro del container.
- `wp cli info` debería listar la versión del phar; la del sitio PHP en `info --allow-root` requiere permisos (mejor usar `wp cli info` con www-data).

### Precondiciones, abortar y recuperar

- Precondición: el proyecto está **encendido**. Si no, `wp` falla con "el container wp-{id} no existe". Enciéndelo desde la UI/CLI antes.
- `Ctrl+C` aborta el comando. WP-CLI lo propaga como `exit 130` o similar; el wrapper no toca estado del sitio.
- `wp search-replace` puede tardar y tocar la DB; ejecútalo con cuidado (no es destructivo si sabes qué cadena reemplazas).
- Si el wrapper se queda antiguo (pre-fix con `--user www-data`), relanza el panel o ejecuta `install_cli_wrapper`; el `cli::install_one` sobrescribe el archivo.

### Limitaciones

- WP-CLI tiene un timeout interno de 120 s (`wpcli::WPCLI_TIMEOUT`) en el comando in-app; el wrapper de terminal no aplica ese timeout (es una llamada a `docker exec` directa). Para comandos largos, considera ejecutarlos con un timeout en tu shell.
- `wp db import` desde el wrapper pasa por la imagen php, no por el container DB. El certificado autofirmado de MySQL 8 puede hacer que `wp db import` falle; en su lugar, usa el flujo de **Migrar y encender** (§04).

## 3. CLI `wordpress-panel-cli`

### Instalación y detección

`cli::install_cli_wrapper` también copia `scripts/wordpress-panel-cli.sh` a `~/.local/bin/wordpress-panel-cli`. `scripts/wp-wrapper.sh` lo invoca para `detect-project`. `scripts/first-run.sh::paso 5` también lo instala.

### Comandos

`scripts/wordpress-panel-cli.sh::usage`:

```text
detect-project <ruta>
    Imprime el id del proyecto que contiene la ruta.

snapshot   (autodetecta el proyecto del directorio actual)
  snapshot list                     Lista los puntos de guardado.
  snapshot create <label>           Crea un punto de guardado.
  snapshot delete <snapshotId>      Borra un punto de guardado.
  snapshot clone <snapshotId>       Crea un clon temporal desde el snapshot.

git   (repo objetivo inferido del CWD; override con --path <ruta rel a public/>)
  git scan                                    Lista repos del proyecto.
  git status  [--path <p>] [--branch <b>]     Estado de la rama.
  git pull    [--path <p>] [--branch <b>]     git pull de la rama.
  git set-deploy [--path <p>] --branch <b> [--build "<cmd>"] [--dirs a,b,c]
                                                Configura el deploy.
  git deploy  [--path <p>]                    Ejecuta el deploy guardado.

worktree   (autodetecta el proyecto del directorio actual)
  worktree list
  worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]
  worktree remove <id-worktree> [--delete-branch]

list | ls                           Lista TODOS los proyectos con su estado.
start [proyecto]                    Enciende un proyecto.
stop  [proyecto]                    Apaga un proyecto.
open <qué>                          admin|site|front|folder.
containers                          Lista los containers del proyecto.
resources                           docker stats de los containers.
logs [servicio] [-f] [-n N]         Ver logs (php, db, nginx, mailpit, minio o nombre).
php upload <MB>                     Tope de subida del proyecto.
```

### Variables de entorno

- `PANEL_WP_ROOT`: ruta alternativa a `~/panel-wp` para `detect-project` y `project_or_die`. Útil si moviste la raíz de proyectos.
- `WORDPRESS_PANEL_CLI`: el MCP la respeta; el CLI bash no la usa directamente.

### Procedimiento

- **Listar**: `wordpress-panel-cli list` imprime una tabla con columnas `ESTADO`, `NOMBRE`, `DOMINIO`, `GRUPO`, `ID` (jq: `wordpress-panel-cli list | jq -r '.[] | "\(.name)\t\(.domain)\t\(.running)"'`).
- **Encender/Detener**: el nombre o id puede ser subcadena (`resolve_pid` con ascii_downcase). Si el nombre es ambiguo, el CLI imprime las coincidencias y sale con `exit 2`.
- **Logs**: el servicio `php` se traduce a `wp-{id}`; los demás, `db|nginx|mailpit|minio`, se resuelven vía D-Bus (`dbus_json ProjectContainers` con `select(.role==$r)`).
- **Resources**: `docker stats --no-stream` sobre los containers existentes del proyecto (filtrado por `docker inspect`).

### Cambio esperado y evidencia

- `list` y `containers` salen con `code 0`; `start`/`stop` con `code 0` y un ✓.
- `logs` usa `docker logs --tail N`; `-f` con `--follow` o `-f`. En Mosh/screen remoto, `-f` puede perder el terminal al cerrar.

### Precondiciones, abortar y recuperar

- Si el panel no está abierto, el CLI falla con `error: el panel WordPress no está en ejecución` y `exit 4`. Abre el panel.
- Si ni `gdbus` ni `qdbus6` están instalados: `error: ni gdbus ni qdbus6 disponibles para hablar con el panel` y `exit 3`. Instala `glib2` (gdbus) o `qt6-tools` (qdbus6).
- `git deploy` y `git set-deploy` se ejecutan vía D-Bus; el método D-Bus espera argumentos `String` y separa por coma (sin espacios en `dirs`).
- `php upload <MB>` valida que `<MB>` sea entero; un valor no numérico falla con `MB debe ser un entero (0 = default)`.
- `Ctrl+C` durante una operación larga: el `docker exec` o el D-Bus se queda en el panel; el wrapper de bash sale con código 130. El panel continúa. Reanuda donde corresponda.

### Limitaciones

- `resolve_pid` solo matchea por id exacto o por subcadena del nombre. No hay `jq` para el id.
- Los códigos de salida son: `0` ok, `1` genérico, `2` argumento inválido, `3` falta gdbus/qdbus6, `4` panel no abierto.
- `dbus_json` filtra la salida de `gdbus` con `python3 -c '...literal_eval...'` para desenvolver la tupla `('json',)`. Si tu `python3` no está en el PATH, falla.
- El wrapper no serializa errores del panel: muestra el stdout/stderr del D-Bus tal cual.

### Riesgos

- `git deploy` y `gh_deploy` ejecutan comandos en `sh -lc` en el host. Si el `build_cmd` configurado es destructivo (p. ej. `rm -rf`), el daño se propaga. Define comandos idempotentes (`pnpm install && pnpm build`); no usar `&&` sin revisión.
- `git pull --ff-only` rechaza pull no fast-forward; el wrapper reporta el error. Si necesitas un merge local, hazlo desde el editor.
- `dbus_json ListSnapshots` puede devolver JSON muy grande con muchos snapshots; no hay paginación.

## 4. MCP `mcp/server.mjs`

### Registro del servidor

`mcp/README.md` documenta el setup. Resumen:

- Claude Code, ámbito usuario:

  ```bash
  claude mcp add wordpress-panel --scope user -- node /home/franklin/MEGA/dev/wordpress-panel/mcp/server.mjs
  ```

- opencode, en `~/.config/opencode/opencode.json` (o el del proyecto) bajo `mcp.wordpress-panel`.

  ```json
  {
    "mcp": {
      "wordpress-panel": {
        "type": "local",
        "command": ["node", "/home/franklin/MEGA/dev/wordpress-panel/mcp/server.mjs"],
        "enabled": true
      }
    }
  }
  ```

- Verifica: `claude mcp list` debe mostrar `wordpress-panel … ✔ Connected`. Si no conecta, arranca el panel y reejecuta el comando.

### Variables de entorno del MCP

- `WORDPRESS_PANEL_CLI`: ruta explícita al CLI. Default: `~/.local/bin/wordpress-panel-cli` o `scripts/wordpress-panel-cli.sh` del repo.
- `PANEL_WP_ROOT`: ruta alternativa a `~/panel-wp`. Debe coincidir con la del panel abierto.

### Catálogo (19 herramientas)

| Herramienta | Args | Notas |
|---|---|---|
| `list_projects` | — | `argv: ['list']`. |
| `start_project` | `project` (id o nombre) | `argv: ['start', project]`. |
| `stop_project` | `project` | `argv: ['stop', project]`. |
| `project_containers` | `project` | `argv: ['containers']`, ejecuta con cwd del proyecto. |
| `project_resources` | `project` | `argv: ['resources']`. |
| `project_logs` | `project`, `service` ∈ `php\|db\|nginx\|mailpit\|minio`, `lines` | `argv: ['logs', service||'php', '-n', lines||200]`. |
| `open_project` | `project`, `what` ∈ `admin\|site\|folder` | `argv: ['open', what]`. |
| `list_snapshots` | `project` | `argv: ['snapshot', 'list']`. |
| `create_snapshot` | `project`, `label` | `argv: ['snapshot', 'create', label]`. |
| `delete_snapshot` | `project`, `snapshotId` | `argv: ['snapshot', 'delete', snapshotId]`. |
| `clone_snapshot` | `project`, `snapshotId` | `argv: ['snapshot', 'clone', snapshotId]`. |
| `git_scan` | `project` | `argv: ['git', 'scan']`. |
| `git_status` | `project`, `path` (rel. a public/), `branch?` | `argv: ['git', 'status', '--path', path, ...('--branch', branch)?]`. |
| `git_pull` | `project`, `path`, `branch?` | idem con `pull`. |
| `git_set_deploy` | `project`, `path`, `branch`, `build?`, `dirs?` (CSV) | `argv: ['git', 'set-deploy', '--path', path, '--branch', branch, ...('--build', build)?, ...('--dirs', dirs)?]`. |
| `git_deploy` | `project`, `path` | `argv: ['git', 'deploy', '--path', path]`. |
| `worktree_list` | `project` | `argv: ['worktree', 'list']`. |
| `worktree_create` | `project`, `branch`, `target?`, `base?`, `copyDb?` | `argv: ['worktree', 'create', branch, ...flags]`. |
| `worktree_remove` | `project`, `worktreeId`, `deleteBranch?` | `argv: ['worktree', 'remove', worktreeId, ...('--delete-branch')?]`. |
| `set_php_upload_limit` | `project`, `mb` | `argv: ['php', 'upload', String(mb)]`. |

Adicionalmente, las mutaciones de D-Bus (start, stop, all, create/remove worktree, create clone) emiten `sites-changed` y la UI del panel se recarga sola (`+page.svelte::listen('sites-changed', () => load())`).

### Precondiciones y resolución de proyecto

- `mcp/server.mjs::resolveProject` carga `~/panel-wp/*/config.json`, busca por id exacto o por subcadena del nombre (case-insensitive). Si no hay match, error `"no hay proyecto que coincida con «<arg>»"`. Si hay varios, error `"«<arg>» es ambiguo: a, b, …"`.
- Los comandos que tocan un proyecto (cualquier `needProject: true`) corren con `cwd = resolveProject(args.project).path` para que `scripts/wordpress-panel-cli.sh::project_or_die` detecte el proyecto por CWD.

### Procedimiento

Las herramientas se invocan desde el cliente MCP (Claude Code, opencode). El servidor no implementa recursos, solo herramientas, y responde por stdio:

- `initialize` / `notifications/initialized` para el handshake.
- `tools/list` para el catálogo.
- `tools/call` con `name` y `arguments` (un objeto). El servidor devuelve `content: [{ type: 'text', text }]` y `isError: true/false`.

### Cambio esperado y evidencia

- `isError: false` y `text` con la salida del CLI (puede ser un volcado de texto, una tabla jq-formateada, o un JSON).
- En Claude Code, las herramientas aparecen como `mcp__wordpress-panel__<herramienta>`.
- En opencode, aparecen como `wordpress-panel.<herramienta>`.

### Abortar y recuperar

- Si una herramienta devuelve `isError: true`, el `text` empieza con `(sin salida)` solo si el CLI no imprimió nada. Normalmente el CLI sí imprime el error.
- `resolveProject` lanza excepciones (`throw`) que el `toolsCall` atrapa y devuelve como `isError: true`. El agente MCP debe manejar esos errores re-preguntando o refiniendo el nombre.
- Si el panel se cierra a mitad de una llamada, el CLI falla con `error: el panel WordPress no está en ejecución`; la herramienta devuelve `isError: true`.

### Limitaciones

- El servidor no implementa autenticación, rate limiting ni journaling. Cualquiera con acceso al proceso MCP puede invocar las herramientas.
- No hay `notifications/initialized` y `notifications/cancelled` más allá de la conformidad del protocolo.
- El `set_php_upload_limit` del MCP exige `mb` entero; el servidor lo convierte a `String` antes de pasar al CLI.
- `git_set_deploy` con `dirs` vacío se omite el flag; con `dirs` con comas vacías, el CLI los filtra.

### Riesgos

- El MCP hereda los riesgos del CLI: `git_deploy` ejecuta `sh -lc` con el `build_cmd` configurado. Un agente comprometido o mal configurado podría desplegar código no revisado.
- `delete_snapshot` y `worktree_remove` son destructivos y no tienen confirmación humana desde el MCP.
- Las mutaciones vía MCP cambian el estado sin que la UI lo solicite explícitamente: la UI se recarga vía `sites-changed`, pero un usuario que esté editando el proyecto perderá cambios no guardados en la UI.

## 5. Git / Deploy directo

### Precondiciones

- Repo registrado en `site.github.repos` (`gh_clone` o `gh_register`).
- Rama objetivo y (opcional) comando de build y carpetas de build.

### Procedimiento

1. **Escanear repos**: `wordpress-panel-cli git scan` (o `gh_scan` por MCP). Devuelve `path, branch, remote, registered`.
2. **Verificar estado**: `wordpress-panel-cli git status --path wp-content/themes/mi-theme --branch main` (`gh_branch_status` por MCP). Devuelve `current`, `target`, `hasRemote`, `ahead`, `behind`, `dirty`, `canPull`, `message`.
3. **Configurar deploy**: `wordpress-panel-cli git set-deploy --branch main --build "pnpm install && pnpm build" --dirs dist`. Persiste `repo.branch`, `repo.buildCmd`, `repo.buildDirs` en `config.json` (o en el sidecar de la D-Bus).
4. **Ejecutar deploy**: `wordpress-panel-cli git deploy --path wp-content/themes/mi-theme` (o `gh_deploy` por MCP). Internamente:
   - `git checkout {branch}` (falla si el árbol está sucio).
   - `git pull --ff-only origin {branch}` (falla si diverge).
   - `sh -lc {build_cmd}` en cada `build_dirs` (raíz o subcarpeta). `-lc` carga el perfil del usuario (nvm/pnpm).
5. **Revisar output**: las líneas del build se emiten por `op-log`; el código de salida se reporta al final.

### Cambio esperado y evidencia

- `git -C {path} status` muestra la rama objetivo, sin cambios locales, sin commits por delante.
- `ls {path}/dist` (o la carpeta de build) tiene los artefactos esperados.
- En la UI, el estado de `branch_status` muestra `behind: 0`, `dirty: false`, `canPull: false`.

### Abortar y recuperar

- `Ctrl+C` no aborta; el deploy espera al pull/build.
- Si el build falla, el código del repo queda en la rama objetivo con el pull aplicado. Corrige el `build_cmd` o `build_dirs` y reejecuta.
- Si la rama diverge, abre el editor, resuelve el merge y reejecuta.

### Limitaciones

- El deploy es por repo, no por sitio. Si tu proyecto tiene varios repos registrados (`github.repos`), despliega cada uno por separado.
- `gh_deploy` no soporta un build que requiera sudo o un entorno concreto; el comando se ejecuta con los permisos del usuario que arrancó el panel.
- `set-deploy` con `branch` vacío se ignora (mantiene el branch anterior); con `branch` no vacío y repo no registrado, devuelve error.

### Riesgos

- `sh -lc {build_cmd}` ejecuta con el shell del usuario. Comandos que dependan de variables de entorno no presentes en un shell limpio (nvm no inicializado, pnpm en `~/.local/bin` sin PATH, etc.) fallan. Usa comandos absolutos o exporta el PATH necesario.
- `git pull --ff-only` es seguro pero rechaza no-FF; el `git_deploy` retorna el error y deja el árbol como está. No hay merge automático.
- No hay sandboxing: el build corre con tus permisos. Revisa el `build_cmd` antes de guardar el deploy.

## 6. Criterio de salida

- La operación devuelve sin error (CLI exit 0 o `isError: false` en MCP).
- El cambio es visible en la UI: el proyecto se mueve a "corriendo", el snapshot aparece en la lista, el worktree sirve, etc.
- Si la mutación vino del CLI/MCP, la UI ya se recargó vía `sites-changed`; no hace falta pulsar F5.

Los runbooks anteriores cubren cada flujo en profundidad. El runbook de diagnóstico está en `07-diagnostico-y-mantenimiento.md`.
