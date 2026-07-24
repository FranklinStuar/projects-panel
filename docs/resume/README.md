# Panel WP: especificación para reconstrucción

Documentación técnica de comportamiento actual y plan para rehacer Panel WP desde cero con arquitectura mantenible.

> **Corte analizado:** 2026-07-23 · rama `main` · commit `373841c`  
> **Código actual:** fuente primaria. Documentación histórica puede describir diseños superados.

## Qué contiene

Este directorio responde cinco preguntas:

1. **¿Qué hace Panel WP?** — matriz funcional y fichas detalladas.
2. **¿Cómo funciona?** — arquitectura, persistencia, Docker, frontend e interfaces.
3. **¿Cómo se usa y recupera?** — runbooks operativos.
4. **¿Cómo llegó aquí y qué deuda tiene?** — changelog, decisiones y límites.
5. **¿Cómo rehacerlo mejor?** — arquitectura objetivo, contratos, testing y roadmap.

## Lectura rápida

### Quiero entender producto

1. [Resumen ejecutivo](01-resumen-ejecutivo.md)
2. [Estado y matriz funcional](02-estado-y-matriz-funcional.md)
3. [Arquitectura general](actual/01-arquitectura-general.md)
4. [Changelog consolidado](historia/01-changelog-consolidado.md)

### Quiero operar Panel WP

1. [Instalación y primera ejecución](operacion/01-instalacion-y-primera-ejecucion.md)
2. [Runbook de proyectos](operacion/03-runbook-proyectos.md)
3. [Importación, migración y recuperación](operacion/04-runbook-importacion-migracion-y-recuperacion.md)
4. [Snapshots, clones y worktrees](operacion/05-runbook-snapshots-clones-y-worktrees.md)
5. [Diagnóstico y mantenimiento](operacion/07-diagnostico-y-mantenimiento.md)

### Quiero mantener implementación actual

1. [Mapa del repositorio](03-mapa-del-repositorio.md)
2. [Backend Rust/Tauri](actual/03-backend-rust-tauri.md)
3. [Frontend SvelteKit](actual/04-frontend-sveltekit.md)
4. [Catálogo IPC y eventos](referencia/03-catalogo-ipc-y-eventos.md)
5. [D-Bus, CLI y MCP](referencia/04-catalogo-dbus-cli-y-mcp.md)
6. [Desarrollo, pruebas y build](operacion/02-desarrollo-pruebas-build-y-empaquetado.md)

### Quiero reconstruir desde cero

1. [Método y alcance](00-metodo-y-alcance.md)
2. [Objetivos, principios y no objetivos](reconstruccion/01-objetivos-principios-y-no-objetivos.md)
3. [Arquitectura objetivo](reconstruccion/02-arquitectura-objetivo.md)
4. [Contratos, estado y persistencia](reconstruccion/03-contratos-estado-y-persistencia.md)
5. [Orquestación Docker y operaciones](reconstruccion/04-orquestacion-docker-y-operaciones.md)
6. [Testing y calidad](reconstruccion/06-estrategia-de-pruebas-y-calidad.md)
7. [Roadmap, backlog y migración](reconstruccion/07-roadmap-backlog-y-migracion.md)
8. [Prueba de aceptación](verificacion/02-prueba-de-aceptacion-de-la-reconstruccion.md)

## Índice completo

### Base

- [00 — Método y alcance](00-metodo-y-alcance.md)
- [01 — Resumen ejecutivo](01-resumen-ejecutivo.md)
- [02 — Estado y matriz funcional](02-estado-y-matriz-funcional.md)
- [03 — Mapa del repositorio](03-mapa-del-repositorio.md)

### Sistema actual

- [Arquitectura general](actual/01-arquitectura-general.md)
- [Modelo de dominio y persistencia](actual/02-modelo-dominio-y-persistencia.md)
- [Backend Rust/Tauri](actual/03-backend-rust-tauri.md)
- [Frontend SvelteKit](actual/04-frontend-sveltekit.md)
- [Docker, red y servicios](actual/05-docker-red-y-servicios.md)
- [IPC, eventos y estado](actual/06-ipc-eventos-y-estado.md)
- [CLI, D-Bus, MCP y plasmoid](actual/07-cli-dbus-mcp-y-plasmoid.md)
- [Herramientas de desarrollo](actual/08-herramientas-de-desarrollo.md)
- [Seguridad, permisos y límites](actual/09-seguridad-permisos-y-limites.md)

### Funciones

- [Ciclo de vida de proyectos](funciones/01-ciclo-de-vida-de-proyectos.md)
- [WordPress, WP-CLI y auto-login](funciones/02-wordpress-wpcli-autologin.md)
- [Dominios, endpoints y SSL](funciones/03-dominios-endpoints-y-ssl.md)
- [Bases de datos y migración](funciones/04-bases-de-datos-y-migracion.md)
- [Mailpit, MinIO, Adminer y servicios](funciones/05-mailpit-minio-adminer-y-servicios.md)
- [Backups, auto-dump y dump log](funciones/06-backups-autodump-y-dumplog.md)
- [Importación LocalWP y proyectos desconectados](funciones/07-importacion-localwp-y-proyectos-desconectados.md)
- [Git, GitHub, VS Code, terminal y deploy](funciones/08-git-github-vscode-terminal-y-deploy.md)
- [Puntos de guardado y clones](funciones/09-puntos-de-guardado-y-clones.md)
- [Worktree-projects](funciones/10-worktree-projects.md)
- [Grupos y UI master-detail](funciones/11-grupos-y-ui-master-detail.md)
- [Logs, progreso, mantenimiento y recuperación](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md)

