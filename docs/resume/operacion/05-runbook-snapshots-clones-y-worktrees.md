# Runbook de snapshots, clones y worktrees

Este runbook amplía `03-runbook-proyectos.md` y `04-runbook-importacion-migracion-y-recuperacion.md` con los detalles que importan al crear, listar, restaurar y destruir puntos de guardado, clones temporales y worktree-projects. Cubre las tres superficies operativas (UI, CLI, MCP) y los riesgos asociados a cada flujo.

## Matriz de superficies

| Operación | UI | CLI | MCP | D-Bus |
|---|---|---|---|---|
| Crear snapshot | tab "Puntos de guardado" → "Punto de guardado" (menú "···") | `snapshot create "etiqueta"` | `create_snapshot(project, label)` | `CreateSnapshot(id, label)` |
| Listar snapshots | tab "Puntos de guardado" | `snapshot list` | `list_snapshots(project)` | `ListSnapshots(id)` |
| Borrar snapshot | botón "Borrar" en la fila del snapshot | `snapshot delete <id>` | `delete_snapshot(project, snapshotId)` | `DeleteSnapshot(id, snapshotId)` |
| Detectar excluibles | tab "Puntos de guardado" → "Exclusiones" | n/d | n/d | n/d |
| Guardar exclusiones | "Exclusiones" → "Guardar" | n/d | n/d | n/d |
| Crear clone desde snapshot | botón "Clonar desde aquí" | `snapshot clone <id>` | `clone_snapshot(project, snapshotId)` | `CreateClone(parentId, snapshotId)` |
| Listar worktrees | tab "GitHub" del padre | `worktree list` | `worktree_list(project)` | `ListWorktrees(parentId)` |
| Crear worktree-project | tab "GitHub" del padre → "Worktrees" → "Crear worktree" | `worktree create <rama> [--target ...] [--base ...] [--copy-db]` | `worktree_create(project, branch, target?, base?, copyDb?)` | `CreateWorktree(parentId, targetPath, branch, baseBranch, sharedDb)` |
| Eliminar worktree | botón "✕" en la fila del worktree | `worktree remove <id> [--delete-branch]` | `worktree_remove(project, worktreeId, deleteBranch?)` | `RemoveWorktree(id, deleteBranch)` |

## 1. Precondiciones universales

- `panel-net` existe.
- `dnsmasq` resuelve `*.test` a `127.0.0.1` (o la IP alterna del endpoint).
- `tar` con `--zstd` y `zstd` disponibles.
- `git` autenticado (claves SSH o `gh auth login`) si vas a tocar worktrees sobre repos remotos.
- Espacio en disco: un snapshot ocupa el tamaño del código (sin uploads/cache) + el dump de la DB.

## 2. Snapshots (puntos de guardado)

### 2.1. Crear

#### Procedimiento

1. UI: tab "Puntos de guardado" → botón "Punto de guardado" (en el menú "···") o directamente en la cabecera. Indica una etiqueta.
2. CLI: `wordpress-panel-cli snapshot create "antes del refactor"`.
3. MCP: `create_snapshot(project, label)`.

#### Lo que ocurre (`src-tauri/src/snapshot.rs::run`)

1. `[1/3]` `docker::ensure_db` arranca el motor DB si no estaba. No enciende `wp-{id}`; solo el motor. Si el motor ya corre, no hace nada.
2. `[2/3]` `backup::export_db_to` vuelca la DB con `mysqldump --single-transaction --no-tablespaces --skip-dump-date` (vía `docker::exec_capture` dentro del container DB) a `snapshots/{id}/db.sql`. La opción `--skip-dump-date` está ahí para que el dedup por hash del auto-dump no se rompa por la línea `Dump completed on <fecha>` que cambia en cada volcado.
3. `[3/3]` `tar --zstd -cf code.tar.zst` con exclusiones fijas (`./wp-content/uploads`, `./wp-content/cache`, `./wp-config.php`, `./*.log`) + `site.snapshot_excludes` (configurables en la UI, ruta relativa a `public/`). Corre desde el host sobre `site.public_dir()`.
4. `tar` con código 1 = "avisos no fatales" (file changed as we read it, típico en un WP activo con cache/logs mutando); se acepta y se loguea la primera línea del stderr. Código 2+ = error real, aborta y borra el directorio.
5. Escribe `meta.json` con `id`, `label`, `createdAt`, `dbName`, `dbType`, `codeBytes`, `dbBytes`, `excludes`.

