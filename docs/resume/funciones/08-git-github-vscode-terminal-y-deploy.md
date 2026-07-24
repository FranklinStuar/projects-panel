# 08 · Git, GitHub, VSCode, terminal y deploy directo

Cubre la integración genérica con `git` y `gh` en el host (sin auth propia),
la apertura de VSCode con un workspace multi-root, la apertura de un emulador
de terminal con el wrapper `wp`, y el deploy directo
(`checkout` + `git pull --ff-only` + build en host) usado para staging.

> Importante: el panel **no tiene auth propia de GitHub**. Reutiliza la sesión
> y las SSH keys del usuario en el host (ver `github.rs::6-9`). El binario
> `gh` debe estar instalado y autenticado por separado.

## Resultado para el usuario

- **Detectar** repos git bajo `wp-content/` (registrados en `config.json` o
  huérfanos). Cada repo lleva `path`, `name`, `remote`, `branch`,
  `registered`.
- **Clonar** un repo desde GitHub con `gh repo clone` (en `wp-content/themes`,
  `plugins` o `mu-plugins`, o en una ruta custom) y registrarlo en
  `github.repos`.
- **Pull** (uno o todos) en el host, aprovechando bind-mounts: los cambios
  aparecen al instante sin reiniciar el contenedor.
- **Quitar** un repo: borra la carpeta + lo desregistra.
- **Registrar** un git huérfano ya en disco (lee `origin` y rama actuales).
- **Configurar y ejecutar un deploy directo**: `git checkout <rama>` →
  `git pull --ff-only` → comando de build en cada `buildDirs` del repo
  (login shell del host). Es un deploy **local/staging**: checkout + pull +
  build en el mismo host donde corre el panel. **No es deploy a un VPS ni
  pipeline CI/CD** (los stubs `feature_stub("deploy")` lo dejan claro).
- **Abrir VSCode** con un `.code-workspace` multi-root generado una vez.
- **Abrir terminal** en la carpeta del proyecto con `wp` listo.

## Precondiciones

- **`gh`** instalado para clonar (`github.rs::22` chequea
  `gh --version`). Sin `gh`, `gh_clone` no funciona, pero `gh_pull`,
  `gh_scan`, `gh_register`, `gh_set_deploy`, `gh_deploy` y `gh_branch_status`
  sí (operan con `git` directamente).
- **`gh auth login`** previo: el panel no autentica; usa la sesión
  existente. `gh_status` reporta `authenticated` parseando la salida de
  `gh auth status` (`github.rs::parse_user` busca `account NAME` y
  fallback `as NAME`).
- **SSH keys / HTTPS token del usuario** ya configuradas en el host; igual
  que para `gh`, no hay tokens propios.
- **VSCode** (`code`), **VSCodium** (`codium`), **code-insiders** o
  **vscodium** en PATH para `open_vscode`
  (`github.rs::open_vscode`, `candidates`).
- **Algún emulador de terminal**: `konsole`, `gnome-terminal`,
  `xfce4-terminal`, `kitty`, `alacritty` o `x-terminal-emulator`
  (`cli.rs::open_terminal_at`).
- **El repo debe existir en disco** (clonado o registrado) para los flujos
  `pull` / `branch_status` / `deploy`. `clone` lo crea; `register` solo
  lee `remote` y `branch` de uno ya presente.

## Flujo feliz (numerado)

### Detectar repos

1. UI: tab "GitHub" en `ProjectDetail.svelte` → `loadGh()`
   (`ProjectDetail.svelte:479`) llama `api.ghStatus()` y `api.ghScan(id)`.
2. `gh_scan` → `github::scan(site)` (`github.rs::475`):
   - Lee `wp-content/`, profundidad 4 (`find_git_dirs`), sin descender en
     `node_modules`, `vendor`, `.git`.
   - Por cada `.git/` encontrado: `git remote get-url origin` + `git
     rev-parse --abbrev-ref HEAD`.
   - Marca `registered` comparando `path` con `github.repos[].path`.
3. UI: cada repo se muestra con nombre, path, rama, remoto, y badge
   registrado (gris) o huérfano (ámbar). Botones: «Registrar» (huérfanos),
   «Pull», «Deploy ▾», «✕».

### Clonar repo nuevo

1. UI: input `owner/repo` + `branch` + select `plugin|theme|muplugin` +
   ruta custom opcional. `cloneKind` propone la ruta
   (`github::propose_path`, `github.rs::383`: `wp-content/{sub}/{name}`).
