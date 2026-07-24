# 10 · Worktree-projects

Cubre la creación y eliminación de proyectos de prueba atados a una rama de
un repo (theme/plugin) del proyecto padre. **No se duplica el código**:
el `public` del padre se comparte por **montaje Docker** y se sobreponen
solo el repo objetivo (un `git worktree` sobre una rama nueva) y un
`wp-config.php` propio.

## Resultado para el usuario

- **Crear** un worktree-project del repo `target_path` (relativo a
  `app/public/`) sobre una rama nueva. El sitio resultante:
  - Comparte el `app/public` del padre por montaje Docker (todos los
    archivos del padre son visibles).
  - Sobrepone solo el repo objetivo, que es un `git worktree` sobre
    `branch` en `{path}/wt/{basename}`.
  - Lleva un `wp-config.php` propio con el dominio y la BD del worktree.
  - Tiene una URL propia (`{slug}.test`).
  - Puede **compartir la BD del padre** (vía constantes `WP_HOME` /
    `WP_SITEURL`) o usar una **copia propia** (dump + import del padre).
- **Listar** los worktrees de un proyecto padre.
- **Eliminar** un worktree-project: `git worktree remove`, drop del esquema
  si era copia, borrar carpeta. **La rama queda en el repo del padre** (a
  menos que se pida borrarla).

## Precondiciones

- El proyecto padre debe existir (`SiteConfig` registrado).
- El `target_path` (relativo a `public/`) debe contener `.git/`: la
  creación aborta si no es un repo git
  (`worktree.rs::run_create`, `if !repo_dir.join(".git").exists()`).
- El nombre de la rama debe ser válido (`worktree.rs::invalid_branch_reason`
  cubre espacios, `-` inicial, `..`, y caracteres prohibidos por
  `git-check-ref-format`). Pegar el comando entero (`git checkout -b ...`)
  sugiere la rama extraída con `guess_branch`.
- Para `shared_db = false`: la DB del padre debe estar accesible (el motor
  DB compartido corriendo). `ensure_db` ya lo garantiza.
- El worktree-project se monta desde el padre; **no** necesita que el
  padre esté encendido, solo que exista en disco.

## Flujo feliz (numerado)

### Crear un worktree-project

1. UI: tab «GitHub» en `ProjectDetail.svelte` → formulario «Nuevo
   worktree» (`ProjectDetail.svelte:1049`): select de `detected` (path),
   input `rama nueva`, input `base (opcional)`, checkbox «Compartir la BD
   del proyecto principal». `wtSharedDb = true` por defecto.
2. `createWorktree` (`ProjectDetail.svelte:498`) →
   `api.createWorktreeSite(id, wtTargetPath, wtBranch.trim(), wtSharedDb,
   wtBaseBranch.trim() || undefined)` →
   `create_worktree_site` (`lib.rs::756`) →
   `worktree::create_worktree` (`worktree.rs::37`).
