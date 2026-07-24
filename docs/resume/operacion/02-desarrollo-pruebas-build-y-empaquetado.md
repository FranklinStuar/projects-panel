# Desarrollo, pruebas, build y empaquetado

## Principio de uso

Separa siempre estas cuatro cosas:

1. **UI mock**: prueba la SPA y sus flujos sin Tauri ni Docker.
2. **Aplicación real en desarrollo**: Vite + ventana Tauri + backend Rust + Docker.
3. **Builds parciales**: frontend estático o binario Rust, útiles para detectar errores.
4. **Bundle release**: aplicación Tauri empaquetada, más el plasmoid por separado.

Fuentes: `package.json::scripts`, `playwright.config.ts::webServer`, `docs/TESTING.md`, `src-tauri/tauri.conf.json::build`, `src-tauri/tauri.conf.json::bundle`, `.claude/commands/deploy.md::/deploy` y `scripts/package-plasmoid.sh`.

## Matriz rápida

| Objetivo | Comando | Backend/Docker | Artefacto o evidencia |
|---|---|---:|---|
| Typecheck Svelte | `pnpm check` | No | código 0 |
| Check Rust | `cd src-tauri && cargo check` | No | código 0 |
| UI mock manual | `pnpm dev:mock` | No | `http://localhost:1420` |
| Unit Rust | `cd src-tauri && cargo test` | No | tests no ignorados verdes |
| Integración Rust | `cd src-tauri && cargo test -- --ignored --test-threads=1` | Sí (suite parcial) | tests ignorados verdes |
| E2E UI | `pnpm test:e2e` | No | reporte Playwright |
| App real dev | `pnpm tauri dev` | Sí | ventana Tauri |
| Frontend estático | `pnpm build` | No | `build/` |
| Backend debug | `cd src-tauri && cargo build` | No | `src-tauri/target/debug/wordpress-panel` |
| Bundle release | `NO_STRIP=1 pnpm tauri build` | No al compilar | `src-tauri/target/release/bundle/` |
| Plasmoid | `bash scripts/package-plasmoid.sh` | No | `dist/wordpress-panel.plasmoid` |

Las superficies UI/`wordpress-panel-cli`/MCP no construyen ni prueban el panel. Son operaciones de la aplicación ya en ejecución. Aquí la superficie operativa principal es la shell; la excepción es la inspección manual de la UI mock o real.

## 1. UI mock sin backend

Sirve la SPA con un mock de Tauri IPC en `http://localhost:1420`; los flujos largos (migración, borrado, import, snapshot, deploy) emiten líneas `op-log` con retardo para ver la consola en vivo.

```bash
pnpm dev:mock
```

### Cambio esperado

- En la consola del navegador aparece `[mock-ipc] activo`.
- La lista maestra muestra los proyectos de `src/lib/dev/fixtures.ts` (corriendo / parado / pendiente de migración) y el endpoint con puerto alterno.

### Evidencia

```bash
curl -sS -o /dev/null -w '%{http_code}\n' http://localhost:1420
```

### Precondiciones, abortar y recuperar

- Precondición: el puerto 1420 está libre. Si ya hay un Vite en 1420, se sirve ese y los flujos de la SPA fallan.
- `Ctrl+C` detiene el servidor mock; la SPA pierde la conexión inmediatamente.
- Si Vite no salió al ejecutar `pnpm test:e2e`, sal del mock manual primero. `playwright.config.ts::webServer.reuseExistingServer` está activo fuera de CI y reutilizaría un Vite sin `VITE_MOCK_IPC=1`.

## 2. Suite de tests sin Docker

Unit rápidos y reproducibles:

```bash
pnpm check
cd src-tauri && cargo test
```

### Cobertura documentada

- `wordpress.rs::slugify_*`
- `localwp.rs::major_minor_*`, `localwp.rs::pick_supported_*`
- `backup.rs::rotate_conserva_*`, `backup.rs::rotate_no_borra_*`
- `config.rs::site_url_cuatro_ramas`, `config.rs::container_name_y_sql_dir`, `config.rs::*_camelcase`, `config.rs::*_roundtrip`
- `netcheck.rs::v4_little_endian`, `netcheck.rs::listen_addr_matches_port_and_state`, `netcheck.rs::free_for_semantics`
- `nginx.rs::vhost_normal_sin_uploads_block`, `nginx.rs::vhost_clone_incluye_uploads_fallback_http`, `nginx.rs::vhost_clone_incluye_uploads_fallback_ssl`, `nginx.rs::vhost_worktree_root_padre_y_alias_objetivo`
- `clone.rs::find_free_slot_*`, `clone.rs::slugify_etiquetas`, `clone.rs::db_name_derivacion`
- `worktree.rs::valida_rama_y_sugiere`, `worktree.rs::slugify_ramas`, `worktree.rs::find_free_slot_evita_colisiones`, `worktree.rs::path_basename_objetivo`
- `dumplog.rs::clean_*`
- `github.rs::summarize_estados`
- `groups.rs::create_is_idempotent`, `groups.rs::delete_removes_from_list`

