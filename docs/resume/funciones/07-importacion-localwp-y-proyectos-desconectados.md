# 07 · Importación desde LocalWP y proyectos desconectados

Cubre dos vías para meter proyectos preexistentes al panel: la importación
masiva desde LocalWP (`localwp.rs` + `migrate.rs`) y la re-importación de una
carpeta de `~/panel-wp/` que ya no está registrada en el panel (`disconnected`
preserved/reconstructed). Ambos flujos terminan en un proyecto con
`migrationPending=true` que se enciende con «Migrar y encender».

## Resultado para el usuario

- **Importar LocalWP**: trae los archivos (`app/public`) y el dump `local.sql`
  de un sitio de LocalWP a `~/panel-wp/{slug}/`, crea su `config.json` y lo
  deja como pendiente de migración. Tras pulsar «Migrar y encender», el panel
  recrea la DB, importa el dump y reescribe `home`/`siteurl` al dominio
  `.test` del panel.
- **Proyectos desconectados**: lista carpetas en `~/panel-wp/` que perdieron
  su `config.json`. Si conservan un sidecar `config.disconnected.json` la
  re-importación restaura exactamente la config original (`preserved`); si
  no, se reconstruye un `SiteConfig` mínimo a partir de `wp-config.php`
  (`reconstructed`, versiones PHP 8.3 / MySQL 8.0 por defecto). Tras
  re-importar queda como `migrationPending`.

## Precondiciones

- **LocalWP instalado y con `sites.json`** en `~/.config/Local/sites.json`.
  El panel **no** autodescubre sitios: lee este archivo literalmente
  (`localwp.rs::sites_json`, `read_raw`).
- **El sitio de LocalWP debe tener su dump en disco** (`app/sql/local.sql`).
  LocalWP no expone un endpoint para volcarlo en caliente; si no está, el
  panel avisa y deja el sitio sin DB (`note` en `ImportResult`). El import
  **no** hace `search-replace` de contenido (es una copia 1:1 del `.sql`);
  el ajuste de URLs lo hace `migrate::fix_site_url` vía `wp option update`
  tras importarlo.
- **El sitio de LocalWP debe tener su `app/public/`** (carpeta con el
  WordPress); sin ella `import_site` aborta
  (`localwp.rs::import_site`, `if !src_public.exists()`).
- **No debe existir ya una carpeta con el mismo slug** en `~/panel-wp/`
  (`localwp.rs::import_site`, `if dest.exists()`).
- **Para `disconnected`** la carpeta debe seguir bajo `~/panel-wp/` y haber
  perdido `config.json`. Si conserva el sidecar, la config se restaura
  completa; si no, debe existir al menos `app/public/wp-config.php`
  (`config.rs::list_disconnected_sites`).

## Flujo feliz (numerado)

### Importar desde LocalWP

1. `list_localwp_sites` (Tauri cmd en `lib.rs::349`) llama
   `localwp::list_sites` (`localwp.rs::106`). Parsea `sites.json` con un
   modelo tolerante (`RawSite` con `Default`), infiere `domain = "{slug}.test"`
   vía `wordpress::slugify`, marca `already_imported` si el `domain` o el
   `name` ya existe en `load_all_sites()`. Ordena por nombre.
2. La UI muestra cada sitio con PHP, MySQL, flag multisite/xdebug y un badge
   «Ya importado» si corresponde (`/import-localwp`).
3. Al pulsar «Importar», `import_localwp_site` ejecuta
   `localwp::import_site` (`localwp.rs::136`):
   - `crate::wordpress::create_dirs(&site)` (estructura base) +
     `write_php_ini`.
   - `cp_contents(&src_public, &site.public_dir())` (`localwp.rs::245`):
     `cp -a "{src}/." {dest}` para preservar atributos (recorrer en Rust un
     árbol WP grande es más lento).
   - Si existe `app/sql/local.sql`, copia como
     `app/sql/imported.sql` con tamaño en MB registrado por `progress::log`.
   - Genera `SiteConfig` con `migration_pending = true`, `group =
     Some("LocalWP")`, `xdebug_enabled` desde LocalWP, `services.php` y
     `services.db` ajustados a versiones soportadas (`PHP_SUPPORTED`,
     `MYSQL_SUPPORTED` en `localwp.rs::25`) con `pick_supported` (la última
     soportada si la del origen no lo está).
   - `write_site_config(&site)` (idempotente).