2. `gh_clone` (`lib.rs::809`):
   - `github::clone(site, repo, branch, rel_path)` corre
     `gh repo clone {repo} {dest} -- -b {branch}` (los flags `-b` se pasan
     tras `--`).
   - Fallo claro si la carpeta destino ya existe.
   - Añade `GithubRepo { repo, branch, path: rel_path, build_cmd: None,
     build_dirs: [] }` a `github.repos` y persiste.
3. UI refresca: el repo aparece como registrado.

### Pull

1. UI botón «Pull» o «Pull todo»:
   - `gh_pull(id, path, branch)` → `github::pull`
     (`github.rs::119`): `git -C {dir} pull [origin {branch}]`.
   - `gh_pull_all(id)` itera `github.repos` y concatena resultados.
2. Los cambios en disco los refleja el bind-mount del container php
   (`/var/www/html → app/public`); sin reinicio necesario.

### Registrar huérfano

1. UI botón «Registrar» sobre un repo detectado no registrado:
   `gh_register(id, path)` (`lib.rs::876`):
   - `github::read_repo_meta` (`github.rs::508`) lee `origin` y rama actual.
   - Si ya está registrado, no-op.
   - Persiste `GithubRepo` con `build_cmd = None`, `build_dirs = []`.

### Estado de rama

1. UI panel expandible «Deploy ▾» → botón «Ver estado»:
   `gh_branch_status(id, path, branch)` (`lib.rs::890`) →
   `github::branch_status` (`github.rs::191`):
   - `git fetch --quiet origin`.
   - `git status --porcelain` → `dirty`.
   - `git rev-list --left-right --count HEAD...origin/{branch}` →
     `ahead` / `behind`.
   - `summarize` (`github.rs::232`, pura, testeable): arma `can_pull` y
     `message`.
2. UI muestra `Rama actual`, `objetivo`, `↓ behind`, `↑ ahead`, `dirty`,
   mensaje y colores según `can_pull`.

### Configurar deploy directo

1. UI: input «rama», «comando de build» (`pnpm install && pnpm build`,
   etc.) y checkboxes de `dirCandidates`. `gh_build_dirs(id, path)`
   (`lib.rs::927`) → `github::build_dir_candidates` (`github.rs::342`)
   devuelve la raíz (`""`) si tiene `package.json` y subcarpetas de nivel
   1 con `package.json`, excluyendo `node_modules`, `.git`, `vendor`.
2. `gh_set_deploy(id, path, branch, buildCmd, buildDirs)`
   (`lib.rs::898`): persiste `branch`, `build_cmd` (vacío/None si no hay),
   `build_dirs` (lista limpia de `/` inicial/final) en el `GithubRepo`
   correspondiente.

### Deploy directo (staging)

1. UI botón «Pull + build» o CLI `wordpress-panel-cli git deploy
   [--path <p>]` → `gh_deploy(id, path)` (`lib.rs::935`) →
   `github::deploy` (`github.rs::255`):
   - `log` «▶ Deploy de {rel_path} (rama {branch})…» al `op-log`.
   - Si `branch` no vacío: `git checkout {branch}`; aborta si el árbol está
     sucio.
   - `git pull --ff-only [origin {branch}]`; aborta si diverge (no
     fast-forward; se delega al editor).
   - Si `build_cmd` presente y no vacío: por cada `build_dirs` (vacío =
     raíz, se trimea `/`):
     - `wd = dir.join(sub)`; aborta si la subcarpeta no existe.
     - `shell = std::env::var("SHELL").unwrap_or("sh")`.
     - `shell -lc {cmd}` (login shell para cargar nvm/node/pnpm/etc.).
     - Cada línea de stdout/stderr se emite al op-log con prefijo `  `.
     - Si el código de salida ≠ 0, aborta.
   - «✓ Deploy de {rel_path} listo».
2. Como es en host, los artefactos del build quedan en disco y los sirve
   nginx/php al instante (mismo bind-mount).

### VSCode

1. UI botón «Abrir en VSCode»:
   `open_vscode(id)` (`lib.rs::952`) →
   `github::ensure_workspace(site)` (`github.rs::540`):
   - Si es worktree-project: workspace con `wt/{basename}` como única
     carpeta (el `public` del padre está vacío en el worktree).
   - Si no: `app/public` como carpeta principal + cada repo git detectado
     como carpeta adicional (multi-root).
   - Nombre del archivo: `{nombre}.code-workspace` (caracteres no
     alfanuméricos se reemplazan por `-`). No se sobreescribe si ya
     existe: el usuario puede editarlo a mano.