3. `worktree::run_create` (`worktree.rs::56`) emite por `op-log`:
   - `▶ Creando worktree «{branch}» de «{parent.name}» ({target_path}).`
   - `  Dominio: {domain} · BD: {compartida|copia}.`
   - Si la rama es inválida, aborta con sugerencia de la rama extraída.
   - `[1/7] Preparando carpeta del worktree…`:
     `create_dirs(&site)` + `write_php_ini(&site)` +
     `mkdir wt` + `write wp-config.php` (con `<?php\n` inicial; debe
     existir como archivo antes del bind, si no Docker crea un directorio)
     + `write_site_config`.
   - `[2/7] Creando git worktree → rama «{branch}»…`:
     `add_worktree(repo, dest, branch, base)` (`worktree.rs::348`):
     - `git worktree prune` primero (intentos anteriores fallidos pueden
       dejar el dest «missing but already registered»).
     - Intento 1: `git worktree add -b {branch} {dest} {base?}`.
     - Intento 2 (rama ya existe): `git worktree add {dest} {branch}`.
     - Si ambos fallan, error con ambos stderr.
   - `[3/7] Base de datos: compartida con el padre (sin copia).` o
     `… creando esquema propio y copiando del padre…`:
     - `ensure_db` para arrancar el motor.
     - Si `shared_db`: nada más.
     - Si no: `create_database({slug}_db)` + `backup::dump_bytes(parent)`
       + escribir `app/sql/from-parent-{ts}.sql` +
       `migrate::import_dump(site, dump_path)`.
   - `[4/7] Generando certificado SSL para {domain}…` o «SSL desactivado».
   - `[5/7] Arrancando el worktree (container PHP + nginx)…`:
     `docker::start_site(site)`. Esto crea el container con el branch
     `worktree_of` (`docker::create_php_container`, `docker.rs:719-750`)
     que monta:
     - `{parent_public}:/var/www/html` (raíz, rw).
     - `{wt_target}:/var/www/html/{target_path}` (override del repo).
     - `{wt_wp_config}:/var/www/html/wp-config.php` (override de wp-config).
     - `{php_ini}:/usr/local/etc/php/conf.d/zz-project.ini:ro`.
     - `{wp_cli_phar}:/usr/local/bin/wp:ro`.
     - Docker ordena los binds por profundidad del destino, así el padre
       (raíz) se monta antes y los overrides quedan encima.
     - El vhost nginx (`nginx::render_vhost`, `nginx.rs:53-69`):
       `root /srv/projects/{parent}/app/public` (estáticos del padre);
       bloque adicional `location ~ ^/{target}/(.+\.(css|js|img…))$
       { alias /srv/projects/{dirname}/wt/{basename}/$1; }` para servir
       los assets del repo objetivo desde el `git worktree`.
   - `[6/7] Escribiendo wp-config.php del worktree…`:
     `wp_config_create(site, db_container)`. Si `shared_db`:
     `wp config set WP_HOME {url} --type=constant` y `WP_SITEURL` igual.
     Esto sobrescribe el dominio en tiempo de ejecución **sin mutar la DB
     del padre** (las constantes tienen precedencia sobre las opciones
     serializadas).
   - `[7/7] Ajustando URLs de la copia a {domain}…` (solo si no es
     compartida): `migrate::fix_site_url` con `--skip-plugins --skip-themes`.
     Si falla, `⚠` y se sigue.
4. Si algo falla a medias, el orquestador limpia: `remove_container`,
   `nginx::remove_vhost`, `remove_dir_all(site.path)` y devuelve el
   error original (`worktree.rs:263-269`).
5. La UI navega al detalle del worktree (`onSelect(wt.id)`). El badge `W`
   violeta aparece en el dashboard; en el detalle, una banda violeta
   recuerda que es un worktree y que se elimina desde el proyecto padre.

### Listar worktrees

- `list_worktrees(parentId)` (`lib.rs::795`) →
  `worktree::list_worktrees(parent_id)` (`worktree.rs::334`): filtra
  `load_all_sites()` por `worktree_of.parent_id == parent_id`.

### Eliminar un worktree-project

1. UI: desde el proyecto padre (en la lista de worktrees del tab GitHub) o
   desde el detalle del worktree (`removeWorktree` en
   `ProjectDetail.svelte:520`): `api.removeWorktreeSite(id, false)`.
2. `remove_worktree_site` (`lib.rs::782`) →
   `worktree::remove_worktree` (`worktree.rs::276`) emite por `op-log`:
   - `▶ Eliminando worktree «{branch}».`
   - `Apagando el worktree…`: `docker::stop_site` + `remove_container`.
   - `Quitando el git worktree (la rama se conserva)…`:
     `remove_git_worktree(repo, dest)` con `git worktree remove --force`;
     si la carpeta ya no está, hace `git worktree prune`.
   - `Borrando la rama «{branch}»…` (solo si `delete_branch = true`):
     `git branch -D {branch}`.
   - `Borrando el esquema «{db_name}»…` (solo si `shared_db = false`):
     `ensure_db` + `wordpress::drop_database` + `teardown_unused_shared`.
     **Nunca** si era compartida (sería la DB del padre).
   - `Borrando la carpeta del worktree…`: `remove_dir_all(site.path)`.
   - `✓ Worktree eliminado. La rama sigue en el proyecto principal.`
