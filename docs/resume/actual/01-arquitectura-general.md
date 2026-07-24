# Arquitectura general

> Estado verificado sobre `main` en `373841c`, 2026-07-23. Describe la implementación existente, no el plan futuro.

## Forma del sistema

Panel WP es una aplicación de escritorio Linux: una SPA SvelteKit 2/Svelte 5 se ejecuta dentro de una ventana Tauri 2 y delega la autoridad operativa en un backend Rust. Este backend persiste configuración en el filesystem del usuario, orquesta Docker y llama herramientas del host cuando no existe una vía adecuada o deliberadamente se reutiliza la sesión del usuario.

```text
┌──────────────── ventana Tauri ────────────────┐
│ SvelteKit SPA                                 │
│ rutas/componentes → api.ts → invoke()         │
│             ↑ eventos Tauri                   │
│             │                                 │
│ Rust: comandos + módulos de dominio           │
└─────────────┬──────────────┬───────────────────┘
              │              │
         bollard/API     procesos host
              │          git, gh, mkcert,
              │          terminal, docker¹
        Docker daemon
              │
  panel-net: nginx, DB, PHP, Mailpit, MinIO,
             Adminer

¹ Excepciones concretas: build PHP, import SQL con stdin y migración de datadir.
```

La aplicación no incorpora una base central de proyectos. `config::load_all_sites` reconstruye el registro escaneando `~/panel-wp/*/config.json`; Docker aporta el estado de ejecución real. El estado global pequeño reside en `~/.config/wordpress-panel/`.

## Principios materializados

- **Nada corre sin demanda.** `docker::DockerManager::start_site` crea/arranca recursos; `DockerManager::stop_site` y `DockerManager::teardown_unused_shared` apagan servicios compartidos que ya no necesita ningún PHP activo.
- **Compartir antes que duplicar.** Hay un nginx y Mailpit globales, una DB por motor+versión y MinIO/Adminer bajo demanda; solo PHP-FPM es por proyecto.
- **Código y datos visibles en disco.** WordPress, configuración PHP, dumps, certificados y metadatos viven bajo la carpeta del proyecto; los datadir DB compartidos se bindean a la configuración del usuario.
- **Una sola lógica operativa.** GUI llama IPC Tauri; CLI y MCP llegan al mismo backend por D-Bus. El plasmoid también usa esa interfaz.

## Capas y ownership

```text
Presentación       src/routes, src/lib/components
Contrato GUI       src/lib/api.ts + src/lib/types.ts
Aplicación         src-tauri/src/lib.rs (comandos y composición)
Dominio/servicios  wordpress, migrate, snapshot, clone, worktree,
                   github, autologin, backup, groups
Infraestructura    docker, nginx, php, ssl, domain, logs, dbus, cli
Persistencia       config.json por sitio + archivos globales de config_dir
```

Rust es propietario de las mutaciones durables y del ciclo de vida. El frontend posee selección, pestañas, estados de carga, buffers visuales y preferencias locales como grupos contraídos o usuario de auto-login. Docker conserva el estado runtime; nunca se infiere `running` solo desde la UI.

## Flujos principales

**Encendido:** `lib::start_site` carga configuración → `DockerManager::start_site` asegura red, DB, Mailpit, MinIO opcional, PHP, vhost y nginx → registra `AutoDump`.

**Parada:** aborta watcher → exporta DB best-effort y rota dumps → para PHP → retira vhost → apaga DB/MinIO/nginx/Mailpit/Adminer si dejan de ser necesarios.

**Creación/migración:** operaciones largas emiten `op-log`; WordPress y servicios se preparan desde módulos Rust, sin que Svelte replique lógica.

**Acceso externo:** los sitios se publican siempre en loopback y puertos altos elegidos desde 8080/8443. Los PHP de proyecto no publican puertos.

## Funciones presentes y ausentes

Existen importación LocalWP/desconectados, snapshots, clones, worktree-projects, Git/GitHub, dumps, terminal, VSCode, WP-CLI, Adminer, Mailpit y MinIO. Los botones Cloudflare, deploy genérico y empaquetado de sitio son stubs; el deploy Git directo sí existe. **No existe un módulo ni proveedor de IA.** Para el detalle de comandos o modelos, usar conceptualmente `../referencia/*` en vez de convertir este resumen en catálogo.

## Deuda observable

La configuración Tauri mantiene `csp: null`; varias integraciones dependen de binarios Linux del host; el plasmoid ejecuta `qdbus6` mediante el motor `executable`; y hay duplicación parcial de adaptación entre IPC y D-Bus. La posición de botones nativos sigue pendiente; no se fuerza `GTK_CSD=0` (ese cambio fue revertido).

## Fuentes primarias

- `src-tauri/src/lib.rs::run`, `start_site`, `stop_site`
- `src-tauri/src/config.rs::load_all_sites`, `SiteConfig`
- `src-tauri/src/docker.rs::DockerManager::start_site`, `teardown_unused_shared`
- `src/lib/api.ts::api`
- `src/routes/+layout.ts::ssr`
- `src-tauri/tauri.conf.json::app`