2. `github::open_vscode(ws)` prueba `code`, `codium`,
   `code-insiders`, `vscodium` en orden y hace `spawn` (proceso detached).

### Terminal con `wp` listo

1. UI botón «Abrir terminal del proyecto» (`ProjectDetail.svelte:1109`):
   `open_terminal(id)` (`lib.rs::457`):
   - `cli::install_cli_wrapper` (idempotente; copia `wp` y
     `wordpress-panel-cli` a `~/.local/bin` con `chmod 755`).
   - `cli::open_terminal_at(path)` prueba `konsole`, `gnome-terminal`,
     `xfce4-terminal`, `kitty`, `alacritty`, `x-terminal-emulator` con el
     flag que fija el cwd (`--workdir`, `--working-directory`, etc.).
2. Dentro de la terminal, `wp ...` detecta el proyecto por CWD
   (`scripts/wp-wrapper.sh`): `wordpress-panel-cli detect-project "$PWD"` →
   `docker exec -i --user www-data wp-{pid} php /usr/local/bin/wp --path=/var/www/html ...`.

## Variantes y casos borde

- **`gh` no instalado**: `gh_status.installed = false`; UI avisa con el
  comando de instalación. `gh_clone` falla con error literal de `gh`.
- **`gh` sin sesión**: `gh_status.authenticated = false`; UI avisa con
  `gh auth login`. `gh_clone` falla con error de `gh auth`.
- **Pull con árbol sucio**: `gh_deploy` aborta con «cambios sin commitear:
  haz pull desde el editor para resolverlos».
- **Pull no fast-forward**: `gh_deploy` aborta con «la rama diverge del
  remoto: resuélvelo desde el editor».
- **Build con error**: aborta con código de salida; muestra stderr. El
  sitio sigue funcionando con la versión anterior.
- **Rama pegada con comando**: la UI tiene placeholder `feature/x` y
  valida formato. La CLI exige el nombre solo.
- **`buildDirs` incluye `node_modules`**: lo excluye `build_dir_candidates`
  en la detección. Si el usuario lo teclea a mano, `gh_set_deploy` lo
  deja pasar y `gh_deploy` aborta solo si la carpeta no existe.
- **Múltiples carpetas de build** (proyecto con `src` y `src-redesign`):
  el comando se ejecuta en cada una en orden
  (`github.rs::298-332`).
- **Sin comando de build**: `gh_deploy` hace checkout + pull y termina.
  Útil para repos sin paso de build (PHP puro sin front-end compilado).
- **Repo en path custom** (no bajo `wp-content/`): permitido vía
  `gh_clone(..., path?)` (`github.rs::propose_path` se ignora). El `path`
  debe ser relativo a `app/public/`. Si se sale de `wp-content/` para
  `gh_remove`, la guarda `if !canon.starts_with(&wp_content)` aborta.
- **`branch_status` sin remoto**: `has_remote = false`; `can_pull = false`;
  mensaje «No existe origin/{target} o falló el fetch: …».
- **Worktree-project**: el workspace de VSCode apunta al `git worktree`
  (rama), no al `public` del padre (que está vacío en el worktree).
- **Panel cerrado durante `gh_deploy`**: el watcher no se ve afectado; el
  build se ejecuta en el host y queda en disco.
- **VSCode no instalado**: error claro con sugerencia de instalar.

## Datos persistidos

- **`SiteConfig.github.repos`** (`config.rs::GithubConfig`): lista
  genérica de `GithubRepo { repo, branch, path, build_cmd?, build_dirs? }`.
- **Legacy `theme`/`plugins`**: `GithubConfig::normalize`
  (`config.rs::149`) los pliega en `repos` al cargar y los vacía
  (idempotente). La UI ya no los usa; se conservan solo para leer
  `config.json` antiguos.
- **Sidecar `dump-log.jsonl`**: este flujo no escribe dumps; los deploys
  son pulls del código del repo, no vuelcan la DB.
- **Workspace VSCode**: `{path}/{slug}.code-workspace`. Se crea una vez,
  editable a mano. Si el panel cambia el set de repos, el archivo NO se
  actualiza (es decisión del usuario).

## Containers y Docker

- **No se modifican containers** durante estas operaciones. `gh_clone`,
  `gh_pull`, `gh_register`, `gh_set_deploy`, `gh_deploy`, `gh_branch_status`
  son operaciones puras del host (subprocesos `git`/`gh`).
- **El deploy directo no reinicia el contenedor php**: los archivos se
  sirven en vivo por bind-mount. Si el build genera assets en una carpeta
  servida por nginx, el cambio es visible al instante; los assets servidos
  por PHP-FPM también.