3. `SiteConfig` desaparece del panel; la rama queda accesible en
   `git branch` del repo del padre.

## Variantes y casos borde

- **Rama pegada con comando** (`git checkout -b feature/x`):
  `invalid_branch_reason` devuelve «contiene espacios»; `guess_branch`
  extrae `feature/x` y el error sugiere esa rama.
- **Rama ya existe**: `add_worktree` intenta crear con `-b`, falla, y
  reintenta con `worktree add {dest} {branch}` (checkout de la existente).
- **`base_branch` vacía**: el `add_worktree` no pasa base; la rama se crea
  desde el HEAD actual del repo.
- **Copia de DB** (shared_db=false): el dump del padre se guarda como
  `app/sql/from-parent-{ts}.sql`; útil para diagnóstico. El nombre sigue
  el patrón `from-parent-YYYYMMDD-HHMMSS.sql`.
- **Watchdog de import** (3 min sin avance): la copia falla igual que en
  migración, con `reset_database`. Reintentar desde el padre (borrar el
  worktree + crear uno nuevo) es el camino más simple.
- **Workspace VSCode del worktree** (`github::ensure_workspace`,
  `github.rs:551`): apunta al `wt/{basename}` (la rama nueva), no al
  `public` del padre (que está vacío en la carpeta del worktree). El
  `open_vscode` del worktree abre esa rama directamente.
- **Eliminar padre con worktrees vivos**: `delete_site(parent, false)`
  deja la carpeta del padre desconectada y los worktrees **siguen
  funcionando** (la composición por montajes sigue resolviendo el padre
  por `parent_dirname`). Al re-importar el padre, los worktrees vuelven a
  la lista. Si el padre se borra con carpeta (`delete_site(parent,
  true)`), los worktrees quedan con un padre inexistente y los montajes
  Docker fallarán (el `remove_container` puede dejar el container
  inconsistente).
- **Conflicto de slug**: `find_free_slot` (`worktree.rs:481`) prueba
  `{parent_dirname}-{branch_slug}`, `-1`, …, `-99`, y como fallback
  concatena un UUID corto.
- **`target_path` con espacios o caracteres raros**: `path_basename` toma
  el último segmento; `trim_matches('/')` limpia extremos.
- **Sin `wp-config.php` propio antes de montar**: el orquestador escribe
  `<?php\n` antes de `write_site_config` para que el bind monte un
  archivo, no un directorio (`worktree.rs:175`).
- **DB compartida con la copia**: NO permitida por el flujo; cada
  worktree con `shared_db=true` usa el MISMO `db_name` que el padre. Si
  se crea uno con copia, su `db_name` es `{slug}_db`, no el del padre.

## Datos persistidos

- **`SiteConfig::worktree_of`**: `Option<WorktreeInfo>` (`config.rs:99`):
  - `parent_id`: id del proyecto padre.
  - `parent_dirname`: basename de `parent.path` (la ruta del padre puede
    haber cambiado entre PCs; el dirname es estable mientras la carpeta
    exista en `~/panel-wp/`).
  - `target_path`: ruta del repo relativa a `app/public/`.
  - `branch`: nombre de la rama del worktree.
  - `shared_db`: true = comparte esquema del padre; false = copia propia.
  - `created_at`: ISO 8601.
- **Estructura del worktree-project**:
  - `{path}/wt/{basename}/` → `git worktree` del repo (la rama nueva vive
    en el repo del padre, no aquí — el directorio es solo el checkout de
    trabajo).
  - `{path}/wp-config.php` → wp-config propio con dominio y BD del
    worktree.
  - `{path}/app/public/` → vacío (los archivos se sirven del padre vía
    bind-mount; este directorio existe porque la composición de mounts lo
    requiere y porque `create_dirs` lo crea).
  - `{path}/app/sql/from-parent-{ts}.sql` → dump del padre si fue copia.
  - `{path}/config.json` → `SiteConfig` con `worktree_of` poblado.