#### Cambio esperado y evidencia

- `ls -la ~/panel-wp/{slug}/snapshots/{id}/` lista `code.tar.zst`, `db.sql`, `meta.json`.
- `OpConsole` muestra `✓ Punto de guardado listo — total en disco: NN MB`.
- El listado (UI/CLI/MCP) muestra la nueva entrada con tamaño en MB.

#### Abortar

- No hay botón de "abortar snapshot" en la UI. Si el `tar` se cuelga, matar `tar` desde la terminal no rompe el flujo (no se borra el snapshot parcial automáticamente). Reanuda borrando con `delete_snapshot` y vuelve a crear.

#### Recuperar

- Si `tar` falla con código 2+, `snapshot.rs::run` borra el directorio del snapshot. No queda nada huérfano.
- Si la DB se paró a mitad del dump, el `db.sql` queda completo (es el último paso antes de `tar`); el snapshot queda con un `db.sql` correcto y un `tar` parcial solo si el snapshot se creó dos veces seguidas.
- Si `meta.json` no se llega a escribir (p. ej. disco lleno), el listado no muestra el snapshot; pero la carpeta existe. Bórrala a mano o vuelve a crear.

#### Exclusiones (persistente en `config.json`)

- Tab "Puntos de guardado" → "Exclusiones" plegable.
- `snapshot::detect_excludable` escanea:
  - subcarpetas inmediatas de `wp-content` excepto `uploads` y `cache` (ya excluidas siempre);
  - carpetas de backup conocidas: `wp-content/updraft` (UpdraftPlus), `wp-content/ai1wm-backups` (All-in-One WP Migration), `wp-content/wpvividbackups` (WPvivid), `wp-content/backups-dup-lite` (Duplicator), `wp-content/backups-dup-pro` (Duplicator Pro), `wp-content/backuply` (Backuply), `wp-snapshots` (Duplicator).
- Cada entrada reporta tamaño y flag `known` (recomendado excluir).
- Pulsar **Guardar** persiste la lista en `config.json` (`site.snapshot_excludes`). Las exclusiones se heredan a los clones (`clone.rs::run` copia `parent.snapshot_excludes`).
- Las rutas añadidas a mano se normalizan: sin `./` inicial, sin `/` final, deduplicadas y ordenadas (`lib.rs::set_snapshot_excludes`).

#### Limitaciones

- El motor DB debe poder arrancar; en sistemas con `panel-mysql-*` eliminado manualmente, falla.
- `tar` no es transaccional: si el sitio escribe durante el snapshot, pueden quedar archivos a medio escribir. Por eso la exclusión de `cache/` y `*.log` minimiza la superficie ruidosa.
- `meta.json` se serializa con `serde_json::to_string_pretty`; si añades campos nuevos, son retrocompatibles si `serde` los marca con `default`.

#### Riesgos destructivos

- `delete_snapshot` borra el directorio del snapshot completo, no se puede deshacer. Confirmar antes.
- Las exclusiones no validan que la ruta exista en el snapshot: excluir algo inexistente no rompe pero confunde al revisor.

### 2.2. Listar y borrar

- `list_snapshots` ordena por `createdAt` descendente; no tiene paginación porque son ~decenas por proyecto.
- `delete_snapshot` valida que el directorio exista antes de `remove_dir_all`. Si el `meta.json` está corrupto, el listado lo ignora y `delete_snapshot` con el `id` igual falla; limpia manualmente `~/panel-wp/{slug}/snapshots/{id}/`.

