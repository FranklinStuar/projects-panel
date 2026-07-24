# 05 · Dependencias, comandos y prerrequisitos

> Referencia verificada contra el commit `373841c` (rama `main`, 2026-07-23).
> Cubre crates Rust, paquetes npm, binarios del sistema, comandos de
> desarrollo, prerrequisitos por fase, variables de entorno y notas de
> deuda (DEFERRED). Cada binario/crack/package lleva la fuente (`ruta::símbolo`).

## 1. Crates Rust (`src-tauri/Cargo.toml`)

### 1.1 Dependencias de runtime

| Crate                          | Versión      | Features               | Símbolo/uso en el código                                                                                                |
| ------------------------------ | ------------ | ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `tauri`                        | `2`          | (vacías)               | `lib.rs::run`, todos los `#[tauri::command]`, `Manager`, `Emitter`, `AppHandle`, `State`.                                |
| `tauri-plugin-shell`           | `2`          | (default)              | `lib.rs:965` (`.plugin(tauri_plugin_shell::init())`). Sin uso directo en `lib.rs`.                                       |
| `tauri-plugin-opener`          | `2`          | (default)              | `lib.rs:964`, `app.opener().open_url/open_path` (`open_admin`, `open_site`, `open_folder`, `open_mailpit`, `open_minio`, `open_adminer`). |
| `tokio`                        | `1`          | `full`                 | `tokio::time::sleep`, `tokio::spawn`, `JoinHandle`, `tokio::task::JoinHandle`, `tokio::process::Command`.               |
| `bollard`                      | `0.18`       | (default)              | `Docker::connect_with_local_defaults`, `create_container`, `start_container`, `stop_container`, `inspect_container`, `create_exec`, `start_exec`, `create_image`, `list_networks`, `create_network`. |
| `futures-util`                 | `0.3`        | (default)              | `StreamExt` para `logs::spawn_stream`, `docker::ensure_image`, `bollard::exec` AsyncWriteExt.                            |
| `zbus`                         | `5`          | `default-features = false`, `tokio` | `dbus::serve` (Connection + interface). Feature `tokio`: el executor de zbus corre sobre el runtime Tokio del panel (ver comentario en `Cargo.toml:30-33`). |
| `serde`                        | `1`          | `derive`               | Todos los `#[derive(Serialize, Deserialize)]` (config, snapshot, github, dbus, etc.).                                  |
| `serde_json`                   | `1`          | (default)              | `serde_json::to_string/from_str`, `Value`, `json!`.                                                                     |
| `anyhow`                       | `1`          | (default)              | `Result<T>` en todos los módulos.                                                                                       |
| `thiserror`                    | `2`          | (default)              | (Disponibilidad; errores propios en su mayoría usan `anyhow!`.)                                                         |
| `uuid`                         | `1`          | `v4`, `serde`          | `Uuid::new_v4` (id de proyectos, snapshots, clones, worktrees, autologin tokens).                                       |
| `chrono`                       | `0.4`        | `serde`                | `Utc::now()`, `to_rfc3339`, `format!` para timestamps.                                                                  |
| `dirs`                         | `5`          | (default)              | `config_dir` (XDG), `home_dir`. `config_dir()`, `projects_root()`, `cli::local_bin()`, `system::mkcert_ca_installed`, `system::plasmoid_installed`. |
| `reqwest`                      | `0.12`       | `json`, `stream`       | Descargadores: `wp-cli.phar` (`php.rs:62-68`), `wordpress-{ver}.tar.gz` (`wordpress::download_core`), `wp-versions.json` (`wordpress::fetch_versions`). |

### 1.2 Dependencias de desarrollo

| Crate               | Versión | Features   | Uso                                                                                                  |
| ------------------- | ------- | ---------- | ---------------------------------------------------------------------------------------------------- |
| `tempfile`          | `3`     | (default)  | `tempfile::tempdir()` en tests de `backup::rotate_dumps`, `clone::find_free_slot`, `worktree::find_free_slot`. |
| `tauri` (dev)       | `2`     | `test`     | `tauri::test::mock_app()` para tests de integración (`migrate::run_migration`, `localwp::import_site`, `clone::create_clone`). |

### 1.3 Features / targets

- `[features] default = []` (placeholder).
- `crate-type = ["staticlib", "cdylib", "rlib"]` (Tauri 2).
- `edición 2021`, `rust-version = "1.77"`.