## Containers y Docker

- **Una sola composición por worktree**: container `wp-{wtId}` con 5
  binds (`docker::create_php_container`, `docker.rs:711-777`):
  1. `{parent_public}:/var/www/html` (raíz rw, archivos del padre).
  2. `{wt_target}:/var/www/html/{target_path}` (override del repo).
  3. `{wt_wp_config}:/var/www/html/wp-config.php` (override de wp-config).
  4. `{php_ini}:/usr/local/etc/php/conf.d/zz-project.ini:ro`.
  5. `{wp_cli_phar}:/usr/local/bin/wp:ro`.
  - Sin publicación de puertos al host; nginx le habla por `panel-net`
    (igual que cualquier proyecto).
- **Nginx vhost** (`nginx::render_vhost`, `nginx.rs:53-69`):
  - `root /srv/projects/{parent_dirname}/app/public` (raíz del padre).
  - Bloque adicional:
    ```
    location ~ ^/{target}/(.+\.(css|js|mjs|png|jpe?g|gif|svg|ico|webp|woff2?|ttf|eot|map|json))$ {
      alias /srv/projects/{dirname}/wt/{basename}/$1;
      expires 7d;
      access_log off;
    }
    ```
    Va ANTES del `location /` para ganarle al match de regex.
- **DB**:
  - Compartida (`shared_db=true`): misma `db_name` y mismo engine. El
    `wp-config` propio define `WP_HOME` y `WP_SITEURL` como constantes
    para sobrescribir el dominio sin tocar la DB.
  - Copia (`shared_db=false`): esquema propio `{slug}_db`. El motor DB
    compartido (`panel-mysql-{ver}` o equivalente) está compartido; el
    orquestador hace `CREATE DATABASE` + `import_dump`.
- **UID/GID**: igual que en cualquier container del panel (alinea
  `www-data` al `PUID`/`PGID` del host vía entrypoint). Crítico para que
  el repo objetivo se pueda escribir desde el editor y desde
  WordPress.

## Fallos y compensaciones

- **`git worktree add` falla por dest registrado**: `worktree prune`
  antes del intento limpia la metadata (`worktree.rs:362`).
- **`git worktree add` falla por rama inválida**: el error es inmediato,
  antes de tocar la carpeta del worktree.
- **`wp_config_create` falla** (p. ej. permisos): el orquestador
  captura y limpia todo (container, vhost, carpeta).
- **El padre se borra mientras el worktree vive**: el bind-mount al
  padre queda apuntando a una carpeta inexistente. WordPress puede
  servir estáticos que sigan en el bind cacheado, pero cualquier lectura
  del WP (admin) puede fallar. La solución es borrar el worktree y
  recrearlo cuando el padre vuelva.
- **`delete_branch` con la rama en otro worktree**: `git branch -D`
  rechaza si la rama está checked out en otro worktree. El orquestador
  no lo detecta antes; el usuario recibe el error y debe quitar primero
  el otro worktree.
- **`shared_db=false` y la DB del padre se ha migrado**: la copia del
  dump es del momento de crear el worktree; cambios posteriores en la DB
  del padre NO se reflejan. Para re-sincronizar, borrar el worktree y
  recrearlo.

## Superficies

### UI (SvelteKit, SPA)

- **`/site/[id]`** → tab «GitHub» en `ProjectDetail.svelte` (línea ~960+):
  - Panel violeta «¿Qué es esto?» con explicación detallada
    (`ProjectDetail.svelte:986-1023`).
  - Lista de worktrees existentes (rama, targetPath, dominio, flag
    `BD compartida/copia`) con botones «Abrir» y «✕» (que avisa que la
    rama queda guardada).
  - Formulario «Nuevo worktree»: select con `detected[]`, input rama,
    input base opcional, checkbox `Compartir la base de datos…`,
    botón «Crear worktree».
  - Si el proyecto mismo es un worktree, se muestra una banda violeta
    arriba explicando que se elimina desde el padre.