## 3. Clones temporales desde un snapshot

### 3.1. Crear

#### Procedimiento

1. UI: en la lista de snapshots, botón "Clonar desde aquí". La creación no tiene opciones en la UI; el `meta.label` define el nombre del clone.
2. CLI: `wordpress-panel-cli snapshot clone <snapshotId>`.
3. MCP: `clone_snapshot(project, snapshotId)`.

#### Lo que ocurre (`src-tauri/src/clone.rs::run`)

1. Carga el padre (`config::find_site`) y parsea `meta.json` del snapshot.
2. Deriva el slug del clone: `{parent_dirname}-{label_slug}` con desambiguación `-N` (`clone::find_free_slot`); si choca con path o domain, prueba `-1`, `-2`, …, hasta `-99`; fallback con sufijo UUID de 8 chars.
3. Crea el SiteConfig: `id` uuid, `name = meta.label`, `domain = {clone_slug}.test`, `services` del padre, `db_name = {clone_slug}_db`, `clone_of = Some(CloneInfo)`. **Importante**: el `name` del clone es la etiqueta del snapshot, no `"{padre} (clone)"`. Esto cambia respecto a la primera versión del plan (ver `docs/CHANGELOG.md::Clones como sublista`).
4. Crea estructura: `wordpress::create_dirs`, `write_php_ini`, `config::write_site_config`.
5. Extrae `code.tar.zst` con `tar --zstd -xf` en `site.public_dir()`. Crea `app/public/wp-content/uploads/` vacío para que las subidas nuevas se guarden en el clone (no en el padre).
6. Crea la DB del clone: `docker::ensure_db` + `wordpress::create_database` (esquema separado `{clone_slug}_db`).
7. Importa el dump: `migrate::import_dump` con el `db.sql` del snapshot. Si el dump se cuelga, `IMPORT_IDLE_TIMEOUT` lo cancela y `reset_database` deja el esquema vacío (ver `04-runbook-importacion-migracion-y-recuperacion.md::[5/6]`).
8. Inyecta mu-plugins del panel (`wordpress::sync_mu_plugins`).
9. SSL si aplica.
10. Enciende container + vhost + nginx.
11. Genera `wp-config.php` con `wp config create` y luego `migrate::fix_site_url` con el dominio del clone.

#### Cambio esperado y evidencia

- `docker ps` muestra `wp-{clone-id}` y, si es el único activo de su motor, su DB compartida.
- La URL del clone (`{padre}-{etiqueta}.test`) abre WordPress con el código y la DB del snapshot.
- En el dashboard, el clone aparece con badge ámbar anidado bajo el padre.
- `~/panel-wp/{padre}-{etiqueta}/app/public/wp-content/uploads/` está vacío tras la creación.

#### Abortar

- El flujo no expone botón de abortar. Si la importación del dump se cuelga, `import_dump` la cancela por watchdog y devuelve error; el SiteConfig ya se escribió (se ve en la lista maestra como pendiente de algo) pero la DB quedó vacía. Para limpiarlo: `delete_site(clone_id, deleteFolder=true)`.
- Si `tar -xf` falla (snapshot corrupto), `clone::run` aborta y no enciende el container; el SiteConfig queda escrito pero sin SSL ni vhost. Borra a mano y vuelve a clonar tras recargar el snapshot.

#### Recuperar

- Si el dump era muy grande y `IMPORT_IDLE_TIMEOUT` (3 min) lo cortó, el error en consola indica "import cancelado: sin actividad por 3 min". Reintenta el clone (la DB ya está vacía y `import_dump` empieza de cero).
- Si nginx recarga con el vhost del clone antes de tener cert, falla. El paso `[6/8]` de la OpConsole muestra el error; ejecuta **Regenerar SSL** en el menú "···" del clone.

#### Limitaciones

