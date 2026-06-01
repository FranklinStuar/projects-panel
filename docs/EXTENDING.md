# Cómo agregar funciones

Recetas para extender el panel sin romper convenciones ni el principio rector
(ver `CLAUDE.md`). Tras cualquier cambio estructural, **actualiza
`ARCHITECTURE.md` y `CHANGELOG.md`**.

## Regla de oro de recursos

Cualquier servicio nuevo debe:
1. Arrancar **on-demand** (solo cuando un proyecto activo lo necesita).
2. **Compartirse** si varias instancias tendrían la misma config; instanciarse por
   proyecto solo si necesita aislamiento real (como php-fpm).
3. Apagarse en `teardown_unused_shared` cuando ya nadie lo use.
4. Usar imagen alpine si existe.

## Agregar un comando IPC

1. **Backend** (`src-tauri/src/lib.rs`):
   ```rust
   #[tauri::command]
   async fn mi_comando(arg: String) -> CmdResult<MiTipo> {
       let docker = DockerManager::connect().map_err(e)?;
       modulo::hace_algo(&docker, &arg).await.map_err(e)
   }
   ```
2. Regístralo en `invoke_handler![ ... , mi_comando]`.
3. **Tipos** (`src/lib/types.ts`): añade `MiTipo` en `camelCase` (espejo del serde).
4. **API** (`src/lib/api.ts`): `miComando: (arg: string) => invoke<MiTipo>('mi_comando', { arg })`.
   Los nombres de args en `invoke` deben coincidir con los parámetros Rust.

## Agregar un módulo Rust

1. Crea `src-tauri/src/mi_modulo.rs`.
2. Declara `mod mi_modulo;` en `lib.rs`.
3. Funciones públicas devuelven `anyhow::Result<T>`; el mapeo a `String` se hace
   en la capa de comando (`e()`), no en el módulo.

## Agregar una ruta/página frontend

1. Crea `src/routes/mi-ruta/+page.svelte` (Svelte 5 runes).
2. Si es navegable desde el menú, añádela al array `nav` en
   `src/routes/+layout.svelte`.
3. Recuerda: es SPA (`ssr=false`); usa `onMount` para cargar datos vía `api`.

## Agregar un servicio compartido (ej. MinIO, Mailpit)

En `docker.rs`:
1. Añade constante de nombre (`pub const MINIO: &str = "panel-minio";`).
2. `ensure_minio(&self)` siguiendo el patrón de `ensure_nginx`: comprobar
   `is_running` → `ensure_image` → si existe parado arrancar, si no `create_container`
   en `NETWORK`, publicar puerto al host solo si el usuario debe acceder (UI).
3. Llama `ensure_minio` desde `start_site` **solo si el proyecto lo pide**
   (flag en `config.json`).
4. Apágalo en `teardown_unused_shared` cuando ningún activo lo use.

## Agregar un motor/versión de base de datos

1. `config.rs`: extiende `DbType` (variante nueva) + `image()`, `service_prefix()`,
   `port()`.
2. `wordpress.rs::create_database`: añade la rama de creación de DB para ese motor.
3. `docker.rs::db_env`: variables de entorno del container.
4. Frontend `site/new/+page.svelte`: añade opción y versiones en `DB_VERSIONS`.
5. `wp-config`: si el driver difiere (postgres requiere plugin en WP), documenta.

## Agregar una versión de PHP

1. Frontend: añade a `PHP_VERSIONS` en `site/new/+page.svelte`.
2. La imagen se construye sola (`php.rs::ensure_php_image` usa `--build-arg
   PHP_VERSION`). Verifica que `php:{ver}-fpm-alpine` exista en Docker Hub.

## Agregar configuración a un proyecto (campo en config.json)

1. `config.rs`: añade el campo a `SiteConfig` (o sub-struct en `Services`), con
   `#[serde(default)]` si debe ser retrocompatible con configs viejas.
2. `types.ts`: refleja el campo.
3. Úsalo donde toque (docker/nginx/wordpress). Regenera/migra configs si hace falta.

## Logs en vivo (patrón para Fase 2)

- bollard expone `logs()` como stream. Envuélvelo en un módulo `logs.rs` y emítelo
  al frontend con Tauri events (`app.emit`), suscrito desde Svelte con
  `@tauri-apps/api/event`. No bloquear el hilo de comandos.

## Proveedor de IA (Fase 5, `agent.rs`)

- La API key va al **keyring del SO** (libsecret/Keychain), nunca en texto plano.
- Define un trait `Provider` con `chat()/tool_use()`; implementa por proveedor.
- Herramientas de escritura **siempre piden aprobación** (mostrar diff/comando)
  antes de aplicar. Ver sección "Agentes de IA" en `PLAN.md`.

## Añadir un test (ver `docs/TESTING.md`)

- **Lógica pura (Rust)**: `#[cfg(test)] mod tests` en el mismo módulo (accede a
  funciones privadas). Corre con `cd src-tauri && cargo test`.
- **Con Docker o que muta el entorno**: en `src-tauri/src/integration_tests.rs`,
  `#[tokio::test]` + `#[ignore]`, nombre `zztest-*`, limpiando lo propio. Corre
  con `cargo test -- --ignored --test-threads=1`. Si necesitas `&AppHandle` usa
  `tauri::test::mock_app()` (las fns que emiten `op-log` son genéricas sobre
  `Runtime`).
- **GUI (escenario de clics)**: añade un mock para el comando en
  `src/lib/dev/mock-ipc.ts` (devuelve copias frescas, no la referencia mutada) y
  un spec en `e2e/`. Documenta el orden de clics en `docs/TESTING.md §C`. Corre
  con `pnpm test:e2e`. Usa `{ exact: true }` para nombres de rol que sean
  subcadena de otros.

## Checklist al terminar un cambio

- [ ] Compila: `cd src-tauri && cargo build` y `pnpm build`.
- [ ] Tipos TS sincronizados con modelos serde.
- [ ] Comando registrado en `invoke_handler!` y en `api.ts`.
- [ ] Servicios nuevos respetan la regla de recursos (on-demand + teardown).
- [ ] Tests: `cargo test` y `pnpm test:e2e` verdes; si tocaste un flujo nuevo,
      añade su test (lógica) y/o escenario (GUI mock).
- [ ] `ARCHITECTURE.md` y `CHANGELOG.md` actualizados.