## 2. Paquetes npm (`package.json`)

### 2.1 Dependencias de runtime

| Paquete                  | Versión       | Uso                                                                                                |
| ------------------------ | ------------- | -------------------------------------------------------------------------------------------------- |
| `@tauri-apps/api`        | `^2.1.1`      | `invoke`, `listen`, `emit`, `UnlistenFn`.                                                          |

### 2.2 Dependencias de desarrollo

| Paquete                            | Versión       | Uso                                                                                          |
| ---------------------------------- | ------------- | -------------------------------------------------------------------------------------------- |
| `@playwright/test`                 | `^1.60.0`     | `playwright test` (`pnpm test:e2e`).                                                          |
| `@sveltejs/adapter-static`         | `^3.0.6`      | Build SvelteKit SPA.                                                                          |
| `@sveltejs/kit`                    | `^2.8.0`      | Framework.                                                                                    |
| `@sveltejs/vite-plugin-svelte`     | `^5.0.0`      | Plugin Vite.                                                                                  |
| `@tauri-apps/cli`                  | `^2.1.0`      | `pnpm tauri dev`, `pnpm tauri build`.                                                         |
| `@tsconfig/svelte`                 | `^5.0.4`      | TS config base.                                                                               |
| `@types/node`                      | `^25.9.1`     | Tipos Node.                                                                                   |
| `autoprefixer`                     | `^10.4.20`    | PostCSS.                                                                                       |
| `svelte`                           | `^5.2.0`      | Svelte 5 (runes).                                                                              |
| `svelte-check`                     | `^4.1.0`      | `pnpm check`.                                                                                  |
| `tailwindcss`                      | `^3.4.15`     | CSS.                                                                                           |
| `tslib`                            | `^2.8.1`      | Helpers TS.                                                                                    |
| `typescript`                       | `^5.6.3`      | Compilador.                                                                                    |
| `vite`                             | `^6.0.0`      | Dev server (`pnpm dev`) + build.                                                               |

### 2.3 Scripts npm

```jsonc
{
  "dev":           "vite dev",
  "build":         "vite build",
  "preview":       "vite preview",
  "check":         "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json",
  "tauri":         "tauri",
  "dev:mock":      "VITE_MOCK_IPC=1 vite dev",
  "test:e2e":      "playwright test"
}
```

`pnpm-workspace.yaml` declara el monorepo mínimo (`mcp/`). `mcp/package.json` no
declara dependencias (`server.mjs` usa solo APIs Node nativas).

## 3. Binarios del sistema

| Binario                          | Verificado en                     | Notas                                                                                                                                    |
| -------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `docker` (CLI)                   | `php::ensure_php_image`, `docker::migrate_db_to_volume`, `migrate::import_dump`, `wordpress-panel-cli` (sub-comandos `containers`/`resources`/`logs`) | Versión mínima: la que exponga la API usada por bollard 0.18.                                                                            |
| `gh`                             | `github::GhStatus`, `github::clone` | `gh --version`, `gh auth status`, `gh repo clone`.                                                                                       |
| `git`                            | `github::*`, `worktree::*`        | `git pull`, `git checkout`, `git fetch`, `git rev-list`, `git worktree add/remove`, `git branch -D`.                                       |
| `mkcert`                         | `ssl::generate`, `system::mkcert_ca_installed` | `mkcert -install` (en `scripts/first-run.sh`), `mkcert -cert-file/-key-file`.                                                            |
| `dnsmasq` (gestionado por NetworkManager) | `domain::install_wildcard`, `scripts/first-run.sh` | `dnsmasq` cargando `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf`.                                                                  |
| `pkexec` (polkit)                | `domain::install_wildcard`        | Diálogo gráfico para instalar la regla dnsmasq.                                                                                          |
| `tar`                            | `snapshot::create_snapshot`, `clone::create_clone`, `wordpress::download_core` | Soporta `--zstd` (libarchive reciente). `tar -xzf` y `tar -cf --zstd`.                                                                    |
| `cp`                             | `localwp::cp_contents`            | `cp -a` (preserva atributos).                                                                                                             |
| `curl` / `wget`                  | (no usado, `reqwest` cubre)     | Opcional.                                                                                                                                |
| `mysqldump` / `mysql` / `psql` / `pg_isready` | vía bollard en el container DB | `cli del` motor. Image la monta. (No es binario del host.)                                                                              |
| `konsole` / `gnome-terminal` / `xfce4-terminal` / `kitty` / `alacritty` / `x-terminal-emulator` | `cli::open_terminal_at` | Se prueban en orden; el primero que exista gana.                                                                                       |
| `code` / `codium` / `code-insiders` / `vscodium` | `github::open_vscode`                               | Se prueban en orden.                                                                                                                     |
| `x-terminal-emulator` (alternativa Debian) | `cli::open_terminal_at`         | Cae al final.                                                                                                                            |
| `xdg-open`                       | `scripts/wordpress-panel-cli.sh` (open folder) | Sin envoltorio.                                                                                                                          |
| `gdbus` / `qdbus6`               | `scripts/wordpress-panel-cli.sh`  | `gdbus` preferido, `qdbus6` fallback.                                                                                                    |
| `jq`                             | `scripts/wordpress-panel-cli.sh`  | Para formatear JSON en la salida.                                                                                                          |
| `python3`                        | `scripts/wordpress-panel-cli.sh`  | `ast.literal_eval` para des-escapar la salida de `gdbus`.                                                                                |
| `Node` (sin versión específica)  | `mcp/server.mjs`                  | `child_process.spawn`, `readline`, `fs.readdirSync/readFileSync`, `os.homedir`. Usa `process.stderr` para logs.                          |
| `Plasma` (KDE 5/6)               | plasmoid y `system::plasmoid_ok`  | Path: `~/.local/share/plasma/plasmoids/<id>/`.                                                                                            |