4. La UI abre `OpConsole` (`progress::log` va al canal `op-log`); al cerrar
   muestra el `note` con las advertencias (versión ajustada, falta de dump,
   multisite).
5. El proyecto aparece en `+page.svelte` con `status = migrationPending`.
   El usuario pulsa «Migrar y encender» → `migrate_site`
   (`lib.rs::155` → `migrate::migrate_site`):
   - `wordpress::sync_mu_plugins(site)` (inyecta auto-login y mailpit;
     los imports de LocalWP no los traen, un import de otra PC puede tenerlos
     desfasados).
   - `docker::ensure_db` + `wordpress::create_database` (engine + esquema
     vacío).
   - `ssl::generate` (cert mkcert por dominio).
   - `docker::start_site` (enciende php + vhost + reload).
   - `wordpress::wp_config_create` (credenciales del panel).
   - `migrate::latest_dump(site)` busca el `.sql` más reciente por mtime;
     `import_dump` lo aplica por `docker exec -i mysql ...` con pragmas de
     sesión (`SET autocommit=0; SET unique_checks=0; SET foreign_key_checks=0`).
   - `migrate::fix_site_url` (`migrate.rs::182`) actualiza `home`/`siteurl`
     con `--skip-plugins --skip-themes` (evita que un plugin colgado bloquee
     el fix).
6. El proyecto queda con `migration_pending = false` y
   `last_migrated_at` poblado.

### Re-importar proyecto desconectado

1. La UI abre `ImportProjectModal.svelte` con
   `api.listDisconnectedSites` → `config::list_disconnected_sites`
   (`config.rs::464`):
   - Escanea `~/panel-wp/*/`, ignora carpetas con `config.json`.
   - Si hay `config.disconnected.json` (`DISCONNECTED_CONFIG`,
     `config.rs::431`), lo lee y devuelve `kind = "preserved"`.
   - Si no, exige `app/public/wp-config.php` y devuelve
     `kind = "reconstructed"` con versiones por defecto
     (`DEFAULT_PHP = "8.3"`, `DEFAULT_DB = "8.0"`).
2. Al pulsar «Importar», `import_disconnected_site(folder_name)`
   (`lib.rs::236` → `import_disconnected`):
   - Lee el sidecar si existe y fija `cfg.path` al directorio actual (la
     carpeta pudo haberse movido entre PCs).
   - Si no, llama a `reconstruct_config(folder_name, dir)`
     (`lib.rs::308`): extrae `DB_NAME` del `wp-config.php` con
     `config::parse_db_name` (`config.rs::540`); fallback
     `{slug}_db`.
   - Si el `id` ya existe en `load_all_sites()` (carpeta duplicada),
     genera un UUID nuevo para evitar colisión.
   - `migration_pending = true`, `last_migrated_at = None`,
     `write_site_config`, elimina el sidecar si lo había.
3. Mismo cierre que LocalWP: el usuario hace «Migrar y encender» y
   termina con el sitio encendido.

## Variantes y casos borde

- **Versión PHP/MySQL no soportada**: `pick_supported` devuelve la última
  soportada y marca el flag `adjusted`, que se acumula en `note`
  (`localwp.rs::211-227`).
- **Sitio multisite**: `raw.multi_site` se interpreta como
  `!r.multi_site.is_empty()`; se avisa en `note` para revisión manual.
- **`sites.json` ausente**: `read_raw` devuelve `anyhow!("no se encontró …")`
  con la ruta exacta; la UI lo captura como `localError` y muestra el
  mensaje literal.
