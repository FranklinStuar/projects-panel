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

*(Pendiente: se documenta junto con la infraestructura de mock IPC + Playwright.)*
