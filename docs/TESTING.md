# TESTING — Panel WP

Dos vías de prueba, con necesidades distintas:

- **A. Sin panel** — verifica que la lógica y la funcionalidad del backend son
  correctas. Rápido (sin Docker) e integración (con Docker real).
- **B. Con panel** — verifica botones, ventanas y accesibilidad de la UI,
  sirviendo el SPA con `invoke()` mockeado (no necesita backend ni Docker).
  *(Ver más abajo; se documenta junto con la infraestructura de Playwright.)*

Este documento es el **historial replicable**: cómo correr cada cosa y, para la
GUI, el orden exacto de clics y los datos a llenar.

---

## A. Tests de lógica SIN panel (Rust)

### A.1 Unit puros — rápidos, sin Docker

```bash
cd src-tauri && cargo test
```

Corre en segundos, no toca Docker ni red. Cobertura:

| Módulo | Test | Qué verifica |
|--|--|--|
| `wordpress.rs` | `slugify_*` | minúsculas, símbolos→`-`, colapso de guiones, trim, dígitos/unicode |
| `localwp.rs` | `major_minor_*` | `"8.4.10"` → `"8.4"`; casos parciales/vacíos |
| `localwp.rs` | `pick_supported_*` | versión soportada sin ajuste; no soportada → última soportada + flag |
| `backup.rs` | `rotate_conserva_*` | deja los N dumps `db-*.sql` más recientes; ignora `imported.sql`/`local.sql` |
| `backup.rs` | `rotate_no_borra_*` | no borra si hay ≤ keep |
| `config.rs` | `site_url_cuatro_ramas` | URL con/sin SSL × puerto estándar/alterno |
| `config.rs` | `container_name_y_sql_dir` | `wp-{id}`, ruta `app/sql` |
| `config.rs` | `*_camelcase` / `*_roundtrip` | claves serde en **camelCase** (espejo de `types.ts`) y deserialización sin pérdida |
| `netcheck.rs` | (existentes) | parseo de `/proc/net/tcp`, detección de puerto libre |

### A.2 Integración — con Docker real

Están marcados `#[ignore]`: **no** corren en `cargo test`. Para ejecutarlos:

```bash
cd src-tauri && cargo test -- --ignored --test-threads=1
```

`--test-threads=1` es **obligatorio**: redirigen variables de entorno del
proceso (`HOME`, `XDG_CONFIG_HOME`) o tocan infraestructura Docker compartida.

Viven en `src-tauri/src/integration_tests.rs` (dentro del crate, para usar los
módulos privados). Cada uno usa nombres `zztest-*` y limpia lo suyo.

| Test | Docker | Qué hace |
|--|--|--|
| `import_localwp_hermetico` | **No** | Monta un `HOME` temporal con `sites.json` + un sitio LocalWP falso, llama `localwp::import_site`, comprueba el `config.json` `migrationPending=true`, la copia de `app/public` y del dump (`imported.sql`). Totalmente aislado. |
| `db_lifecycle_idempotente` | Sí | `ensure_db` + `create_database` dos veces (idempotente); la DB compartida queda arriba. |
| `crear_exportar_migrar_e2e` | Sí | `create_site` real (descarga WordPress, 1ª vez construye `panel-php`) → `export_db` → `rotate_dumps` → `migrate_site`; al final apaga y borra el proyecto. |

**Prerequisitos de los tests con Docker:**
- Docker corriendo y accesible.
- Red `panel-net` (la crea `ensure_network`, o `bash scripts/first-run.sh`).
- Salida a internet (descarga del core de WordPress).

El test hermético de import no necesita nada de esto y se puede correr suelto:

```bash
cargo test --lib integration_tests::import_localwp_hermetico -- --ignored --exact
```

### A.3 Cómo añadir un test de lógica

- **Función pura** (sin Docker/FS): añade un `#[cfg(test)] mod tests` en el mismo
  módulo (así accede a funciones privadas, p. ej. `major_minor` en `localwp.rs`).
- **Con FS pero sin Docker**: usa `tempfile::tempdir()` y, si la función resuelve
  rutas con `dirs::` (`home_dir`/`config_dir`), redirige `HOME`/`XDG_CONFIG_HOME`
  al temporal — patrón en `import_localwp_hermetico`. Márcalo `#[ignore]`.
- **Con Docker**: añádelo a `integration_tests.rs`, `#[tokio::test]` + `#[ignore]`,
  nombre `zztest-*`, y limpia con `teardown(&docker, &site)`.
- **Progreso (`op-log`)**: las funciones que reciben `&AppHandle` son genéricas
  sobre el runtime (`<R: Runtime>`), así que en tests se les pasa
  `tauri::test::mock_app().handle()`.

---

## B. Tests CON panel (GUI)

El SPA llama a `invoke()` en cada acción, así que sin backend no arranca. Para
probar **botones, ventanas y accesibilidad** sin Tauri ni Docker, se sirve el
frontend con una capa de IPC simulada (`mockIPC`) que responde con fixtures.

### B.1 Arrancar el panel en modo mock

```bash
pnpm dev:mock      # = VITE_MOCK_IPC=1 vite dev   → http://localhost:1420
```

Ábrelo en el navegador. La consola del navegador imprime `[mock-ipc] activo`.
Verás 3 proyectos de muestra (corriendo / parado / pendiente de migración), el
endpoint en puerto alterno, el estado del sistema y 2 sitios de LocalWP. Nada es
real: las acciones mutan un estado en memoria que se reinicia al recargar.

