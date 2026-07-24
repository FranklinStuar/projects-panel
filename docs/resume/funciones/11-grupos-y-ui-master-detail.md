# 11 · Grupos y UI master-detail

Cubre la lista durable de grupos (`groups.rs`), la asignación por drag&drop,
y el rediseño UI estilo LocalWP: riel de íconos + lista maestra a la
izquierda + detalle embebido (`ProjectDetail.svelte`) sin navegar.

## Resultado para el usuario

- **Crear, renombrar, borrar y reordenar** grupos persistidos. La lista
  vive en `~/.config/wordpress-panel/groups.json` (`{ order: [...] }`).
- **Asignar un proyecto a un grupo** arrastrando su fila sobre la cabecera
  del grupo (drag&drop nativo HTML5). Soltar sobre «Sin grupo» quita la
  asignación.
- **Ver proyectos agrupados** en el dashboard, con:
  - Una sección fija «En ejecución» al inicio (los proyectos `running`
    suben sin duplicarse).
  - Cabeceras de grupo plegables (estado en `localStorage`).
  - Clones anidados bajo su padre (cuando ambos están parados).
- **Navegar al detalle** sin cambiar de ruta: se monta
  `ProjectDetail.svelte` por `selectedId` dentro de la misma página `/`.

## Precondiciones

- **Master-detail embebido**: la ruta `/` (`src/routes/+page.svelte`)
  siempre es la lista; el detalle se monta en el panel derecho. La ruta
  `/site/[id]` (`src/routes/site/[id]/+page.svelte`) existe solo como
  wrapper de deep-link (URLs externas o abrir un proyecto concreto).
- **`groups.json`** debe ser escribible (es un archivo del usuario bajo
  `config_dir`). Si el usuario no tiene permisos, los grupos no
  persisten.
- **Drag&drop**: requiere un navegador con soporte para HTML5 drag&drop
  (todos los modernos). Funciona con mouse y con touchpad.
- **Plugin `core:event`**: los eventos `sites-changed` requieren la
  capability `core:event:default` en
  `src-tauri/capabilities/default.json` (ver
  `docs/ARCHITECTURE.md §"Capability obligatoria para eventos"`).

## Flujo feliz (numerado)

### Crear un grupo

1. UI: botón «+» pequeño en la cabecera de la lista de proyectos
   (`+page.svelte:196`): abre un input inline «Nombre del grupo…».
   Enter confirma; blur también; Escape cancela.
2. `addGroup()` (`+page.svelte:103`) →
   `api.createGroup(name)` → `create_group(name)` (`lib.rs::548`) →
   `groups::create(name)` (`groups.rs::52`):
   - `trim` del nombre.
   - Si vacío: no-op.
   - Si ya existe en `groups.json`: no-op (idempotente).
   - Si no: lo añade al final de `order` y persiste.
3. `load()` recarga `api.listGroups()` y redibuja la lista con la nueva
   cabecera.

### Asignar un proyecto a un grupo (drag&drop)

1. UI: el usuario inicia drag sobre una fila (no en clones:
   `draggable={!s.config.cloneOf}`, `+page.svelte:330`). `dragstart`
   guarda `dragId = id`.
2. Al pasar sobre una cabecera de grupo: `dragover` con `preventDefault`
   marca `dragOverGroup = name`. La cabecera recibe fondo azul.
3. `drop` → `dropOnGroup(group)` (`+page.svelte:120`):
   - Si `group === UNGROUPED`, target = `''` (cadena vacía).
   - Si el grupo actual coincide, no-op (sin cambio).
   - `api.setSiteGroup(id, target || null)` → `set_site_group(id, group?)`
     (`lib.rs::527`):
     - `site.group = group.filter(|g| !g.trim().is_empty())` (None si vacío).
     - `groups::create(group)` para registrar el grupo destino en
       `groups.json` si era nuevo.
     - `write_site_config(&site)`.