- **Sin dump**: `note` lo indica y el sitio queda `migrationPending`. Al
  migrar, `latest_dump` devolverá `None` y
  `migrate_site::run` reporta "No hay dump en app/sql/" sin abortar.
- **Carpeta desconectada con `app/public` faltante**: `import_disconnected`
  rechaza con `anyhow!("la carpeta … no contiene app/public")`.
- **Carpeta desconectada sin sidecar ni `wp-config.php`**: ni siquiera
  aparece en `list_disconnected_sites` (el `reconstructed` requiere
  `wp-config.php`).
- **Eliminar un proyecto sin borrar carpeta** (`delete_site(id, false)`):
  `config.json` se renombra a `config.disconnected.json`
  (`lib.rs::218`). La carpeta queda «desconectada»: ignorada por
  `load_all_sites`, pero re-importable. Es el camino inverso al import.
- **Id duplicado en re-import**: el `id` del sidecar puede chocar con un
  proyecto vivo; se regenera con `Uuid::new_v4` y se persiste el nuevo
  (`lib.rs::285`).
- **Auto-dump activo**: `delete_site` quita el contenedor y la DB del
  servidor compartido, pero el `app/sql/` del proyecto se conserva (no
  forma parte del servicio compartido). Si `deleteFolder=true`, también se
  borra.

## Datos persistidos

- **LocalWP**: nuevo `~/panel-wp/{slug}/config.json` con
  `migrationPending=true`, `group="LocalWP"`, `services.{php,db}` ajustados,
  `app/public/` (copia), `app/sql/imported.sql` (si había dump),
  `app/public/wp-config.php` original del sitio LocalWP (se regenerará en
  la migración).
- **Desconectado preserved**: restaura el `config.json` previo desde el
  sidecar, solo fija `path` al directorio actual y deja
  `migrationPending=true`.
- **Desconectado reconstructed**: `SiteConfig` mínimo con
  `services.php.version = "8.3"`, `services.db.version = "8.0"`, `type =
  Mysql`, `db_name` extraído del `wp-config.php` (o derivado del slug).
- **Sidecar**: `config.disconnected.json` vive junto a
  `config.json`; solo existe para carpetas desconectadas. Se elimina en la
  re-importación (`lib.rs::295`).

## Containers y Docker

- Ninguno de los dos flujos arranca containers por sí mismos. Dejan el
  proyecto `migrationPending`. Los containers se crean en
  `migrate_site` (ruta común).
- `docker::ensure_db` se invoca **solo** cuando se ejecuta la migración.
  Hasta entonces la carpeta del proyecto está en disco pero `wp-{id}` no
  existe.
- `migrate::import_dump` usa `docker exec -i panel-mysql-{ver} mysql -uroot
  -ppanel {db}` con stdin adjunto desde la app Rust
  (`migrate.rs::261`); el `exec_stdin` de bollard se cuelga con dumps
  grandes, por eso se usa el CLI directo. El import mide la DB con
  `information_schema` para decidir avance; si 180 s sin progreso,
  `child.start_kill()` + `wordpress::reset_database` (DB vacía) + error con
  instrucción de reintentar.

## Fallos y compensaciones

- **`cp -a` falla copiando `app/public`**: error crudo, no se crea el
  `config.json` ni el sidecar; el usuario ve el mensaje literal.
- **Dump aplica parcialmente** (mysql muere durante el import):
  `migrate.rs::run_migration` mata el exec y llama
  `wordpress::reset_database` (`DROP DATABASE` + recreación vacía). El
  usuario puede reintentar «Migrar y encender»: la DB queda limpia y los
  pasos previos son idempotentes.
- **`fix_site_url` falla** (p. ej. plugin colgado): se registra `⚠` en el
  op-log y se sigue; el proyecto queda encendido con el dominio antiguo y
  el usuario puede ajustarlo manualmente desde el admin.
- **`wp option update --skip-plugins --skip-themes`**: está aplicado en
  `migrate::fix_site_url` por la misma razón: un plugin migrado puede
  engancharse al init y bloquear el comando.
