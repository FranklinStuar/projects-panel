# CLI, D-Bus, MCP y plasmoid

## Cadena de integración

```text
Agente MCP
   │ JSON-RPC stdio
   ▼
mcp/server.mjs
   │ spawn wordpress-panel-cli (cwd del proyecto)
   ▼
scripts/wordpress-panel-cli.sh
   │ gdbus o qdbus6, sesión de usuario
   ▼
com.goldmediatech.WordpressPanel.Manager
   │
src-tauri/src/dbus.rs::Manager
   │
casos de uso Rust → Docker/filesystem/git
```

El MCP es un adaptador fino sin SDK externo: implementa `initialize`, `ping`, `tools/list` y `tools/call`, una línea JSON-RPC por línea. stdout queda reservado al protocolo y stderr a logs. Resuelve proyectos leyendo `config.json`, construye argumentos y ejecuta el CLI. **No es IA interna:** ofrece herramientas a clientes MCP; el panel no contiene modelo, agente ni proveedor LLM.

## CLI

`cli::install_cli_wrapper` copia `wordpress-panel-cli.sh` y `wp-wrapper.sh` a `~/.local/bin` con modo 0755. Se intenta al arrancar Tauri y también mediante acción manual.

`wordpress-panel-cli`:

- detecta proyecto por el CWD bajo la ruta configurada;
- resuelve id exacto o subcadena de nombre consultando `ListSites`;
- usa `gdbus` o `qdbus6` y requiere el panel abierto;
- delega snapshots, clones, worktrees, Git, start/stop y aperturas al backend;
- para `resources` y `logs` usa directamente Docker CLI después de resolver containers.

El wrapper `wp` resuelve el proyecto y ejecuta:

```text
docker exec -i --user www-data wp-{id}
  php /usr/local/bin/wp --path=/var/www/html ...
```

Así WP-CLI no corre como root y los archivos respetan ownership del host.

## D-Bus

`dbus::serve` registra en el bus de sesión:

- servicio `com.goldmediatech.WordpressPanel`;
- objeto `/com/goldmediatech/WordpressPanel`;
- interfaz `com.goldmediatech.WordpressPanel.Manager`.

Las colecciones complejas se serializan como strings JSON. Las mutaciones que alteran proyectos emiten `sites-changed` a la GUI. La conexión vive en una tarea Tauri pendiente; si D-Bus no está disponible, solo fallan estas integraciones.

## Plasmoid KDE

El plasmoid QML usa `P5Support.DataSource` con engine `executable` para lanzar `qdbus6`. Cada 3 segundos consulta `GetRunningSites`, muestra contador y lista de activos, permite detener cada sitio y ofrece “Apagar todo y cerrar”. No incluye “encender todos”.

```text
Timer 3 s → qdbus6 GetRunningSites → JSON → ListView
fila Stop → qdbus6 StopSite(id)
botón final → StopAll → Quit
```

`scripts/first-run.sh` instala/actualiza la carpeta mediante `kpackagetool6`; `scripts/package-plasmoid.sh` crea `dist/wordpress-panel.plasmoid` como zip con `metadata.json` y `contents/` en raíz.

## Dependencias y fallos

La cadena MCP necesita Node, el script CLI, `jq`, utilidades de formato y un cliente D-Bus; muchas acciones necesitan Docker CLI. Además, el proceso Tauri debe estar abierto y poseer el nombre D-Bus. El MCP puede elegir wrapper instalado o script del repo mediante variables de entorno.

## Límites y deuda

- No hay autenticación adicional en D-Bus de sesión.
- MCP y CLI leen configuración para resolución, pero las mutaciones pasan por el backend.
- El plasmoid construye comandos shell con argumentos entre comillas simples sin una función robusta de escape; los ids usados son controlados, pero el patrón merece cautela.
- El polling de 3 s y el motor executable son sencillos, no reactivos.
- La verificación visual del plasmoid en Plasma sigue pendiente.
- El catálogo de tools/métodos debe mantenerse en `../referencia/*`.

## Fuentes primarias

- `mcp/server.mjs::TOOLS`, `runCli`, `toolsCall`
- `scripts/wordpress-panel-cli.sh::dbus_call`, `resolve_pid`
- `scripts/wp-wrapper.sh`
- `src-tauri/src/cli.rs::install_cli_wrapper`, `open_terminal_at`
- `src-tauri/src/dbus.rs::Manager`, `serve`
- `plasma/applets/wordpress-panel-plasmoid/contents/ui/main.qml::callDbus`, `refresh`
- `scripts/package-plasmoid.sh`