4. `load()` recarga; la fila aparece bajo la cabecera destino.

### Renombrar/borrar/reordenar grupos

- **Renombrar**: `api.renameGroup(old, new)` → `rename_group`
  (`lib.rs::553`) → `groups::rename(old, new)` (`groups.rs::67`):
  - `trim` del nuevo; si vacío o igual al viejo, no-op.
  - Cambia el nombre en `order` y deduplica.
  - Recorre `load_all_sites()` y reasigna `config.group` para todos los
    proyectos que lo tenían.
- **Borrar**: `api.deleteGroup(name)` → `delete_group` (`lib.rs::557`) →
  `groups::delete(name)` (`groups.rs::97`):
  - Quita el nombre de `order` y reescribe `groups.json`.
  - Recorre `load_all_sites()` y pone `config.group = None` en los que lo
    tenían.
- **Reordenar**: drag de cabeceras de grupo (drop reordena).
  `api.reorderGroups(order)` → `reorder_groups` (`lib.rs::564`) →
  `groups::reorder(order)` (`groups.rs::114`):
  - Deduplica, trimea, descarta vacíos.
  - Sobrescribe `order` en `groups.json`.

### Listado master-detail

1. `+page.svelte::onMount` carga `sites`, `endpoint`, `persistedGroups`
   con `Promise.all([api.getSites(), api.panelEndpoint(),
   api.listGroups()])`.
2. Se suscribe a `listen('sites-changed', load)` para reaccionar a
   mutaciones que vengan del CLI/MCP (D-Bus, ver `dbus.rs::notify_sites_changed`).
3. **Agrupación** (`+page.svelte:145`):
   - Filtra `sites` por `status !== 'running'` (los `running` van arriba).
   - Construye `clonesByParent` (mapa `parent_id → [clones]`).
   - Para cada proyecto parado, decide si va suelto en su grupo (con sus
     clones anidados) o si es un clon de un padre también parado (en ese
     caso NO se renderiza suelto, ya está anidado bajo el padre).
   - Orden: grupos persistidos primero (incluyendo vacíos como drop
     target), luego grupos sueltos detectados (los que solo aparecen en
     `config.group` de algún proyecto), al final «Sin grupo».
4. **Render**: cada grupo como `<section>` con cabecera (botón plegable +
   nombre + contador) y debajo las filas (`siteRow` con badge `C` ámbar
   para clones, badge `W` violeta para worktrees).
5. **Detalle embebido**: `{#if selectedId}<ProjectDetail ... />{/if}`
   en el panel derecho. `onChanged` refresca la lista izquierda; `onSelect`
   cambia el `selectedId` (clones/worktrees recién creados navegan al
   detalle del nuevo); `onDeleted` deselecciona y refresca.

### Plegado de grupos

- Estado en `localStorage` bajo `wp-panel:collapsed-groups`
  (`+page.svelte:29`).
- `toggleCollapse(name)` conmuta y persiste el JSON.
- El estado es por nombre de grupo: renombrar el grupo «limpia» su estado
  de plegado (la key cambia).

### Detalle embebido (`ProjectDetail.svelte`)

- Recibe `id` por prop, no navega. `selectedId` cambia el detalle sin
  cambiar la ruta.
- Tabs: `info`, `logs`, `ext`, `github`, `svc`, `snapshots` (definidos en
  `ProjectDetail.svelte:591`).
- Cabecera: acción primaria (Encender/Detener o Migrar y encender si
  `migrationPending`), accesos rápidos (admin/site/folder) cuando está
  activo, menú «···» con «Abrir carpeta», «Punto de guardado», «Regenerar
  SSL» (si SSL activo), «Eliminar».
- Mount por `{#key selectedId}` para forzar remontaje al cambiar
  proyecto (state limpio).
- El wrapper `/site/[id]` monta el mismo componente pero con
  `onSelect = (next) => goto('/site/' + next)` y `onDeleted = () => goto('/')`,
  para deep-links.