## 4. Comandos de desarrollo

### 4.1 Toolchain

```bash
# Frontend
pnpm install                                # deps frontend (incluye @tauri-apps/cli)
pnpm tauri dev                              # vite :1420 + ventana Tauri
pnpm build                                  # build frontend estático → build/
pnpm dev:mock                               # SPA con IPC mockeado (sin backend)
pnpm test:e2e                               # Playwright (necesita dev:mock)
pnpm check                                  # svelte-check

# Backend
cd src-tauri && cargo build                 # solo backend
cd src-tauri && cargo test                  # lógica pura, rápido, sin Docker
cd src-tauri && cargo test -- --ignored --test-threads=1   # integración (Docker)

# Sistema (una vez)
bash scripts/first-run.sh                   # panel-net, dnsmasq, mkcert CA
```

### 4.2 Wayland

```bash
# Si la ventana sale en blanco (Linux + WebKitGTK):
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

### 4.3 env vars/toggles documentados

| Env                              | Efecto                                                                                                          |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `PANEL_WP_ROOT`                  | Override de `~/panel-wp` para el CLI y el MCP (`scripts/wordpress-panel-cli.sh:14`, `mcp/server.mjs:21`).       |
| `WORDPRESS_PANEL_CLI`            | Path explícito al CLI para el servidor MCP (`mcp/server.mjs:24-27`).                                            |
| `WEBKIT_DISABLE_DMABUF_RENDERER` | `1` desactiva el renderer DMA del WebKitGTK (workaround para Wayland/Tauri).                                    |
| `VITE_MOCK_IPC`                  | `1` activa el modo `--mock` (Frontend con IPC simulado, ver `src/lib/dev`).                                       |
| `PUID` / `PGID`                  | Lo pasa el backend al container php (`docker::host_uid_gid`); default 82:82 (= `www-data` en la imagen alpine). |
| `SHELL`                          | `github::deploy` cae a `sh` si no está definida.                                                                |

## 5. Prerrequisitos por fase

### 5.1 Fase 1–2 (compose inicial)

- `docker` daemon accesible al usuario (grupo `docker` o socket 0660).
- `pnpm` + Node 18+.
- `cargo` 1.77+ y `rustup`.
- Wayland/X11 con `webkit2gtk-4.1` (lo trae `tauri` como link dinámico).

### 5.2 Fase 4 (system status / migración / first-run)

- `mkcert` (instalado y CA añadida por `scripts/first-run.sh`).
- `dnsmasq` cargando reglas de NetworkManager (`/etc/NetworkManager/dnsmasq.d/`).
- `pkexec` (polkit).
- `~/.config/wordpress-panel/` con permisos de escritura.

### 5.3 Fase 5 (snapshots + clones + worktree + GitHub + MCP)

- `git` con SSH key/credentials configuradas.
- `gh` autenticado (no obligatorio para usar repos ya clonados).
- `tar` con `--zstd` (libarchive ≥ 3.1).
- `mkcert` (snapshot/clone regeneran cert).
- `gdbus`/`qdbus6` y `jq` (CLI); `node` (MCP).

## 6. Estados de recursos y persistencia

Tabla resumen (versiones extendidas en `docs/resume/01-…` y `02-…`):

| Recurso                              | Persistido en                                                                                       |
| ------------------------------------ | --------------------------------------------------------------------------------------------------- |
| `SiteConfig`                         | `~/panel-wp/{slug}/config.json`                                                                      |
| `config.disconnected.json` (sidecar) | `~/panel-wp/{slug}/config.disconnected.json`                                                         |
| `groups.json`                        | `~/.config/wordpress-panel/groups.json`                                                              |
| `panel.json` (endpoint)              | `~/.config/wordpress-panel/panel.json`                                                               |
| `dump-log.jsonl`                     | `~/.config/wordpress-panel/dump-log.jsonl`                                                            |
| `wp-versions.json` (cache)           | `~/.config/wordpress-panel/wp-versions.json`                                                          |
| `wp-cli.phar`                        | `~/.config/wordpress-panel/wp-cli.phar`                                                               |
| `db-data/{container}`                | `~/.config/wordpress-panel/db-data/{container}`                                                      |
| `minio-data`                         | `~/.config/wordpress-panel/minio-data`                                                               |
| `nginx/conf.d`                       | `~/.config/wordpress-panel/nginx/conf.d`                                                             |
| `dnsmasq-panel.conf` (texto)         | `~/.config/wordpress-panel/dnsmasq-panel.conf`                                                       |
| `00-panel-tuning.conf`               | `~/.config/wordpress-panel/nginx/conf.d/00-panel-tuning.conf`                                          |
| snapshots                            | `~/panel-wp/{slug}/snapshots/{sid}/`                                                                  |
| code-workspace                      | `~/panel-wp/{slug}/{safe}.code-workspace`                                                            |
| Plantilla php.ini                    | `docker/php.ini.tmpl` (asset del repo, no se persiste)                                                |

## 7. Tests

`docs/TESTING.md` detalla la estrategia. Resumen:

- Unit tests: `cargo test` (veloz, sin Docker). Cubren:
  - `config::Deserialize`/`route` (round-trip camelCase).
  - `nginx::render_vhost` (casos clone/worktree).
  - `backup::rotate_dumps`.
  - `dumplog::clean`.
  - `wordpress::slugify`, `clone::slugify`, `worktree::slugify`.
  - `netcheck::port_status` (parseo de `/proc/net/tcp`).
  - `github::summarize`.
- Tests `#[ignore]`: `cargo test -- --ignored --test-threads=1`. Reales:
  - `groups::create/delete/reorder` (mutan `config_dir`).
  - `migration::migrate_site` con `tauri::test::mock_app()`.
  - `localwp::import_localwp_hermetico` (mounte `HOME` temporal con `sites.json`).