- **`wp-config.php` ilegible** en `reconstruct_config`: fallback
  `{slug}_db`; no aborta.
- **Carpetas con `migrationPending` pero sin contenedor**: al pulsar
  «Cancelar» desde el detalle, `delete_site(id, true)` borra la carpeta
  entera. La rama del backend emite cada paso por `op-log`.

## Superficies

### UI (SvelteKit, SPA)

- **`/import-localwp`** (`src/routes/import-localwp/+page.svelte`):
  lista los sitios candidatos, botón «Importar» por fila, badge
  «Ya importado», `OpConsole` para progreso. Errores en banda roja.
- **`/` → botón «Importar proyecto»** del dashboard
  (`src/routes/+page.svelte:296`): abre `ImportProjectModal.svelte` que
  consume `api.listDisconnectedSites` / `api.importDisconnectedSite` y
  pinta cada `DisconnectedSite` con su `kind` («config conservada» verde /
  «reconstruido» ámbar), versiones detectadas, flag `hasDump`.
- **`ProjectDetail.svelte`** (`src/lib/components/`): para proyectos
  `migrationPending` muestra acciones «Cancelar» (rojo) y
  «Migrar y encender» (ámbar) en vez del botón encender/detener.
  `migrate()` invoca `api.migrateSite(id)` y abre `OpConsole`.
- **`DeleteProjectModal.svelte`**: checkbox «Borrar también la carpeta…»
  conmutando entre `deleteSite(id, true)` y `deleteSite(id, false)`.
  Cuando es `false`, el modal avisa que la carpeta queda desconectada y
  re-importable.

### IPC (Tauri commands en `src-tauri/src/lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `list_localwp_sites` | — | `localwp::list_sites` |
| `import_localwp_site` | `id` | `localwp::import_site`; emite `op-log` |
| `list_disconnected_sites` | — | `config::list_disconnected_sites` |
| `import_disconnected_site` | `folderName` | `import_disconnected`; emite `op-log` |
| `migrate_site` | `id` | `migrate::migrate_site`; emite `op-log` |
| `delete_site` | `id`, `deleteFolder` | `false` → sidecar; `true` → `remove_dir_all` |
| `repair_autologin` | `id` | Reinyecta mu-plugins en proyectos importados de LocalWP |

`api.ts` (`src/lib/api.ts`) expone los espejos: `listLocalwpSites`,
`importLocalwpSite`, `listDisconnectedSites`, `importDisconnectedSite`,
`migrateSite`, `deleteSite`, `repairAutologin`.

### CLI (`scripts/wordpress-panel-cli.sh`)

No expone subcomandos directos para LocalWP ni disconnected; estos flujos
son solo de UI/panel.

### MCP (`mcp/server.mjs`)

Tampoco expone herramientas para LocalWP o disconnected. Las herramientas
presentes (`list_projects`, `start_project`, `stop_project`, etc.) operan
sobre proyectos ya registrados.

### D-Bus (`src-tauri/src/dbus.rs`)

No expone métodos específicos para estos flujos.

## Tests

- `localwp::tests::major_minor_recorta_patch`: `8.4.10 → 8.4`, `7.4 →
  7.4`, vacío → vacío.
- `localwp::tests::pick_supported_soportada_sin_ajuste` /
  `pick_supported_no_soportada_usa_mas_reciente`: PHP 5.6 → 8.4 ajustado,
  MySQL 8.2 → 8.4 ajustado.
- `config::tests::siteconfig_roundtrip_camelcase`: valida que
  `migrationPending` se serializa camelCase.
- `integration_tests.rs` (tests marcados `#[ignore]`) cubre un import
  LocalWP hermético y la migración end-to-end (Docker real). Ver
  `docs/TESTING.md`.

## Límites conocidos

- **LocalWP dump en disco**: si el usuario no tiene `app/sql/local.sql`
  preexportado, el panel no lo genera. Hay que exportarlo desde LocalWP
  manualmente (doc en `docs/KNOWN_ISSUES.md`).