## Variantes y casos borde

- **Clon anidado bajo padre activo**: si el padre está `running`, el
  clon no se anida (se muestra suelto en su grupo) — porque el padre
  está en «En ejecución» y la jerarquía visual se rompería
  (`+page.svelte:160-162`).
- **Proyecto con `cloneOf`**: no es draggable
  (`draggable={!s.config.cloneOf}`), porque pertenece al padre; cambiar
  su grupo arrastraría el clon, lo cual no tiene sentido.
- **Grupo vacío persistente**: existe en `groups.json` aunque no tenga
  proyectos. Sirve como «drop target» para arrastrar proyectos a un
  grupo recién creado. `+page.svelte:168-174` los pinta igual.
- **Grupo detectado pero no persistido** (un proyecto tiene `config.group`
  con un nombre que no está en `groups.json`): aparece tras los grupos
  persistidos y antes de «Sin grupo». Útil mientras el usuario decide
  crearlo.
- **Drag sobre «Sin grupo»**: la cabecera se renderiza como una sección
  con `role="group"` y `dropOnGroup(UNGROUPED)` mapea a `target = ''`
  → `setSiteGroup(id, null)`.
- **`localStorage` corrupto** (no es JSON válido): el bloque try/catch
  en `+page.svelte:34` ignora el error y arranca con `collapsed = {}`.
- **Eventos `sites-changed` no llegan**: la UI no recarga cambios
  externos. Causa típica: capability `core:event` no concedida. Verificar
  `src-tauri/capabilities/default.json`.
- **Carga inicial vacía**: «No hay proyectos. Crea uno con el botón `+`
  del riel.»
- **Selección perdida**: si el `selectedId` no está en la nueva lista
  (proyecto borrado desde fuera), `+page.svelte:55` lo limpia.
- **Stream de logs en segundo plano**: `stream_logs` es idempotente
  (state `LogStreams` deduplica por id; `lib.rs::468`).

## Datos persistidos

- **`~/.config/wordpress-panel/groups.json`** (`GroupsFile`):
  `{ "order": ["LocalWP", "Clientes", "Sandbox"] }`. Solo guarda el orden
  y la lista; la pertenencia vive en cada `config.group`.
- **`SiteConfig::group`**: `Option<String>` (`config.rs:171`). El valor es
  el nombre exacto del grupo (tras `trim`).
- **`localStorage`**: `wp-panel:collapsed-groups` →
  `Record<string, boolean>`. Por usuario, por navegador (WebKitGTK en el
  panel).
- **`SiteConfig::cloneOf`**: anidación visual cuando el clon y su padre
  están parados.
- **`SiteConfig::worktreeOf`**: badge `W` violeta en la fila.

## Containers y Docker

- **Grupos**: no afectan al ciclo de vida de containers. Un proyecto en
  cualquier grupo se enciende y apaga igual.
- **Master-detail embebido**: el detalle es un componente Svelte puro;
  no inicia containers por sí mismo. Las acciones de los tabs (encender,
  apagar, migrar, etc.) sí invocan los comandos Tauri ya existentes.

## Fallos y compensaciones

- **`groups.json` corrupto** (no parsea): `groups::read_file` ignora
  errores con `unwrap_or_default()` (`groups.rs::34`); la lista queda
  vacía. El siguiente `create` o `reorder` reescribe el archivo.
- **`create_group` con `~/.local/share` no escribible**: error
  propagado a la UI; los grupos creados en memoria se pierden al
  recargar.
- **`set_site_group` con grupo que no existe aún**: `groups::create` lo
  registra automáticamente (`lib.rs::531`); no hay error.
- **`rename_group` con `new` ya existente en `order`**: el rename
  deduplica (`groups.rs:81-84`), evitando duplicados en `order`. La
  reasignación de proyectos ocurre igual.
