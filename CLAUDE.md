# CLAUDE.md — Guía para agentes IA

Este archivo lo carga Claude Code automáticamente. Es el punto de entrada para
entender el proyecto y hacer cambios correctos. **Manténlo actualizado**: cuando
modifiques arquitectura, módulos o convenciones, actualiza también
`docs/ARCHITECTURE.md` y `docs/EXTENDING.md`.

## Qué es

Panel de escritorio (Rust/Tauri 2 + SvelteKit) para gestionar proyectos WordPress
locales con Docker. Reemplaza a LocalWP. **Objetivo nº1: optimizar consumo de
recursos.** Todas las decisiones se subordinan a tres reglas:

1. **Nada corre si no hace falta** — containers solo para proyectos *activos*; parado = 0 recursos.
2. **Compartir antes que duplicar** — 1 nginx, DB por versión, 1 mailpit/minio compartidos; no por proyecto.
3. **Imágenes mínimas** — alpine donde exista.

Si una función rompe estas reglas, se rediseña o se pospone. Detalle completo del
producto y fases en `PLAN.md`.

## Documentación (leer antes de tocar)

- **`PLAN.md`** — plan de producto por fases, UI, flujos, decisiones.
- **`docs/ARCHITECTURE.md`** — modelo de containers/recursos, mapa de módulos, flujo de datos, catálogo de comandos IPC, rutas en disco.
- **`docs/EXTENDING.md`** — recetas paso a paso para agregar funciones (comando IPC, ruta frontend, servicio compartido, motor DB, módulo Rust, proveedor IA…).
- **`docs/CHANGELOG.md`** — qué se construyó y cuándo, por fase.

## Comandos

```bash
pnpm install                 # deps frontend (incluye @tauri-apps/cli)
pnpm tauri dev               # GUI en desarrollo (vite :1420 + ventana Tauri)
pnpm build                   # build frontend estático → build/
cd src-tauri && cargo build  # solo backend
bash scripts/first-run.sh    # primera config del sistema (panel-net, dnsmasq, mkcert)

# Testing (ver docs/TESTING.md)
cd src-tauri && cargo test               # lógica pura, rápido, sin Docker
cargo test -- --ignored --test-threads=1 # integración (Docker / muta entorno)
pnpm dev:mock                            # SPA con IPC mockeado (GUI sin backend)
pnpm test:e2e                            # e2e GUI con Playwright (arranca dev:mock)
```

En Wayland, si la ventana sale en blanco: `WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev`.

## Estructura (resumen)

```
PLAN.md                  plan de producto
CLAUDE.md                este archivo
docs/                    architecture, extending, changelog
src/                     frontend SvelteKit (SPA, ssr=false)
  routes/                páginas (dashboard, site/new, site/[id], domains, services, settings)
  lib/api.ts             capa IPC (espejo de comandos Rust)
  lib/types.ts           tipos TS (espejo de modelos serde)
src-tauri/src/           backend Rust
  lib.rs                 comandos #[tauri::command] + run()
  config.rs              modelos + persistencia (~/panel-wp/*/config.json)
  docker.rs              orquestación bollard (red, compartidos, ciclo de vida)
  nginx.rs / php.rs / domain.rs / wpcli.rs / wordpress.rs
  logs.rs / autologin.rs / github.rs / ssl.rs / dbus.rs
  backup.rs (export DB) / cli.rs (instala wrapper wp en ~/.local/bin)
docker/                  Dockerfile php-fpm + entrypoint, plantillas, mu-plugins
scripts/                 first-run.sh, wp-wrapper.sh, wordpress-panel-cli.sh
```

## Convenciones (no romper)

- **Modelos serde en `camelCase`** (`#[serde(rename_all="camelCase")]`); los tipos
  de `src/lib/types.ts` deben ser su espejo exacto.
- **Comandos Tauri** devuelven `Result<T, String>`; usar el helper `e()` para
  mapear errores. Registrar el comando en `invoke_handler!` y exponerlo en `api.ts`.
- **Fuente de verdad de proyectos** = el `config.json` en cada carpeta de
  `~/panel-wp/`. No hay base de datos central; `load_all_sites()` escanea.
- **Docker solo vía bollard** en runtime (`DockerManager`). Excepción: build de la
  imagen php usa el CLI `docker build` (`php.rs`).
- **Container por proyecto NO publica puertos al host** — solo `panel-nginx` lo hace.
- **Naming de containers**: proyecto = `wp-{site-id}`; compartidos = `panel-*`.
- **Estado de recursos**: al detener un proyecto, apagar también los compartidos
  que ya no use ningún activo (`teardown_unused_shared`). Nunca dejar algo
  corriendo "por si acaso".

## Estado actual

Fases 1–4 completas. Fase 4 incluyó: pantalla de configuración con estado del
sistema (`system.rs`), migración entre sistemas + export-al-detener
(`migrate.rs`, `backup::rotate_dumps`), import desde LocalWP (`localwp.rs`) y
empaquetado del plasmoid (`scripts/package-plasmoid.sh`). Pendiente: Fase 5 (IA,
`agent.rs`). Ver `docs/CHANGELOG.md` para el detalle. Diferido dentro de Fase 3:
container de frontend headless y plugin S3 (WP↔MinIO). Limitaciones/temas
diferidos en `docs/KNOWN_ISSUES.md`: botones de la barra de título, verificación
visual del plasmoid en Plasma, e import LocalWP (la DB requiere dump en disco).