- `try_files` en nginx cubre lectura web de media vieja, no lectura por filesystem desde PHP. `media-new` (clon) y `media-old` (padre, en `wp-content/uploads` ro) se sirven vía `try_files $uri @uploads_base` (`nginx::render_vhost` cuando `site.clone_of.is_some()`). Esto significa que los plugins que escanean el directorio de uploads del clone solo ven los archivos nuevos; `wp media regenerate` no recupera thumbs viejos.
- El clone no es multisesión: dos clones del mismo snapshot a la vez reciben esquemas DB distintos (por el slug) y paths distintos; nginx y php-fpm los sirven por dominio.
- Los uploads del clone se conservan al pausar (stop) y se borran al destruir (borrar). Documentado en `docs/CHANGELOG.md::Clones temporales`.

#### Riesgos

- `remove_dir_all` del clone (al eliminar) borra la carpeta completa, incluyendo dumps de DB que el clone haya podido generar.
- El nginx `try_files` con `^~` (`/wp-content/uploads/`) tiene precedencia sobre la location genérica de archivos estáticos. Si rompes esa location (modificando el vhost), las imágenes del padre pueden no servirse.

### 3.2. Destruir

- UI: usar el botón **Eliminar** en la tarjeta del clone (no hay acción específica de "destruir clone"); la `DeleteProjectModal` aplica el mismo flujo que cualquier proyecto (`lib.rs::delete_site` con `deleteFolder=true` recomendado).
- CLI/MCP: no hay subcomando directo. La UI es la vía.

`delete_site` apaga + quita vhost + `DROP DATABASE {clone_slug}_db` + `remove_dir_all`. El padre no se toca.

## 4. Worktree-projects

### 4.1. Crear

#### Procedimiento

1. UI: tab "GitHub" del proyecto padre → sección "Worktrees" → formulario "Nuevo worktree".
   - Rama: nombre DNS-safe (sin espacios, sin `..`, sin `^~:?*[\]`, sin empezar con `-`). Si pegas un comando entero (`git checkout -b feature/x`), `worktree::invalid_branch_reason` lo rechaza y `worktree::guess_branch` sugiere la rama extraída.
   - Target: ruta del repo relativa a `app/public/` (si no, infiere del CWD si lo abriste desde el editor).
   - Base: rama de la que partir (opcional; default = la actual del repo).
2. CLI: `wordpress-panel-cli worktree create feature/rama [--target wp-content/themes/mi-theme] [--base main] [--copy-db]`.
3. MCP: `worktree_create(project, branch, target?, base?, copyDb?)`.

#### Lo que ocurre (`src-tauri/src/worktree.rs::run_create`)

1. Carga el padre y verifica que `targetPath` apunte a un directorio con `.git/`.
2. Deriva slug: `{parent_dirname}-{branch_slug}` con desambiguación (`worktree::find_free_slot`).
3. Prepara carpeta: `wordpress::create_dirs`, `write_php_ini`, `worktree_root` (`{path}/wt/`), `worktree_wp_config` (`{path}/wp-config.php` con `<?php\n`).
4. `git worktree prune` (idempotente, evita "missing but already registered" de un intento anterior fallido).
5. `git worktree add -b {branch} {dest} [{base}]` con intento 1. Si la rama ya existe, intenta 2: `git worktree add {dest} {branch}` (checkout de la existente).
6. DB:
   - `shared_db=true` (default): usa el esquema del padre. `wp-config` propio define `WP_HOME` y `WP_SITEURL` como constantes que sobrescriben `home`/`siteurl` en tiempo de ejecución: la DB del padre sigue apuntando al dominio del padre; el navegador, al pedir el worktree, ve la home del worktree.
   - `shared_db=false` (`--copy-db`): `wordpress::create_database` crea el esquema, `backup::dump_bytes` del padre, `migrate::import_dump` aplica.
