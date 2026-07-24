# Instalación y primera ejecución

## Objetivo y alcance

Este runbook deja el entorno de desarrollo listo, ejecuta la configuración privilegiada una sola vez y comprueba que el panel arranca **sin dejar proyectos ni servicios corriendo**. Está orientado a Linux; el flujo probado por el proyecto es Manjaro/Arch con KDE Plasma 6 y NetworkManager.

Fuentes de verdad: `scripts/first-run.sh::pasos 1–5`, `src-tauri/src/system.rs::status`, `src-tauri/src/docker.rs::DockerManager::autoselect_endpoint`, `src-tauri/src/lib.rs::run` y `src-tauri/tauri.conf.json::build`.

> Estado actual importante: las instalaciones nuevas ceden 80/443 y eligen siempre puertos altos, empezando por HTTP 8080 y HTTPS 8443. El valor 80/443 de `config.rs::Endpoint::default` es un fallback antes de que `panel-nginx` materialice y persista el endpoint; no describe la selección normal actual.

## 1. Precondiciones

### Plataforma y herramientas obligatorias

| Componente | Uso | Comprobación |
|---|---|---|
| Rust estable, compatible con Rust 1.77+ | backend Tauri | `rustc --version && cargo --version` |
| Node.js 20+ y pnpm | SvelteKit/Tauri CLI | `node --version && pnpm --version` |
| Docker Engine 24+ | todos los servicios | `docker version && docker info` |
| WebKitGTK y dependencias Tauri 2 | ventana nativa | validar que `pnpm tauri dev` enlaza y abre |
| NetworkManager + dnsmasq | wildcard `*.test` | `systemctl is-active NetworkManager` |
| `mkcert` + NSS | HTTPS local confiable | `mkcert -version` |
| `tar`, `zstd`, `git`, `curl`/TLS del sistema | WordPress, snapshots y repos | `tar --help | grep -- --zstd; command -v zstd git` |

Para las superficies opcionales:

- Plasmoid/CLI: `qdbus6` o `gdbus`.
- CLI ampliada: `jq`, `column`, `python3`, `docker` y `git`.
- GitHub: `gh`, autenticado con `gh auth login`.
- Empaquetado del plasmoid: `zip` y `kpackagetool6`.

En Manjaro/Arch, instala los nombres de paquete equivalentes de tu versión. Como mínimo suelen hacer falta Docker, Rust/rustup, Node.js, pnpm, WebKitGTK 4.1, librsvg, libappindicator, patchelf, dnsmasq, mkcert, NSS, Git, GitHub CLI, jq, Python, zstd y zip. No copies una lista de paquetes de otra distribución sin verificar sus nombres.

### Docker sin `sudo`

```bash
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"
```

Cierra sesión y vuelve a entrar después de cambiar el grupo. Verifica:

```bash
docker info >/dev/null
docker ps
```

**Abortar:** si `docker info` falla, no ejecutes `first-run.sh`; su `set -e` lo detendrá, pero puede haber aplicado pasos anteriores.

**Riesgo:** dar acceso al socket Docker equivale prácticamente a privilegios de administrador. Hazlo solo para una cuenta de desarrollo confiable.

## 2. Instalar dependencias del repositorio

Desde la raíz del repositorio:

```bash
pnpm install
pnpm check
cd src-tauri && cargo check
```

### Cambio esperado

- `node_modules/` queda poblado según `pnpm-lock.yaml`.
- Cargo descarga y compila metadatos en `src-tauri/target/`.
- No se crea ningún proyecto ni container.

### Evidencia

Los dos últimos comandos terminan con código 0. Las entradas que los definen son `package.json::scripts.check` y `src-tauri/Cargo.toml::dependencies`.

### Abortar y recuperar

- `Ctrl+C` es seguro durante una descarga; repite el comando.
- Si la instalación de pnpm quedó inconsistente, vuelve a ejecutar `pnpm install`. No borres el lockfile.
- Si Cargo falla por dependencias del sistema, conserva el primer error de enlazado: normalmente nombra la biblioteca Linux ausente.

## 3. Primera configuración del sistema

Ejecuta desde la raíz:

```bash
bash scripts/first-run.sh
```

### Superficies

| Superficie | Disponible | Uso |
|---|---:|---|
| Script/shell | Sí | flujo completo y recomendado |
| UI | Parcial | Configuración muestra el checklist; puede crear `panel-net` e instalar wrappers sin privilegios |
| CLI `wordpress-panel-cli` | No | todavía se está instalando |
| MCP | No | depende del CLI y del panel abierto |

### Cambios esperados, paso a paso

1. `scripts/first-run.sh::paso 1` crea la red bridge `panel-net` si falta.
2. `scripts/first-run.sh::paso 2`:
   - puede crear `/etc/NetworkManager/conf.d/dns.conf` con `dns=dnsmasq`;
   - escribe `/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf` con `address=/test/127.0.0.1`;
   - reinicia NetworkManager.
3. `scripts/first-run.sh::paso 3` ejecuta `mkcert -install`, que instala una CA local en los almacenes de confianza.
4. `scripts/first-run.sh::paso 4` instala o actualiza el plasmoid `com.goldmediatech.wordpresspanel` si existe `kpackagetool6`.
5. `scripts/first-run.sh::paso 5` instala `wp` y `wordpress-panel-cli` en `~/.local/bin/`.

