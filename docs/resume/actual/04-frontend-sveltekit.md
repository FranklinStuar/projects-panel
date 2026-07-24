# Frontend SvelteKit

## Runtime y estructura

Es una SPA SvelteKit con `ssr=false` y `prerender=false`. Tauri sirve el build estático y el router cliente resuelve rutas, incluida `/site/[id]`. Svelte 5 usa runes (`$state`, `$derived`, `$effect`) y Tailwind para estilos.

```text
+layout.svelte
├─ riel fijo: Proyectos, Dominios, Servicios, Importar,
│             Dumps, CLI, Configuración
├─ botón global “Nuevo proyecto”
└─ <main>
   ├─ /                 master-detail
   ├─ /site/new         creación
   ├─ /site/[id]        wrapper de deep-link
   └─ páginas de sección
```

## Master-detail actual

La ruta `/` no navega al seleccionar proyecto. `routes/+page.svelte::selectedId` controla dos paneles:

```text
┌──────── lista (w-64) ───────┬──────── detalle flexible ─────────┐
│ En ejecución                │ ProjectDetail(id=selectedId)       │
│ grupos persistidos          │ cabecera + acción primaria         │
│  ├ proyecto                 │ tabs: info, logs, extensiones,     │
│  └ clone anidado            │ GitHub, servicios, snapshots       │
│ Sin grupo                   │                                    │
└─────────────────────────────┴────────────────────────────────────┘
```

Los activos se fijan arriba y dejan temporalmente su grupo. Los grupos vacíos se muestran como drop targets; arrastrar una fila a una cabecera llama `api.setSiteGroup`. Los clones parados se anidan bajo su padre parado. `ProjectDetail.svelte` es la implementación canónica del detalle; `/site/[id]` solo permite deep-link.

## Estado y ownership

El frontend no mantiene store global de proyectos: cada vista carga por IPC. En `/`, `load` obtiene sitios, endpoint y grupos en paralelo. `ProjectDetail::load` recupera todos los sitios y selecciona el `id`. Tras mutaciones, callbacks refrescan lista y detalle.

Estado durable solo visual en `localStorage`:

- `wp-panel:collapsed-groups`: grupos contraídos.
- `wp-panel:autologin:{id}`: usuario elegido para abrir admin.

Estado autoritativo —configuración, running, snapshots, repos— siempre vuelve del backend.

## Operaciones y eventos

`src/lib/api.ts::api` es una capa fina de `invoke`. Los tipos de `src/lib/types.ts` reflejan serde. Operaciones largas abren `OpConsole`, que escucha `op-log` desde el montaje para no perder eventos iniciales; el prefijo SOH reemplaza una línea de progreso viva.

Logs de PHP usan un canal por sitio `log:{id}`. El componente escucha antes de invocar `stream_logs`, conserva hasta 500 entradas y detiene backend/listener al salir de la pestaña o destruirse.

La página raíz escucha `sites-changed`; mutaciones hechas por CLI/MCP vía D-Bus causan recarga automática. No hay sincronización push exhaustiva para cada cambio GUI: normalmente el propio flujo llama `load`.

## Modo de desarrollo

`+layout.ts::load` importa `$lib/dev/mock-ipc` cuando `VITE_MOCK_IPC=1`, antes de montar páginas. Esto permite Playwright sin backend Tauri ni Docker. Los tests e2e recorren flujos de dashboard, creación, borrado, importación, migración, configuración y accesibilidad.

## Navegación y herramientas

El detalle ofrece accesos a navegador, admin, carpeta, terminal y VSCode. La pestaña GitHub escanea repos del host, gestiona pull/deploy/worktrees y abre un workspace multi-root. Servicios expone Adminer, dump, Mailpit, MinIO y wrapper WP-CLI. Las URLs se forman con el endpoint recibido y actualmente muestran siempre puerto alto.

## Deuda observable

`ProjectDetail.svelte` concentra gran cantidad de estado y casos de uso visuales. Algunas estructuras se tipan como `any[]` para plugins/themes. La recarga completa de todos los sitios para obtener uno simplifica consistencia pero aumenta trabajo. No hay IA en UI ni backend; las menciones históricas de fase futura no equivalen a funcionalidad.

## Fuentes primarias

- `src/routes/+layout.ts::ssr`, `load`
- `src/routes/+layout.svelte::nav`, `isActive`
- `src/routes/+page.svelte::load`, `groups`, `siteRow`
- `src/lib/components/ProjectDetail.svelte::load`, `startLogs`, `tabs`
- `src/lib/components/OpConsole.svelte::onMount`
- `src/lib/api.ts::api`
- `src/lib/types.ts::SiteState`, `SiteConfig`