7. SSL si aplica.
8. `docker::start_site` con la rama `if let Some(wt) = &site.worktree_of` en `docker::create_php_container` (montajes de `parent_public`, `wt_target`, `worktree_wp_config`).
9. `wp config create` con las credenciales. Si `shared_db`, `wp config set WP_HOME` y `WP_SITEURL` con `--type=constant` apuntando a `endpoint::site_url(dominio, ssl)`.
10. Si `!shared_db`, `migrate::fix_site_url`.

#### Cambio esperado y evidencia

- `docker ps` muestra `wp-{worktree-id}`.
- `ls ~/panel-wp/{padre}-{rama}/wt/{target_basename}/` lista el worktree.
- `git -C {wt_target} worktree list` muestra el worktree.
- La URL (`{padre}-{rama}.test`) sirve WordPress con la rama activa en el repo objetivo.
- nginx sirve los estáticos del padre con un `location ~ ^/{target}/…(css|js|img…)$ { alias /srv/projects/{padre}-{rama}/wt/{basename}/$1; }` (`nginx::render_vhost` con `site.worktree_of`).

#### Abortar

- El `run_create` envuelve todo en un `build.await`; ante error, limpia container + vhost + carpeta (`worktree.rs::run_create::catch`). No deja un SiteConfig escrito si el worktree no se creó.

#### Recuperar

- Si la rama tiene espacios, el panel rechaza antes de tocar nada. Corrige el nombre.
- Si el repo está sucio, `git worktree add` puede fallar. Limpia desde el editor y reintenta.
- Si `shared_db=false` y la copia de DB falla, `reset_database` deja el esquema vacío; borra el worktree y vuelve a crearlo.

#### Limitaciones

- Los `wp-config` del worktree montan encima del `wp-config` del padre: si lo editas a mano desde el editor, el worktree usa el tuyo; si no, hereda el del padre. **No** edites `wp-config.php` dentro de `app/public` del padre pensando que afecta al worktree: no lo hace.
- El `git worktree` se almacena en `{path}/wt/{basename}`. Esa carpeta se monta encima de `app/public/{target}` en el container; los archivos del `public` del padre en esa ruta están "tapados" por el worktree.
- Si tu rama borra archivos del repo, el worktree los refleja. La rama del padre no se ve afectada.

#### Riesgos

- `git worktree add -b` puede fallar si la rama ya existe. El segundo intento (sin `-b`) hace checkout de la existente: si no querías eso, valida el nombre antes de enviar.
- El worktree-project comparte la DB con el padre (default): cualquier operación del worktree que modifique opciones de WP muta la DB del padre. Por eso `wp config set WP_HOME` y `WP_SITEURL` se hacen con `--type=constant` (escritos en `wp-config`, no en la DB).

### 4.2. Eliminar

- UI: botón "✕" en la fila del worktree; confirma con la nota "la rama se conserva en el proyecto principal".
- CLI: `wordpress-panel-cli worktree remove <id> [--delete-branch]`.
- MCP: `worktree_remove(project, worktreeId, deleteBranch?)`.
- D-Bus: `RemoveWorktree(id, deleteBranch)`.

#### Lo que ocurre (`src-tauri/src/worktree.rs::remove_worktree`)

1. `docker::stop_site` (que ya deja un dump fresco en `app/sql/db-*.sql` del worktree; el auto-dump del worktree se detiene por `AutoDump::stop`).
2. `docker::remove_container`.
3. `git worktree remove --force` desde el repo del padre. Si falla, `git worktree prune` para limpiar la metadata.
4. Si `deleteBranch=true`, `git branch -D {branch}`.
5. Si `!shared_db`, `wordpress::drop_database` del esquema del worktree. **NUNCA** si es compartida (sería la DB del padre).
6. `teardown_unused_shared`.
7. `remove_dir_all` de la carpeta del worktree.
8. Emite `sites-changed` por D-Bus para que la UI se recargue.

#### Cambio esperado y evidencia

- El worktree desaparece del dashboard.
- `git -C {target} worktree list` ya no lista el worktree.
- La rama del padre persiste (salvo `deleteBranch=true`).
- Si era `shared_db=false`, la DB del worktree ya no existe.