### Operación

- [Instalación y primera ejecución](operacion/01-instalacion-y-primera-ejecucion.md)
- [Desarrollo, pruebas, build y empaquetado](operacion/02-desarrollo-pruebas-build-y-empaquetado.md)
- [Runbook de proyectos](operacion/03-runbook-proyectos.md)
- [Importación, migración y recuperación](operacion/04-runbook-importacion-migracion-y-recuperacion.md)
- [Snapshots, clones y worktrees](operacion/05-runbook-snapshots-clones-y-worktrees.md)
- [Git, CLI y MCP](operacion/06-runbook-git-cli-y-mcp.md)
- [Diagnóstico y mantenimiento](operacion/07-diagnostico-y-mantenimiento.md)

### Referencia

- [Esquemas y archivos persistidos](referencia/01-esquemas-y-archivos-persistidos.md)
- [Containers, imágenes, mounts y puertos](referencia/02-contenedores-imagenes-montajes-y-puertos.md)
- [Catálogo IPC y eventos](referencia/03-catalogo-ipc-y-eventos.md)
- [Catálogo D-Bus, CLI y MCP](referencia/04-catalogo-dbus-cli-y-mcp.md)
- [Dependencias, comandos y prerrequisitos](referencia/05-dependencias-comandos-y-prerrequisitos.md)

### Historia

- [Changelog consolidado](historia/01-changelog-consolidado.md)
- [Decisiones y fixes críticos](historia/02-decisiones-y-fixes-criticos.md)
- [Limitaciones, deuda y trabajo diferido](historia/03-limitaciones-deuda-y-trabajo-diferido.md)

### Reconstrucción

- [Objetivos, principios y no objetivos](reconstruccion/01-objetivos-principios-y-no-objetivos.md)
- [Arquitectura objetivo](reconstruccion/02-arquitectura-objetivo.md)
- [Contratos, estado y persistencia](reconstruccion/03-contratos-estado-y-persistencia.md)
- [Orquestación Docker y operaciones](reconstruccion/04-orquestacion-docker-y-operaciones.md)
- [Seguridad, observabilidad y recuperación](reconstruccion/05-seguridad-observabilidad-y-recuperacion.md)
- [Estrategia de pruebas y calidad](reconstruccion/06-estrategia-de-pruebas-y-calidad.md)
- [Roadmap, backlog y migración](reconstruccion/07-roadmap-backlog-y-migracion.md)

### Verificación

- [Matriz de trazabilidad](verificacion/01-matriz-de-trazabilidad.md)
- [Prueba de aceptación de reconstrucción](verificacion/02-prueba-de-aceptacion-de-la-reconstruccion.md)

## Principios actuales que no deben perderse

- proyecto parado consume cero containers del panel;
- servicios equivalentes se comparten;
- PHP queda aislado por proyecto y no publica puertos host;
- DB usa almacenamiento durable;
- proyecto puede reconstruirse desde su carpeta y config;
- backend es autoridad de orquestación;
- herramientas externas son adaptadores, no núcleos alternativos;
- worktrees aíslan rama sin duplicar WordPress;
- dumps aportan recuperación independiente del datadir.

## Advertencias de lectura

- `PLAN.md` mezcla visión original y futuro. No todo existe.
- `docs/CHANGELOG.md` preserva decisiones superadas.
- Código actual siempre selecciona puertos altos para nginx.
- Fase 5 IA no está implementada.
- Snapshot actual no ofrece restore in-place.
- Deploy directo actual actúa sobre checkout local; no despliega VPS/cPanel/AWS.

## Glosario mínimo

| Término | Uso en este proyecto |
|---|---|
| **Proyecto/sitio** | Carpeta WordPress + `SiteConfig` + schema DB + container PHP cuando está activo. |
| **Servicio compartido** | Container reutilizado por varios proyectos: nginx, DB por versión, Mailpit, MinIO o Adminer. |
| **Dump** | Export SQL portable de una base de datos. |
| **Punto de guardado/snapshot** | `code.tar.zst`, `db.sql` y `meta.json`; base para clone. |
| **Clone** | Sitio temporal materializado desde snapshot, con schema propio. |
| **Worktree-project** | Sitio de prueba que comparte WordPress padre y sobrepone repo Git + wp-config. |
| **Proyecto desconectado** | Carpeta excluida del escaneo por ausencia de `config.json`, normalmente con sidecar conservado. |
| **Endpoint** | IP loopback y puertos globales publicados por nginx. |
| **Reconciliación** | En rebuild: converger estado real Docker/filesystem al estado deseado. |
| **Journal de operación** | En rebuild: registro durable de fases para recuperar operación interrumpida. |

## Actualización

Al cambiar comportamiento:

1. actualizar catálogo canónico;
2. actualizar ficha funcional;
3. actualizar runbook;
4. actualizar trazabilidad y test;
5. añadir decisión/changelog cuando cambie invariante.

Ver [Método y alcance](00-metodo-y-alcance.md) para reglas completas.