El script es idempotente, pero no es transaccional: un fallo en un paso no revierte los anteriores.

### Evidencia verificable

```bash
# Red Docker
docker network inspect panel-net --format '{{.Name}} {{.Driver}}'

# DNS wildcard
getent ahostsv4 panel-probe.test

# CA de mkcert
CAROOT="$(mkcert -CAROOT)"
test -f "$CAROOT/rootCA.pem" && printf 'CA OK: %s\n' "$CAROOT/rootCA.pem"

# Wrappers
test -x "$HOME/.local/bin/wp"
test -x "$HOME/.local/bin/wordpress-panel-cli"
command -v wp wordpress-panel-cli

# Plasmoid (si Plasma 6 está instalado)
test -d "$HOME/.local/share/plasma/plasmoids/com.goldmediatech.wordpresspanel"
```

El endpoint web todavía puede no existir: se selecciona y guarda cuando `docker.rs::DockerManager::ensure_nginx` arranca `panel-nginx` por primera vez. Después aparecerá en `~/.config/wordpress-panel/panel.json`.

### Abortar

- Antes de aceptar `sudo`/`pkexec`: cancela sin cambios privilegiados adicionales.
- Durante el reinicio de NetworkManager: espera a que termine; no mates NetworkManager a mitad.
- En `mkcert -install`: cancelar puede dejar la CA sin instalar en todos los almacenes; repite el comando.

### Recuperar una ejecución parcial

1. Corrige la herramienta ausente indicada en la última línea.
2. Repite `bash scripts/first-run.sh`; cada bloque detecta el estado ya aplicado.
3. Comprueba el checklist en **Configuración**; lo calcula `src-tauri/src/system.rs::status` en modo best-effort.
4. Si el DNS no vuelve tras el reinicio:

   ```bash
   systemctl status NetworkManager --no-pager
   journalctl -u NetworkManager -b --no-pager
   getent ahostsv4 panel-probe.test
   ```

5. Si los wrappers no están en `PATH`, añade a la configuración del shell:

   ```bash
   export PATH="$HOME/.local/bin:$PATH"
   ```

### Riesgos destructivos o globales

- Reinicia NetworkManager: corta temporalmente la red del equipo.
- Sobrescribe el snippet DNS específico del panel.
- Instala una CA local confiable; protege su clave bajo el `CAROOT` de mkcert.
- Instala/actualiza archivos de usuario en `~/.local/bin` y el plasmoid existente.
- No borres `panel-net` si hay proyectos activos.

## 4. Primera ejecución real

```bash
pnpm tauri dev
```

En Wayland, si la ventana queda blanca:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

`src-tauri/tauri.conf.json::build.beforeDevCommand` levanta Vite en `http://localhost:1420`; Tauri abre la ventana nativa. `src-tauri/src/lib.rs::run` también intenta instalar los wrappers, arranca el servidor D-Bus y recupera los watchers de auto-dump de containers de proyecto que ya estuvieran activos.

### Cambio esperado

- Se abre **Panel WP**.
- El backend publica el servicio D-Bus de sesión `com.goldmediatech.WordpressPanel`.
- Con cero proyectos activos, no debe arrancar ningún container del panel por el mero hecho de abrir la ventana.

### Evidencia

```bash
# Con el panel abierto
gdbus introspect --session \
  --dest com.goldmediatech.WordpressPanel \
  --object-path /com/goldmediatech/WordpressPanel >/dev/null \
  && printf 'D-Bus OK\n'

docker ps --format '{{.Names}}' | grep -E '^(wp-|panel-)' || printf '0 containers activos: OK\n'
```

En la UI, abre **Configuración** y comprueba Docker, red, dnsmasq, mkcert, wrappers y plasmoid. La pantalla usa `src-tauri/src/system.rs::status`.

### Abortar y recuperar

- En desarrollo, `Ctrl+C` detiene Vite/Tauri. Los containers ya activos no se detienen automáticamente por ese `Ctrl+C`; páralos antes desde la UI o CLI.
- Si el puerto 1420 está ocupado, identifica el proceso (`ss -ltnp | grep ':1420'`) y detén el Vite anterior.
- Si D-Bus no está disponible, la UI sigue funcionando; CLI, MCP y plasmoid no.
- Si aparece un error de puerto web, consulta `docs/resume/operacion/07-diagnostico-y-mantenimiento.md` antes de borrar `panel.json`.

## 5. Criterio de salida

La instalación queda validada cuando:

- `pnpm check` y `cargo check` son verdes;
- `panel-net` existe;
- `panel-probe.test` resuelve a `127.0.0.1`;
- la CA de mkcert y los wrappers existen;
- la ventana real abre, o abre con `WEBKIT_DISABLE_DMABUF_RENDERER=1`;
- con cero proyectos activos, `docker ps` no muestra `wp-*` ni `panel-*`.

Crear el primer sitio es un procedimiento distinto: sigue `docs/resume/operacion/03-runbook-proyectos.md`.