### Cambio esperado y evidencia

- Ambos comandos terminan con código 0.
- `cargo test` imprime `test result: ok` con la lista verde. Si un test falla, conserva el primer panic y la traza.

### Abortar y recuperar

- `Ctrl+C` es seguro; relanza el comando.
- Un fallo en `pnpm check` suele ser de tipos Svelte/TS; en `cargo test`, lee el primer error: si es un test de integración `zztest-*`, no estaba en `#[ignore]` y requiere Docker (raro pero posible tras una edición).

## 3. Tests e2e con mock

```bash
pnpm test:e2e
```

Playwright arranca `pnpm dev:mock` por su cuenta. La configuración se documenta en `playwright.config.ts::webServer` y los specs en `e2e/`.

### Precondiciones

- Ningún `vite dev` clásico en 1420 (reutilización activada fuera de CI).
- `pnpm install` ya ejecutado (necesario para `@playwright/test`).

### Cambio esperado

Reportes en consola y en `playwright-report/`. Especificaciones que pasan: `dashboard`, `new-site`, `migrate`, `cancel-import`, `import-project`, `delete-site`, `settings`, `a11y`.

### Evidencia

```bash
ls playwright-report/
```

### Abortar y recuperar

- `Ctrl+C` detiene Playwright y el dev:mock subyacente.
- Si el primer spec falla con un error de selector, relee la nota sobre `{ exact: true }` para `Migrar y encender` y `Encender` en `docs/TESTING.md::B.2`.
- Si Playwright no puede reiniciar el Vite, sal del proceso huérfano:

  ```bash
  pkill -f 'vite dev' || true
  ```

## 4. Tests de integración con Docker

```bash
cd src-tauri && cargo test -- --ignored --test-threads=1
```

`--test-threads=1` es obligatorio: los tests `#[ignore]` redirigen `HOME`/`XDG_CONFIG_HOME` o tocan infraestructura Docker compartida.

### Tests definidos en `integration_tests.rs`

- `import_localwp_hermetico`: no necesita Docker, pero requiere `HOME` escribible; redirige variables de entorno.
- `list_e_import_disconnected_hermetico`: idem.
- `db_lifecycle_idempotente`: requiere Docker.
- `crear_exportar_migrar_e2e`: requiere Docker; descarga el core de WordPress y lo limpia al terminar.

### Precondiciones

- Docker corriendo y `panel-net` disponible (lo crea `scripts/first-run.sh` o `create_panel_network`).
- Salida a internet (descarga del tarball).
- Sin otros proyectos de panel encendidos: la suite usa containers efímeros pero no debe interferir con `wp-*` reales.

### Abortar y recuperar

- `Ctrl+C` interrumpe; el siguiente test puede quedar en estado parcial. Inspecciona con `docker ps -a | grep panel-` y `docker ps -a | grep wp-` antes de reejecutar.
- `integration_tests::import_localwp_hermetico` se puede correr aislada:

  ```bash
  cargo test --lib integration_tests::import_localwp_hermetico -- --ignored --exact
  ```

## 5. Aplicación real en desarrollo

```bash
pnpm tauri dev
```

`src-tauri/tauri.conf.json::build.beforeDevCommand` lanza Vite en `http://localhost:1420`; Tauri abre la ventana y conecta el IPC. `src-tauri/src/lib.rs::run::setup` instala los wrappers y arranca el servidor D-Bus si hay sesión; además engancha los watchers de auto-dump para cualquier container de proyecto que ya esté activo en la sesión.

En Wayland, si la ventana sale en blanco:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

### Cambio esperado y evidencia

- Abre **Panel WP**.
- `gdbus introspect --session --dest com.goldmediatech.WordpressPanel --object-path /com/goldmediatech/WordpressPanel` devuelve el árbol de la interfaz `Manager`.
- Con cero proyectos activos, `docker ps --format '{{.Names}}' | grep -E '^(wp-|panel-)'` no devuelve nada.

### Abortar y recuperar

- `Ctrl+C` detiene Vite/Tauri. Los containers ya en marcha no se detienen automáticamente: páralos desde la UI, el CLI o `migrate_site`/auto-dump antes de cerrar.
- Si la consola del navegador dice "Error connecting to Tauri": confirma que la ventana Tauri está abierta; Vite por sí solo no expone IPC.
- Si el puerto 1420 está ocupado, mata el Vite previo:

  ```bash
  ss -ltnp 'sport = :1420'
  ```

## 6. Builds parciales