Piezas:
- `src/lib/dev/fixtures.ts` — datos de muestra (sitios, endpoint, system status,
  LocalWP, versiones WP).
- `src/lib/dev/mock-ipc.ts` — `mockIPC` que responde cada comando de `api.ts`;
  los flujos largos (migrar/importar/borrar) emiten líneas `op-log` con retardo,
  así la consola `OpConsole` se ve poblándose en vivo.
- `src/routes/+layout.ts` — carga el mock solo si `VITE_MOCK_IPC=1` (en
  `pnpm tauri dev` real no se incluye).

### B.2 Tests automatizados (Playwright)

```bash
pnpm test:e2e                       # headless; arranca el SPA mock solo
pnpm exec playwright test --ui      # modo interactivo
pnpm exec playwright show-report    # último informe HTML
```

`playwright.config.ts` levanta `pnpm dev:mock` automáticamente (`webServer`).
**Cuidado:** `reuseExistingServer` está activo; si ya tienes un `vite dev`
NORMAL (sin mock) en el 1420, Playwright lo reutiliza y los tests fallan sin
datos. Cierra cualquier `vite dev` previo antes de correr los e2e.

Specs en `e2e/` (un escenario por archivo): `dashboard`, `migrate`,
`cancel-import`, `delete-site`, `settings`, `new-site`, `a11y`.

Notas para escribir specs:
- Los `confirm()` se aprueban con `page.on('dialog', (d) => d.accept())`. El
  borrado de proyecto **no** usa `confirm()` nativo sino un modal propio
  (`role="dialog"`): scopea los botones con `page.getByRole('dialog')` para no
  chocar con los botones homónimos de las tarjetas.
- Usa `{ exact: true }` en nombres de rol que sean subcadena de otros
  (`Encender` casa con `Migrar y encender`; `Proyectos` con el enlace
  `← Proyectos`).

### B.3 Inspección manual / asistida

El mismo SPA mock se puede recorrer a mano en el navegador, o conducir con las
herramientas MCP de Playwright (navegar, snapshot del árbol de accesibilidad,
screenshots) para revisar botones y ventanas.

---

## C. Escenarios replicables (historial de clics)

Estado inicial del modo mock (`pnpm dev:mock`):

| Proyecto | Grupo | Estado |
|--|--|--|
| Tienda Demo | Cliente A | corriendo |
| Blog Personal | Cliente A | parado |
| Sitio Importado | LocalWP | pendiente de migración |

LocalWP: «Proyecto Viejo» (importable) y «Sitio Importado» (ya importado).

### C.1 Crear un proyecto

| Paso | Acción | Dato |
|--|--|--|
| 1 | Click «Nuevo proyecto» (o ir a `/site/new`) | — |
| 2 | Nombre del proyecto | `Mi Nuevo Sitio` |
| 3 | Verificar dominio autocompletado | `mi-nuevo-sitio.test` |
| 4 | Versión WP | (ya viene la `latest`) |
| 5 | PHP / Motor DB / Versión DB | `8.3` / MySQL / `8.0` |
| 6 | Contraseña | `secret123` |
| 7 | Email | `admin@demo.test` |
| 8 | Click «Crear proyecto» | — |
| ✔ | Vuelve al dashboard y aparece «Mi Nuevo Sitio» (parado) | |

### C.2 Encender / Detener

| Paso | Acción |
|--|--|
| 1 | En «Blog Personal» (parado) click «Encender» |
| ✔ | El botón pasa a «Detener»; el punto se pone verde |

### C.3 Migrar (consola de progreso)

| Paso | Acción |
|--|--|
| 1 | En «Sitio Importado» click «Migrar y encender» |
| 2 | Confirmar el diálogo |
| ✔ | Se abre la consola «Migración»; «Cerrar» está deshabilitado |
| ✔ | Llegan líneas en vivo (…«Importando base de datos»…) |
| ✔ | Al terminar: «✓ … migrado y encendido», «Cerrar» se habilita |
| 3 | Click «Cerrar» → el sitio queda corriendo |

### C.4 Cancelar importación

| Paso | Acción |
|--|--|
| 1 | En «Sitio Importado» (pendiente) click «Cancelar» |
| 2 | Confirmar el diálogo (avisa que borra la carpeta) |
| ✔ | El proyecto desaparece de la lista (y su grupo LocalWP) |

### C.5 Importar desde LocalWP

| Paso | Acción |
|--|--|
| 1 | Ir a `/settings` → sección «Importar desde LocalWP» |
| 2 | En «Proyecto Viejo» click «Importar» |
| ✔ | Se abre la consola «Importar desde LocalWP» con progreso |
| ✔ | Mensaje de éxito; el sitio queda pendiente en Proyectos |

### C.6 Configuración

| Paso | Acción |
|--|--|
| 1 | Ir a `/settings` |
| ✔ | Checklist: Docker/Red ✓, Wrappers/Plasmoid ✗ (fixtures) |
| ✔ | Endpoint con badge «puerto alterno» |
| 2 | Click «Instalar» en Wrappers WP-CLI → pasa a ✓ |

> En modo mock no hay IPC ni Docker reales (todo son fixtures). Para validar el
> flujo end-to-end de verdad usa `pnpm tauri dev` + los tests `#[ignore]` de §A.2.