#### Abortar

- El flujo no expone botón de abortar. La operación es destructiva pero idempotente: si la vuelves a lanzar con un `id` que ya no existe, las llamadas a `docker::stop_site` y `docker::remove_container` son no-ops, y `git worktree remove` falla con stderr en vez de abortar (porque la rama ya está limpia).

#### Recuperar

- Si borraste el worktree equivocado y aún tienes la rama, recrea con `worktree create {branch}` (mismo `target`, sin `--copy-db` para ahorrar tiempo). El repo del padre sigue teniendo la rama.
- Si borraste el worktree equivocado y `deleteBranch=true` ya eliminó la rama, recupérala con `git -C {target} reflog` + `git branch {branch} <sha>`.

#### Limitaciones

- `remove_git_worktree` usa `--force` para sobrevivir a "el worktree tiene cambios sin commitear" — útil para limpieza, peligroso si el worktree tenía trabajo sin guardar.

## 5. Comparación de superficies

| Aspecto | Snapshot | Clone | Worktree |
|---|---|---|---|
| Costo en disco | 1 tar + 1 dump por snapshot | 1 schema DB extra + extr. de tar | 1 schema DB (opcional) + 1 carpeta `wt/` |
| Tiempo de creación | pocos segundos (excl. uploads/cache) | segundos a minutos (extr. tar + import) | segundos (git worktree add es rápido) |
| DB | misma que el padre (dump) | schema propio (por slug) | compartida o copia |
| Uploads | nuevos del clone en su carpeta; viejos desde el padre | nuevos en clone | nuevos en el `wt/` |
| Costo de borrado | borrar `snapshots/{id}/` | borrar carpeta + `DROP DATABASE` | `git worktree remove` + borrar carpeta |
| Reanudable | sí, re-ejecutar `create_snapshot` | sí, re-ejecutar `create_clone` | sí, re-ejecutar `create_worktree` |
| Aislamiento de DB | total (dump) | total (esquema separado) | parcial (solo `shared_db=false`) |
| Costo de CPU/RAM al usar | 1 php-fpm + 0 schema | 1 php-fpm + 0 schema extra | 1 php-fpm + 0/1 schema |

`teardown_unused_shared` apaga el motor DB compartido cuando ningún proyecto activo lo usa. Esto afecta a clones y worktrees con `shared_db=false` igual que a cualquier proyecto.

## 6. Diagnóstico rápido

- "No veo mi clone en la lista": revisa `~/panel-wp/` y busca carpetas anidadas. Los clones se anidan en el dashboard bajo su padre; el listado maestro los agrupa por `cloneOf.parentId`.
- "El worktree no carga el CSS nuevo": confirma que nginx ha recargado tras crear el worktree (`docker::start_site` ya lo hace) y que la rama no rompió la jerarquía de assets del repo. `git -C {wt_target} log --stat` ayuda.
- "El snapshot se queda en `code.tar.zst` parcial": `meta.json` no se escribe; el snapshot se considera fallido. `delete_snapshot` + crear de nuevo.
- "El clone tiene uploads viejos del padre que no se actualizan": los uploads viejos solo se sirven vía nginx `try_files`; los nuevos se guardan en el `wp-content/uploads` del clone. Si necesitas fusionar manualmente, cópialos a `~/panel-wp/{clone}/app/public/wp-content/uploads/` (rw) o usa la API de WP.

## 7. Criterio de salida

- La operación en cuestión devuelve sin error y la UI/CLI/MCP lo refleja.
- Los logs de nginx (`wordpress-panel-cli logs nginx -n 50`) muestran el vhost correcto cargado.
- Para clones/worktrees, el sitio del nuevo proyecto abre con el código y (si aplica) la DB esperados.

Pasar al ciclo de Git/CLI/MCP se cubre en `06-runbook-git-cli-y-mcp.md`.