- **`/` dashboard**: el badge `W` violeta aparece en la fila del
  worktree-project.
- **`OpConsole`** muestra el progreso de `create_worktree_site` y
  `remove_worktree_site`.

### IPC (Tauri commands en `lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `create_worktree_site` | `parentId, targetPath, branch, baseBranch?, sharedDb` | `worktree::create_worktree`; emite `op-log` |
| `remove_worktree_site` | `id, deleteBranch` | `worktree::remove_worktree`; emite `op-log` |
| `list_worktrees` | `parentId` | `worktree::list_worktrees` |

`api.ts` (`src/lib/api.ts`) expone los espejos.

### CLI (`scripts/wordpress-panel-cli.sh`)

Autodetecta el proyecto por el CWD y requiere el panel abierto:

- `worktree list` → `ListWorktrees(parent_id)` (dbus.rs). Imprime
  `id|name|domain` por línea (raw, sin JSON pretty).
- `worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]` →
  `CreateWorktree(parent_id, target_path, branch, base_branch, shared_db)`.
  `--copy-db` desactiva la compartición (`SHARED="false"`).
  Si no se da `--target`, se infiere del `git rev-parse --show-toplevel`
  del CWD (`git_target_path`-equivalente inline).
- `worktree remove <worktreeId> [--delete-branch]` →
  `RemoveWorktree(id, delete_branch)`.

### MCP (`mcp/server.mjs`)

Catálogo:

- `worktree_list(project)`
- `worktree_create(project, branch, target?, base?, copyDb?)`
- `worktree_remove(project, worktreeId, deleteBranch?)`

### D-Bus (`src-tauri/src/dbus.rs`)

- `ListWorktrees(parent_id)`: JSON `[{id,name,domain,branch,targetPath,
  sharedDb}]`.
- `CreateWorktree(parent_id, target_path, branch, base_branch, shared_db)`:
  JSON `{ok,id,domain}` o `{ok:false,error}`. Emite `sites-changed` al
  éxito.
- `RemoveWorktree(id, delete_branch)`: bool. Emite `sites-changed` al
  éxito.

## Tests

- `worktree::tests::valida_rama_y_sugiere`: pega
  `git checkout -b feature/franklinp/sc-8300/uws-new-page` → sugiere
  `feature/franklinp/sc-8300/uws-new-page`.
- `worktree::tests::slugify_ramas`: `feat/nueva-cabecera` →
  `feat-nueva-cabecera`, `BUGFIX_123` → `bugfix-123`, `///` → `wt`.
- `worktree::tests::find_free_slot_evita_colisiones`: colisión con
  `site-feat` → `site-feat-1`.
- `worktree::tests::path_basename_objetivo`:
  `wp-content/themes/mi-theme` → `mi-theme`, `wp-content/plugins/x/` →
  `x`.
- `nginx::tests` (módulo `nginx.rs`): valida que el vhost de un worktree
  tiene el bloque `alias` con `wt/{basename}/$1`.

## Límites conocidos

- **Solo repos bajo `wp-content/`**: `target_path` se interpreta como
  relativo a `app/public/`. Repos fuera de `wp-content/` (por ejemplo en
  la raíz del proyecto) requieren registro manual con `gh_register` antes
  y un `target_path` adecuado.
- **No hay «promover a principal»**: cuando la rama del worktree está
  lista, hay que mergearla manualmente con `git checkout main && git
  merge feature/x` desde el padre (o desde el editor). El panel no
  automatiza el merge.
- **Cambios en `wp-config.php` del padre**: si el usuario toca el
  `wp-config.php` del padre, el worktree no se entera (su wp-config es
  propio y se regeneró al crearse). Hay que regenerar el del worktree
  con `wp_config_create` (no expuesto directamente, pero se recrea
  borrando y volviendo a crear el worktree).