- **`open_admin`, `open_site`, `open_folder`**: requieren el container
  activo los dos primeros (auto-login + URL), no el tercero.

## Fallos y compensaciones

- **`gh` no en PATH**: `gh_status.installed = false`; `gh_clone` falla
  con `gh: command not found`.
- **`gh auth` falla en `clone`**: stderr de `gh` se devuelve al frontend
  como error literal.
- **`git pull --ff-only` diverge**: error con sugerencia «resuélvelo desde
  el editor» (`github.rs::291`); la rama no se modifica.
- **Build con exit code ≠ 0**: aborta con el código (`github.rs::325`); el
  sitio sigue arriba con la versión anterior. Si el usuario quiere
  reintentar, debe arreglar el build (no hay rollback automático — los
  snapshots son el camino para volver atrás).
- **`git_remote`/`git_branch` fallan en repo corrupto**: `gh_scan` devuelve
  `remote: None`, `branch: None` y `registered: false`; UI lo muestra con
  «sin remoto». `gh_register` aborta si `git_remote` falla
  (`github.rs::513`).
- **VSCode no instalado**: error claro; el archivo `.code-workspace` se
  queda creado en disco (puede abrirse con otro editor).
- **`x-terminal-emulator` no disponible**: error con sugerencia de instalar
  uno de la lista.
- **`~/.local/bin` no en PATH**: `install_cli_wrapper` avisa; la terminal
  puede abrirse pero `wp` no se reconoce hasta añadirlo al PATH del
  shell.

## Superficies

### UI (SvelteKit, SPA)

- **`/site/[id]`** → tab «GitHub» en `ProjectDetail.svelte` (línea ~815+):
  estado de `gh`, lista de repos detectados, formulario «Clonar repo»,
  panel «Deploy ▾» expandible con rama, comando de build y carpetas,
  botones «Ver estado» y «Pull + build», botón «Abrir en VSCode».
- **`/site/[id]`** → tab «Servicios» → botón «Abrir terminal del proyecto»
  (`ProjectDetail.svelte:1109`).
- **`/` dashboard**: la cabecera de cada proyecto tiene los accesos rápidos
  cuando está activo (los botones de admin/site/folder). El flujo de Git/
  deploy está en el detalle, no en el master.

### IPC (Tauri commands en `lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `gh_status` | — | `github::status` |
| `gh_scan` | `id` | `github::scan`; escanea `wp-content/` |
| `gh_clone` | `id, kind, repo, branch, path?` | `github::clone`; `path` opcional sobreescribe la ruta propuesta |
| `gh_pull` | `id, path, branch` | `github::pull` |
| `gh_pull_all` | `id` | Pull de todos los registrados |
| `gh_remove` | `id, path` | `github::remove_dir` + desregistrar |
| `gh_register` | `id, path` | `github::read_repo_meta` + persiste |
| `gh_branch_status` | `id, path, branch` | `github::branch_status` |
| `gh_set_deploy` | `id, path, branch, buildCmd?, buildDirs[]` | Persiste config de deploy |
| `gh_build_dirs` | `id, path` | Candidatos de build para el selector |
| `gh_deploy` | `id, path` | `github::deploy`; emite `op-log` |
| `open_vscode` | `id` | `github::ensure_workspace` + `open_vscode` |
| `open_terminal` | `id` | `cli::install_cli_wrapper` + `open_terminal_at` |
| `install_cli_wrapper` | — | `cli::install_cli_wrapper` |

`api.ts` (`src/lib/api.ts`) expone los espejos.

### CLI (`scripts/wordpress-panel-cli.sh`)

Habla con el panel por D-Bus:

- `git scan` → `GhScan` (dbus.rs).
- `git status --path <p> --branch <b>` → `GhBranchStatus`.
- `git pull --path <p> --branch <b>` → `GhPull`.
- `git set-deploy --branch <b> [--build "<cmd>"] [--dirs a,b,c]` →
  `GhSetDeploy` (`build_dirs_csv` separado por comas).
- `git deploy [--path <p>]` → `GhDeploy`.
- `open folder` (no requiere D-Bus; usa `xdg-open`).
- `install_cli_wrapper` no es subcomando CLI; lo ejecuta el panel al
  arrancar (`lib.rs::974`) y el botón «Solo instalar wrapper `wp`» en
  Configuración.

`detect-project <ruta>` resuelve el proyecto por prefijo de `path` en el
config.json; `git_target_path` infiere la ruta del repo desde `git rev-parse
--show-toplevel` cuando se omite `--path`.