- **`delete_group` con proyectos que lo tenían**: los proyectos quedan
  con `group = None` (sin grupo). Persisten en disco aunque no aparezcan
  en ninguna cabecera visible.
- **`reorder_groups` con `order` vacío**: persiste un archivo con `order:
  []`. La lista queda sin grupos persistidos; los grupos derivados de
  `config.group` siguen visibles hasta que se les cambie el grupo.
- **Drag&drop sin soporte HTML5**: el botón `draggable` no hace nada; la
  fila es solo clicable (selecciona). El usuario puede cambiar el grupo
  vía API directa (`set_site_group`) si la implementa alguna integración
  futura (hoy no hay input de grupo en el detalle del proyecto; el drag&
  drop es la única vía en la UI actual).
- **CSS drag indicators**: la clase `dragOverGroup === name` aplica
  `bg-blue-500/10 ring-1 ring-blue-500/40`. Si Tailwind no incluye esos
  colores arbitrarios, no se ve el feedback. Verificar
  `tailwind.config.js` (`darkMode: 'class'` + `safelist` si aplica).
- **Carga con `endpoint` null**: `hostLabel` cae al dominio sin puerto
  (`+page.svelte:66`). Sin endpoint no se puede construir el link
  `dominio:puerto` si el panel publica en puerto alterno.

## Superficies

### UI (SvelteKit, SPA)

- **`/`** (`src/routes/+page.svelte`): master-detail embebido completo.
  - Columna izquierda (w-64): cabecera con botones «+ grupo» y «Apagar
    todo» (con badge de count), input inline para nombre de grupo,
    secciones: «En ejecución» (verde) + grupos persistidos (en orden) +
    «Sin grupo» (al final). Cada proyecto como `siteRow` con dot de
    estado, nombre, badge `C`/`W` si aplica, botón play/stop.
  - Panel derecho (flex-1): `ProjectDetail.svelte` o mensaje «Selecciona
    un proyecto de la lista».
  - Botón «Importar proyecto» abajo de la columna izquierda.
- **`/site/[id]`** (`src/routes/site/[id]/+page.svelte`): wrapper de
  deep-link que monta el mismo `ProjectDetail`. `onDeleted` navega a
  `/`, `onSelect` navega a `/site/{next}`.
- **`+layout.svelte`** (`src/routes/+layout.svelte`): riel de íconos a
  la izquierda (w-14) con `nav` = Proyectos (`/`), Dominios, Servicios,
  Importar desde LocalWP, Log de volcados de DB, CLI (terminal),
  Configuración, y el botón flotante `+` abajo para `/site/new`. Detecta
  ruta activa con `isActive(href)` y aplica `aria-current="page"` con
  `aria-[current=page]:bg-blue-600`.

### IPC (Tauri commands en `lib.rs`)

| Comando | Args | Notas |
|---|---|---|
| `set_site_group` | `id, group?` | `None`/vacío = sin grupo; crea el grupo si no existía |
| `list_groups` | — | `groups::list` |
| `create_group` | `name` | Idempotente |
| `rename_group` | `old, new` | Reasigna proyectos |
| `delete_group` | `name` | Proyectos quedan sin grupo |
| `reorder_groups` | `order[]` | Sobrescribe el orden |

`api.ts` (`src/lib/api.ts`) expone los espejos.

### CLI / MCP / D-Bus

**No** exponen gestión de grupos. El drag&drop es solo de UI (los
comandos Tauri correspondientes viven solo en el backend).

## Tests

- `groups::tests::create_is_idempotent`, `delete_removes_from_list`,
  `reorder_dedups_and_trims`: marcados `#[ignore]` (mutan
  `config_dir()` real, ejecutables con
  `cargo test -- --ignored --test-threads=1`).
- `config::tests::siteconfig_roundtrip_camelcase` valida que `group` se
  serializa como `group` (no renombrado).