- **Plugins/theme activos en `wp-config`**: el `wp-config` propio fija
  `WP_HOME` y `WP_SITEURL` como constantes. Si un plugin tiene cache de
  URL hardcoded (p. ej. transients con el dominio), puede servir URLs
  viejas tras cambiar el dominio del worktree.
- **`shared_db=true` no es «solo lectura»**: la rama del worktree puede
  mutar la DB del padre vía WP admin (crear posts, cambiar opciones).
  Solo la home/siteurl queda aislada por constantes.
- **Múltiples worktrees del mismo repo**: permitido (cada uno con su
  rama); los `git worktree` se acumulan. `worktree prune` los limpia si
  los dirs desaparecen.
- **Eliminación parcial**: si el orquestador falla en `[5/7]`, hace
  rollback completo (container + vhost + carpeta); si falla en
  `[6/7]` o `[7/7]`, también. La rama queda creada (es el primer paso
  externo a Docker); se puede borrar manualmente con `git worktree
  prune` + `git branch -D`.
- **`drop_database` no es atómico**: si la DB del worktree se borró pero
  el `teardown_unused_shared` falla, el motor queda corriendo. Se apaga
  en el siguiente `stop_site` o al apagar todos los proyectos.
- **VSCode workspace del worktree** solo apunta al `wt/{basename}`. No
  incluye el `app/public` del padre (que es lo que normalmente se edita
  en el repo, no en el worktree).

## Invariantes y recomendación rebuild

- **`worktree_of.parent_dirname` es estable** mientras la carpeta del
  padre siga bajo el mismo basename en `~/panel-wp/`. Renombrar la
  carpeta del padre invalida los montajes hasta que se regeneren
  (`remove_container` + recrear).
- **Una rama por worktree**: `git worktree` solo permite un checkout por
  rama en toda la máquina. Si otra sesión (terminal manual del usuario)
  hace `git checkout {branch}` en el repo del padre, el worktree-project
  queda inconsistente.
- **`shared_db=true` no muta la DB del padre para nada**: solo lee
  opciones serializadas (que se ignoran por las constantes) y escribe
  contenido que aparece en el padre (posts, etc.). `shared_db=false` es
  el aislamiento real.
- **El panel nunca borra la rama al eliminar un worktree** (salvo
  `delete_branch=true`). El default es conservarla: el worktree es una
  «vista» sobre la rama, no la rama misma.
- **`add_worktree` prune defensivo**: si un intento anterior dejó el
  dest «missing but registered», `git worktree prune` lo limpia antes
  de reintentar.
- **Rebuild desde cero**: perder `~/panel-wp/{wt}/config.json` borra el
  registro del worktree; el `wt/{basename}` queda en disco como un
  `git worktree` huérfano. `git worktree list` en el repo del padre lo
  mostrará como «prunable»; `git worktree prune` lo limpia. Si la rama
  era importante, recuperarla de los `git worktree list` del padre antes
  de borrar.

## Fuentes

- `src-tauri/src/worktree.rs`
- `src-tauri/src/config.rs` (`WorktreeInfo`, `SiteConfig::worktree_of`,
  `worktree_root`, `worktree_wp_config`, `path_basename`)
- `src-tauri/src/docker.rs::create_php_container` (composición por
  montajes, branch `worktree_of`)
- `src-tauri/src/nginx.rs::render_vhost` (vhost con alias al wt)
- `src-tauri/src/lib.rs` (comandos `create_worktree_site`,
  `remove_worktree_site`, `list_worktrees`)
- `src-tauri/src/dbus.rs` (`ListWorktrees`, `CreateWorktree`,
  `RemoveWorktree`)
- `src/lib/components/ProjectDetail.svelte` (sección Worktrees en tab
  GitHub)
- `src/lib/api.ts` (espejos JS)
- `mcp/server.mjs`, `scripts/wordpress-panel-cli.sh`
- `docs/ARCHITECTURE.md` (sección «Worktree-projects»)