### MCP (`mcp/server.mjs`)

Catálogo:

- `git_scan`, `git_status`, `git_pull`, `git_set_deploy`, `git_deploy`
  (todos `needProject: true` → resuelven a la carpeta del proyecto).

### D-Bus (`src-tauri/src/dbus.rs`)

- `GhScan(id)`, `GhPull(id, path, branch)`, `GhBranchStatus(id, path,
  branch)`, `GhBuildDirs(id, path)`, `GhSetDeploy(id, path, branch,
  buildCmd, buildDirsCsv)`, `GhDeploy(id, path)`. Sin método
  `gh_clone`/`gh_register`/`gh_remove`/`gh_status` por D-Bus: esos flujos
  requieren UI (clonar necesita token de gh interactivo si la sesión es
  muy vieja) o solo se usan desde la UI.

## Tests

- `github::tests::summarize_estados`: cubre 5 estados del resumen de
  rama (sin remoto, dirty, al día, por traer, por delante también).
- `config::tests::github_normalize_pliega_legacy_en_repos`: valida que
  `theme`/`plugins` legacy se pliegan en `repos` y no se serializan.
- `integration_tests.rs` cubre `gh_clone` con un repo local de prueba (no
  contra GitHub real). El deploy con build real se valida manualmente.

## Límites conocidos

- **Sin auth propia**: si la sesión de `gh` expira, hay que correr
  `gh auth login` aparte. El panel no lo detecta en mitad de un clone.
- **Deploy no es CI/CD**: el «deploy directo» es **solo staging local**;
  hace checkout + pull + build en el host. Para deploy a producción hace
  falta CI/CD externo (Cloudflare Tunnel, GH Actions, etc.); el botón
  está stubbeado como `feature_stub("deploy")`.
- **No hay hooks pre/post-deploy**: el flujo es lineal
  (checkout → pull --ff-only → build en cada carpeta). Si el build borra
  archivos (p. ej. `pnpm clean`), se hace en el orden configurado.
- **Workspace VSCode no se regenera**: si se añade un repo nuevo, hay que
  borrar el `.code-workspace` o editarlo a mano.
- **`build_dir_candidates` es superficial**: solo busca `package.json` en
  raíz y subcarpetas de nivel 1. Builds en `tools/build/` o más profundos
  no se sugieren (hay que teclearlos).
- **No hay resolución de conflictos**: si el pull diverge o el árbol
  está sucio, el deploy aborta. El usuario tiene que resolverlo desde el
  editor o con `git` manual.
- **`gh_clone` no pide la categoría si el repo no existe**: solo falla
  con stderr de `gh`.

## Invariantes y recomendación rebuild

- **`github.repos` solo contiene paths que existen en disco**: `gh_remove`
  borra la carpeta antes de desregistrar; `gh_scan` puede listar huérfanos
  (que `gh_register` promueve a `repos`).
- **`build_cmd = None` o cadena vacía = no se ejecuta build**: deploy
  reducido a checkout + pull.
- **El panel no toca el remoto**: nunca hace `git push`, `git remote
  set-url`, ni crea branches remotas. Solo fetch y pull.
- **El workspace VSCode sobrevive a cualquier rebuild**: el panel nunca
  sobreescribe un `.code-workspace` existente (`github.rs:565`).
- **Rebuild desde cero**: borrar el `config.json` y reconstruirlo perdería
  `github.repos`. La lista se puede re-poblar escaneando (`gh_scan`) +
  registrando cada huérfano (`gh_register`); los repos no-clonados se
  tienen que volver a clonar.

## Fuentes

- `src-tauri/src/github.rs` (todo el flujo de `gh`/`git`)
- `src-tauri/src/config.rs` (`GithubConfig`, `GithubRepo`, `normalize`)
- `src-tauri/src/lib.rs` (comandos `gh_*`, `open_vscode`, `open_terminal`,
  `install_cli_wrapper`)
- `src-tauri/src/cli.rs` (`install_cli_wrapper`, `open_terminal_at`)
- `scripts/wp-wrapper.sh`, `scripts/wordpress-panel-cli.sh`
- `src/lib/components/ProjectDetail.svelte` (tabs GitHub y Servicios)
- `src/lib/api.ts` (espejos JS)
- `src/routes/cli/+page.svelte` (documentación de la CLI)
- `mcp/server.mjs` (herramientas MCP)
- `src-tauri/src/dbus.rs` (métodos `Gh*`)
- `docs/ARCHITECTURE.md` (sección «GitHub vía `gh`»)