- e2e: `pnpm test:e2e` (Playwright contra `pnpm dev:mock`).

> Los tests marcados `#[ignore]` anteriores al commit actual siguen
> funcionando (no se han cambiado los símbolos). Confirmados los archivos:
> `src-tauri/src/integration_tests.rs`, `src-tauri/src/groups.rs:124-158`.

## 8. Capability y permisos de Tauri

`src-tauri/capabilities/default.json` (líneas 1-10):

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": ["core:default", "core:event:default"]
}
```

- `windows: ["main"]` — la ventana definida en `tauri.conf.json` (`title: "Panel WP"`, `width: 1100`, `height: 720`, `minWidth: 800`, `minHeight: 560`, `decorations: true`).
- `core:event:default` — explica el comentario en el JSON: "para que el frontend
  pueda escuchar `op-log`".

`tauri.conf.json`:

```json
{
  "productName": "Panel WP",
  "version": "0.1.0",
  "identifier": "com.goldmediatech.WordpressPanel",
  "build": {
    "frontendDist": "../build",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [{ "title": "Panel WP", "width": 1100, "height": 720, "minWidth": 800, "minHeight": 560, "decorations": true }],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all", "icon": ["icons/icon.png"] }
}
```

## 9. Convenciones reiteradas (no romper)

- `agents/CONVENTIONS` (CLAUDE.md): nunca añadir `Co-Authored-By: Claude` ni
  "Generated with Claude Code" en commits.
- Identificador de la app: `com.goldmediatech.WordpressPanel` (consistente en
  `tauri.conf.json`, `dbus::SERVICE`, `dbus::PATH`, `system::PLASMOID_ID`).
- Naming de containers: `wp-{site-id}` (proyectos) y `panel-*` (compartidos).
- Modelos serde: `#[serde(rename_all = "camelCase")]`. El espejo TS vive en
  `src/lib/types.ts` (chequear por nombre de campo, no por línea).
- `Result<T, String>` en `#[tauri::command]`. Helper `e()`.
- Docker vía bollard en runtime. **Tres excepciones**: `docker build`
  (`php.rs`), `docker cp` (`docker.rs::migrate_db_to_volume`), `docker exec -i`
  (`migrate::import_dump`).
- `datadir` DB siempre bindeado a `config_dir/db-data/{container}`.

## 10. Estado de deuda / Diferido

- `agent.rs` (Fase 5, IA) — **no existe**. Solo mencionado en
  `docs/EXTENDING.md:89` como receta futura.
- `ports.rs` (Fase 2) — **no existe**. Comentarios en `docker.rs:50, 109, 875`
  (`#[allow(dead_code)]`) referencian lógica de Fase 2 no materializada.
- `shutdown.rs` (Fase 2) — **no existe**. Mismo origen.
- `feature_stub` (Tauri, `lib.rs:674-684`) — placeholder para `cloudflare`,
  `deploy`, `package`. La UI no lo invoca regularmente.
- `frontendFramework` / `headless` — persistidos en `SiteConfig`, sin
  imagen/contenedor específico.
- Comando `Quit` del D-Bus existe pero no tiene UI; cierra directo la app.
- Botones de la barra de título: **no respetan la config del usuario en KDE**
  (`docs/KNOWN_ISSUES.md`). Diferido.
- Verificación visual del plasmoid en Plasma: no automatizada
  (`docs/KNOWN_ISSUES.md`).
- `import_localwp_hermetico` requiere el dump en disco (`local.sql`); la DB
  no se importa sin él (`docs/KNOWN_ISSUES.md`).
- `mcp/server.mjs` no implementa `resources`/`prompts` (solo `tools`).
- `feature_stub` no tiene equivalente D-Bus / CLI / MCP (`featureStub` solo en
  IPC).
- `summary` de la UI permite `feature_stub` solo en errores: no hay UI que
  presente botones con éxito.

## 11. Verificaciones realizadas para este documento

- `pnpm-lock.yaml` y `Cargo.lock` presentes en el árbol (no se inspeccionan
  hashes concretos, sí que están sincronizados el `Cargo.toml`/`Cargo.lock`).
- `mcp/package.json` no declara deps; `mcp/server.mjs` usa solo `node:` APIs.
- `capabilities/default.json` solo expone `core:default` + `core:event:default`.
- `tauri.conf.json` consistente con `dbus::SERVICE` / `dbus::PATH`
  (`com.goldmediatech.WordpressPanel`).

## Fuentes primarias

- `src-tauri/Cargo.toml`.
- `package.json`, `mcp/package.json`, `pnpm-workspace.yaml`.
- `src-tauri/src/lib.rs` (gestión de plugins, `setup`, `generate_handler!`).
- `src-tauri/src/capabilities/default.json`.
- `src-tauri/tauri.conf.json`.
- `src-tauri/src/docker.rs`, `php.rs`, `wordpress.rs`, `migrate.rs`, `dbus.rs`, `system.rs`, `cli.rs`, `github.rs`, `ssl.rs`, `domain.rs`, `netcheck.rs`.
- `scripts/wordpress-panel-cli.sh`, `scripts/wp-wrapper.sh`, `scripts/first-run.sh`, `scripts/package-plasmoid.sh`.
- `mcp/server.mjs`.
- `docs/CHANGELOG.md`, `docs/TESTING.md`, `docs/KNOWN_ISSUES.md`, `docs/EXTENDING.md`, `docs/ARCHITECTURE.md`.
- `CLAUDE.md`.