- `ProjectDetail.svelte` y `+page.svelte` se prueban vía Playwright
  (`e2e/`) arrancando `pnpm dev:mock`.

## Límites conocidos

- **Solo drag&drop para asignar grupo**: no hay input de texto en el
  detalle del proyecto. Si el navegador no soporta drag&drop (raro), no
  se puede reasignar grupo.
- **No hay grupos anidados**: la lista es plana (orden único).
- **No hay colores/iconos por grupo**: solo el nombre. Estilo LocalWP.
- **No hay búsqueda/filtro**: el `localStorage` solo guarda el estado de
  plegado. Para encontrar un proyecto entre muchos, hay que desplegar
  el grupo correspondiente.
- **El riel de íconos es fijo**: no se puede personalizar el orden ni
  ocultar entradas. La lista vive en `+layout.svelte` y requiere rebuild.
- **Estado de plegado por nombre, no por id**: renombrar un grupo resetea
  su estado de plegado.
- **Drag&drop no es accesible por teclado**: solo mouse. La
  accesibilidad del master-detail es parcial (`role="button"` +
  `tabindex="0"` + Enter/Espacio para seleccionar).
- **Sin URL directa a un grupo**: `/?group=LocalWP` no existe. Solo se
  puede compartir la URL `/site/[id]` de un proyecto concreto.
- **Sin selección múltiple**: solo se selecciona un proyecto a la vez.
- **Badge `C`/`W` no se filtra**: si solo quieres ver worktrees, hay
  que desplegar todos los grupos.

## Invariantes y recomendación rebuild

- **`groups.json` solo persiste el orden y la lista**: la pertenencia
  está duplicada en cada `SiteConfig.group` (espejo). Borrar
  `groups.json` no borra la pertenencia (los grupos derivados siguen
  visibles). Borrar `SiteConfig.group` no borra la entrada en
  `groups.json` (queda como grupo vacío).
- **Un proyecto siempre tiene grupo o `None`**: `set_site_group` con
  cadena vacía o `null` siempre normaliza a `None`. Nunca se queda con
  una cadena vacía.
- **El riel de íconos solo cubre rutas reales**: si añades una ruta, hay
  que meterla en el array `nav` de `+layout.svelte`.
- **`selectedId` no se persiste**: al recargar el panel, la lista
  izquierda se llena y `selectedId` queda `null` (no hay proyecto
  seleccionado). El usuario debe hacer clic.
- **Estado de plegado en `localStorage`**: vive en el perfil del
  navegador (WebKitGTK del panel). No se exporta ni sincroniza.
- **Rebuild desde cero**: borrar `groups.json` borra el orden
  persistido pero los proyectos mantienen su `group`. La siguiente
  carga los mostrará como «grupos sueltos detectados» (sin un orden
  estable). Renombrarlos o reordenarlos desde la UI los persiste de
  nuevo.
- **Drag&drop con shift/teclado**: si en el futuro se quiere, hay que
  añadir listeners de teclado sobre las filas; hoy la UI solo soporta
  mouse.

## Fuentes

- `src-tauri/src/groups.rs`
- `src-tauri/src/config.rs` (`SiteConfig::group`, `SiteConfig::cloneOf`,
  `SiteConfig::worktreeOf`)
- `src-tauri/src/lib.rs` (comandos `set_site_group`, `list_groups`,
  `create_group`, `rename_group`, `delete_group`, `reorder_groups`)
- `src/routes/+page.svelte` (master-detail embebido, drag&drop,
  plegado, listado)
- `src/routes/+layout.svelte` (riel de íconos)
- `src/routes/site/[id]/+page.svelte` (wrapper de deep-link)
- `src/lib/components/ProjectDetail.svelte` (detalle embebido)
- `src/lib/api.ts`
- `docs/ARCHITECTURE.md` (sección «UI master-detail»)
- `docs/KNOWN_ISSUES.md` (botones de la barra de título — no afecta
  grupos)