- **No search-replace**: el panel NO reescribe el contenido del dump
  (`wp search-replace`) porque asume que la URL origen no es válida para el
  panel; en su lugar ajusta `home`/`siteurl` por `wp option update`
  después de importar. Si el dump contiene serializados PHP con la URL
  vieja, esos strings siguen apuntando al dominio origen hasta que un
  plugin tipo duplicador los corrija (responsabilidad del usuario).
- **`fix_site_url` solo toca dos opciones**: si el sitio tenía URLs
  hardcoded en opciones adicionales (`siteurl` en sub-sites multisite,
  etc.), se ajusta el sitio principal pero las entradas secundarias pueden
  quedar inconsistentes. Multisite se avisa en `note` para revisión
  manual.
- **Id colisionado en `reconstructed`**: si se duplica una carpeta con el
  mismo nombre y sin sidecar, el `reconstruct_config` genera un `id`
  nuevo cada vez (correcto). El `wp-config.php` puede seguir teniendo el
  `DB_NAME` viejo, lo que provocará `migrate::fix_site_url` exitoso pero
  `create_database` lo creará nuevo — no es problema porque el schema se
  crea del nombre del `SiteConfig.db_name`, no del `wp-config.php`.
- **dump-old ≠ dump-new**: `imported.sql` no se borra después de migrar.
  Permanece en `app/sql/` junto al `db-{ts}.sql` que pueda generar
  después el auto-dump; se puede podar manualmente.
- **`disconnected` y grupos**: la re-importación preserva `group` del
  sidecar pero `reconstruct_config` lo pone a `None` (no hay
  información previa).

## Invariantes y recomendación rebuild

- **Sidecar `config.disconnected.json` solo coexiste con `config.json`
  ausente**: `load_all_sites` (`config.rs::383`) ignora carpetas sin
  `config.json`; el sidecar no la hace visible. La lista de
  desconectados requiere `load_all_sites` + lectura propia del sidecar.
- **Un proyecto `migrationPending` nunca tiene contenedor `wp-{id}`**:
  `start_site` rechaza porque la fuente de verdad es `config.json`
  pendiente; el camino correcto es `migrate_site`.
- **`migration_pending = true` no se quita nunca manualmente**: solo
  `migrate_site::run_migration` lo baja a `false` tras importar el dump
  o detectar que no hay.
- **Rebuild desde cero**: borrar `config.disconnected.json` borra la
  posibilidad de re-importar como `preserved`; si queda el sidecar pero
  no `app/public`, `list_disconnected_sites` lo mostrará como
  `preserved` pero `import_disconnected` fallará en
  `app/public` faltante. El sidecar es útil mientras `app/public` siga
  en disco.

## Fuentes

- `src-tauri/src/localwp.rs`
- `src-tauri/src/migrate.rs`
- `src-tauri/src/config.rs` (modelos + `DISCONNECTED_CONFIG`,
  `list_disconnected_sites`, `parse_db_name`)
- `src-tauri/src/lib.rs` (comandos `import_localwp_site`,
  `import_disconnected_site`, `migrate_site`, `delete_site`,
  `repair_autologin`)
- `src-tauri/src/wordpress.rs` (`create_dirs`, `sync_mu_plugins`,
  `wp_config_create`, `create_database`, `reset_database`)
- `src-tauri/src/backup.rs` (`export_db_to`, usado por snapshot)
- `src/routes/import-localwp/+page.svelte`
- `src/lib/components/ImportProjectModal.svelte`
- `src/lib/components/DeleteProjectModal.svelte`
- `src/lib/components/ProjectDetail.svelte` (migrar / cancelar / reparar
  auto-login)
- `src/lib/api.ts`
- `docs/ARCHITECTURE.md` (sección «Catálogo de comandos IPC»)
- `docs/KNOWN_ISSUES.md` (limitaciones de import LocalWP)
