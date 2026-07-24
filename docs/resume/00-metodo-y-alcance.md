# Método, alcance y convenciones

> **Estado:** `CURRENT-CONFIRMED` para hechos contrastados con código en la rama analizada.  
> **Corte documental:** 2026-07-23 · rama `main` · commit `373841c`.

## Propósito

Este conjunto describe Panel WP con dos objetivos separados:

1. **Entender y operar el sistema actual** sin depender de conocimiento oral.
2. **Reconstruirlo desde cero** conservando sus decisiones valiosas y evitando deuda acumulada.

No es una propuesta de parche sobre el código actual. Tampoco convierte ideas futuras en funciones existentes.

## Alcance

Incluye:

- producto, arquitectura, módulos y flujo de datos;
- frontend SvelteKit y backend Rust/Tauri;
- Docker, red, imágenes, containers, montajes y persistencia;
- ciclo de vida completo de proyectos WordPress;
- dominios `.test`, endpoint y SSL local;
- MySQL, MariaDB, PostgreSQL, Mailpit, MinIO y Adminer;
- WP-CLI, terminal, VS Code, Git/GitHub y deploy directo local;
- backups, auto-dump, log de volcados, migración y recuperación;
- importación desde LocalWP y reimportación de carpetas desconectadas;
- puntos de guardado, clones y worktree-projects;
- UI master-detail, grupos, logs y mantenimiento;
- IPC Tauri, eventos, D-Bus, CLI, MCP y plasmoid KDE;
- instalación, desarrollo, testing, build y empaquetado;
- historia, fixes críticos, deuda y funciones diferidas;
- arquitectura objetivo y roadmap para nueva implementación.

## Fuera de alcance como función actual

Estos temas aparecen como futuro o contexto, no como capacidad terminada:

- asistente IA de Fase 5 (`agent.rs` no existe);
- container frontend real para modo headless;
- plugin S3 que conecte WordPress con MinIO;
- deploy a VPS, Bitnami/AWS o cPanel mediante SSH/plugin receptor;
- Cloudflare Tunnel y empaquetado de producción mostrados como stubs;
- soporte multiplataforma equivalente al actual flujo Linux/KDE/NetworkManager.

`ideas-cambios.md` aporta contexto para rebuild y futuro producto VPS. No prueba comportamiento actual.

## Jerarquía de evidencia

Cuando fuentes discrepan, usar este orden:

1. **Código y configuración ejecutable actual.**
2. **Tests automatizados.**
3. **Scripts operativos y assets Docker.**
4. **Documentación de arquitectura vigente.**
5. **Changelog e historial Git.**
6. **PLAN.md e ideas futuras.**

Ejemplos de contradicciones resueltas con esta regla:

- `docker.rs::autoselect_endpoint` siempre cede 80/443 y elige puertos altos desde 8080/8443; textos históricos describen una estrategia anterior.
- `GTK_CSD=0` fue revertido; comentarios actuales y `docs/KNOWN_ISSUES.md` lo confirman.
- fuente de verdad actual es `config.json` por proyecto; no existe registro central `sites.json`.
- `ports.rs`, `shutdown.rs` y `agent.rs` aparecen en planes antiguos, pero no son módulos actuales.

## Etiquetas de estado

| Etiqueta | Significado |
|---|---|
| `CURRENT-CONFIRMED` | Existe y fue comprobado por lectura de código/configuración. |
| `CURRENT-UNVERIFIED` | Existe en código, pero necesita validación live o visual. |
| `DEBT` | Funciona, pero su diseño dificulta mantenimiento, seguridad o evolución. |
| `DEFERRED` | Se decidió posponer; puede haber flags o UI parcial. |
| `STUB` | Superficie visible que devuelve “no implementado”. |
| `TARGET` | Diseño recomendado para reconstrucción; no describe runtime actual. |
| `IDEA` | Solicitud o posibilidad futura sin implementación confirmada. |

## Convención de referencias

Las fuentes se citan por ruta y símbolo, por ejemplo:

- `src-tauri/src/docker.rs::DockerManager::start_site`
- `src-tauri/src/config.rs::SiteConfig`
- `src/lib/api.ts::api`
- `src/lib/components/ProjectDetail.svelte`

Se evitan números de línea como referencia única porque cambian con frecuencia. Cada documento técnico termina con **Fuentes primarias**.

## Diferencia entre estados de una capacidad

No mezclar estos conceptos:

- **Implementada:** existe lógica ejecutable.
- **Expuesta:** alguna interfaz permite invocarla.
- **Con paridad:** UI, IPC, D-Bus, CLI y MCP ofrecen comportamiento equivalente.
- **Probada:** existe test automatizado o validación live registrada.
- **Operable:** hay runbook con precondiciones y recuperación.
- **Planificada:** solo aparece en plan, stub o backlog.

Una capacidad puede estar implementada en backend y no estar expuesta por MCP. Matrices de referencia registran esas diferencias.

## Reglas editoriales

1. Código actual manda sobre narrativa histórica.
2. No presentar comportamiento `best-effort` como garantía.
3. Marcar operaciones destructivas y sus límites de reversión.
4. Separar DB durable, dump SQL, punto de guardado y clone: resuelven problemas distintos.
5. No llamar “restore” a crear un clone desde snapshot; no existe restauración in-place implementada.
6. No llamar “deploy de producción” al deploy directo actual: hace checkout, `git pull --ff-only` y build en host local.
7. No asumir paridad entre IPC, D-Bus, CLI y MCP; comprobarla.
8. No copiar catálogos completos entre documentos. Referencia canónica vive en `referencia/`.
9. Usar nombres exactos actuales: `clone_of`, `worktree_of`, `migration_pending`, `panel-net`, `wp-{id}`, `panel-*`.
10. Recomendaciones de rebuild deben explicar problema actual que resuelven y trade-off introducido.

## Qué fue validado

### Validado por inspección estática

- estructura del repositorio;
- modelos y persistencia;
- comandos Tauri y eventos;
- topología Docker declarada;
- flujos backend/frontend;
- scripts CLI/MCP/first-run;
- tests existentes;
- documentación histórica y problemas conocidos.

### No revalidado live durante esta documentación

- ejecución real de containers y consumo de recursos;
- migraciones con dumps grandes;
- resolución DNS y certificados en máquina limpia;
- aspecto del plasmoid en Plasma;
- integración visual Tauri/KDE/Wayland;
- compatibilidad con todas las versiones de imágenes remotas;
- comportamiento de servicios externos o CLIs tras futuras actualizaciones.

Los runbooks indican cómo verificar esos puntos.

## Mantenimiento futuro

Cambio de contrato requiere actualizar, como mínimo:

1. catálogo canónico afectado en `referencia/`;
2. matriz funcional;
3. ficha de función;
4. runbook si cambia operación;
5. matriz de trazabilidad;
6. changelog consolidado;
7. test automatizado o criterio de verificación asociado.

## Fuentes primarias

- `CLAUDE.md`
- `PLAN.md`
- `docs/ARCHITECTURE.md`
- `docs/CHANGELOG.md`
- `docs/EXTENDING.md`
- `docs/TESTING.md`
- `docs/KNOWN_ISSUES.md`
- `ideas-cambios.md`
- `src-tauri/src/`
- `src/`
- `docker/`
- `scripts/`
- `mcp/`
- `plasma/`
