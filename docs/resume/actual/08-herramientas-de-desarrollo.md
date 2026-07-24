# Herramientas de desarrollo

## Toolchain

El frontend usa pnpm, Vite 6, SvelteKit 2/Svelte 5, TypeScript, Tailwind y adapter-static. Tauri CLI enlaza el build con Rust. El backend se compila con Cargo y depende de Tokio, bollard y APIs Tauri.

```text
pnpm dev             Vite :1420
pnpm tauri dev       Vite + backend + WebView
pnpm build           SPA → build/
pnpm check           sync + svelte-check
cargo build/test     backend Rust
```

En Wayland puede requerirse `WEBKIT_DISABLE_DMABUF_RENDERER=1`. No se usa `GTK_CSD=0`; ese ajuste fue revertido y no debe documentarse como requisito.

## Pruebas

Hay tres niveles:

1. Tests unitarios Rust junto a módulos para lógica pura: serialización, URLs, parsing, resúmenes Git, etc.
2. Tests de integración Rust ignorados que pueden usar Docker y mutar entorno; deben ejecutarse en serie.
3. Playwright e2e sobre `pnpm dev:mock`, con IPC Tauri simulado.

`routes/+layout.ts::load` carga `$lib/dev/mock-ipc` antes del render cuando `VITE_MOCK_IPC=1`. Esto valida GUI, no bollard/D-Bus/procesos reales. Los escenarios e2e actuales cubren dashboard, creación, borrado, cancelación/importación, migración, configuración y accesibilidad.

## Primera configuración

`scripts/first-run.sh` es Linux/KDE/NetworkManager y realiza pasos idempotentes:

```text
Docker panel-net
   ↓
dnsmasq wildcard *.test → 127.0.0.1 (sudo + reinicio NM)
   ↓
mkcert -install, si existe
   ↓
plasmoid con kpackagetool6
   ↓
wrappers CLI en ~/.local/bin
```

El runtime también puede crear `panel-net` y dejar un snippet DNS; la instalación privilegiada usa `pkexec` en `domain::install_wildcard` cuando fuese necesaria. Los sitios se publican en puertos altos desde 8080/8443 aunque el wildcard apunte a loopback.

## Terminal y WP-CLI

`lib::open_terminal` instala wrappers y `cli::open_terminal_at` prueba Konsole, GNOME Terminal, XFCE Terminal, Kitty, Alacritty y `x-terminal-emulator`, fijando CWD. `php::wp_cli_phar_path` descarga una única copia en config global y se monta read-only en PHP. Tanto el comando in-app como el wrapper ejecutan como `www-data`.

## VSCode y Git

`github::ensure_workspace` crea una vez un `.code-workspace`:

- proyecto normal: `app/public` más repos detectados como raíces adicionales;
- worktree-project: solo `wt/{basename}` con la rama independiente.

Si el archivo existe no se sobrescribe. `github::open_vscode` prueba `code`, `codium`, `code-insiders` y `vscodium`. Git/GitHub operan en el host con credenciales/SSH existentes. El deploy directo hace checkout, `git pull --ff-only` y un build opcional en shell de login en carpetas seleccionadas.

## Docker para desarrollo/runtime

La imagen PHP se construye localmente con `docker build` y revisión en el tag. Los assets bajo `docker/` forman parte del runtime de desarrollo: Dockerfile, entrypoint, plantilla PHP, vhost, mu-plugins y plugin Adminer. Cambiar el Dockerfile requiere subir `php::IMAGE_REV` para forzar recreación.

## Empaquetado

Tauri tiene bundle activo para todos los targets declarados, aunque las integraciones reales son Linux-específicas. El plasmoid se empaqueta por separado con `zip`. La documentación de comandos/casos detallados debe enlazarse en `../referencia/*`.

## Deuda observable

No hay script único de verificación que ejecute check, Rust y e2e. El mock IPC puede divergir del contrato real. Builds Git configurables ejecutan texto de shell del usuario con sus permisos. Varias herramientas dependen de PATH y escritorios concretos; los mensajes de error son la detección principal.

## Fuentes primarias

- `package.json::scripts`, `devDependencies`
- `src/routes/+layout.ts::load`
- `playwright.config.ts`
- `src-tauri/src/integration_tests.rs`
- `scripts/first-run.sh`
- `src-tauri/src/cli.rs::open_terminal_at`
- `src-tauri/src/php.rs::wp_cli_phar_path`, `IMAGE_REV`
- `src-tauri/src/github.rs::ensure_workspace`, `open_vscode`, `deploy`
- `src-tauri/tauri.conf.json::bundle`