```bash
pnpm build                # frontend estático
cd src-tauri && cargo build
```

### Cambio esperado y evidencia

- `build/index.html` y `build/_app/` aparecen en la raíz del repo.
- `src-tauri/target/debug/wordpress-panel` queda compilado. Si usas `cargo build --release`, queda en `target/release/`.

### Abortar y recuperar

- `Ctrl+C` durante `pnpm build` deja `build/` parcial; borra con `rm -rf build` y relanza.
- `cargo build` interrumpe; relanza para retomar.

## 7. Bundle release

```bash
NO_STRIP=1 pnpm tauri build
bash scripts/package-plasmoid.sh
```

`NO_STRIP=1` es necesario en Manjaro/Arch: el `strip` bundleado de linuxdeploy no soporta la sección `.relr.dyn` de las libs modernas del sistema; sin esto el build de AppImage puede fallar al final.

### Cambio esperado y evidencia

- En `src-tauri/target/release/bundle/` aparecen `appimage/Panel WP_*.AppImage`, `deb/Panel WP_*.deb` y `rpm/Panel WP-*.rpm`.
- `dist/wordpress-panel.plasmoid` se crea con un zip que contiene `metadata.json` y `contents/`.
- En **Configuración** ya instalados: la versión del plasmoid empata con `package.json::version` (`0.1.0`); los binarios del AppImage/deb funcionan lanzando `panel-wp` con la variable `WEBKIT_DISABLE_DMABUF_RENDERER=1` en sesiones Wayland (ver `.claude/commands/deploy.md`).

### Precondiciones

- `pnpm build` ya ejecutado (Tauri lo invoca automáticamente por `beforeBuildCommand`).
- `zip` y `kpackagetool6` para el plasmoid; ausencia de `kpackagetool6` no rompe el `.plasmoid`.
- `dpkg` o `appimaged`/FUSE si vas a instalar el bundle.

### Abortar y recuperar

- Cancela a mitad: borra `src-tauri/target/release/bundle/` y `dist/` y relanza.
- Si el AppImage sale corrupto, comprueba que `NO_STRIP=1` está exportado; en Arch, `ldd` sobre el binario debería listar las libs del sistema, no las de linuxdeploy.

## 8. Instalación local del release (resumen)

Ruta de la opción A (paquetes):

```bash
sudo dpkg -i "src-tauri/target/release/bundle/deb/Panel WP_"*".deb"
kpackagetool6 --type Plasma/Applet --upgrade dist/wordpress-panel.plasmoid
```

Ruta de la opción B (AppImage, recomendada en Manjaro/Arch) y los detalles completos están en `.claude/commands/deploy.md`.

### Cambio esperado y evidencia

- `~/.local/bin/panel-wp.AppImage` con permisos `+x`.
- `~/.local/share/applications/panel-wp.desktop` con `Exec=env WEBKIT_DISABLE_DMABUF_RENDERER=1 ...AppImage`.
- `panel-wp` aparece en el menú de aplicaciones y abre con el plasmoid (si se instaló) listando proyectos.

### Abortar y recuperar

- `sudo dpkg -r panel-wp` desinstala el paquete.
- Para revertir la opción B, borra los archivos en `~/.local/bin`, `~/.local/share/applications` y `~/.local/share/icons/hicolor/.../apps/panel-wp.*` que haya creado el flujo.

## 9. Instalación y reparación del wrapper WP-CLI

Lo normal es que el wrapper se instale al iniciar el panel (`src-tauri/src/lib.rs::run::setup` → `cli::install_cli_wrapper`). Si quieres forzarlo:

```bash
wordpress-panel-cli install-cli-wrapper
```

(equivalente al comando IPC `install_cli_wrapper`). Los binarios van a `~/.local/bin/wp` y `~/.local/bin/wordpress-panel-cli` con permisos `755`.

### Cambio esperado y evidencia

- `wp` detecta el proyecto por CWD y ejecuta WP-CLI en su container.
- Si `~/.local/bin` no está en el `PATH`, el comando avisa con la línea exacta a exportar.

### Abortar y recuperar

- No destructivo. Si una copia falla, repite.
- Si el wrapper llama a `docker exec` con un `wp-{id}` incorrecto, el binario falla; no toca datos en disco.

## 10. Criterio de salida de un ciclo de cambio

Considera el cambio completo cuando:

- `pnpm check`, `cargo check`, `cargo test`, `pnpm test:e2e` son verdes.
- Si tocaste infra Docker, `cargo test -- --ignored --test-threads=1` también.
- El panel real abre, o abre con `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- Si modificaste `docs/ARCHITECTURE.md` o `docs/CHANGELOG.md`, regeneraste los runbooks impactados en `docs/resume/`.

Pasar a la operación real se cubre en `docs/resume/operacion/03-runbook-proyectos.md` en adelante.
