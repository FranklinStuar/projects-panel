# MCP — Panel WP

Servidor [MCP](https://modelcontextprotocol.io) que expone el **Panel WP** a
agentes IA (Claude Code, opencode, …). Es un **envoltorio fino** sobre
`wordpress-panel-cli`: cada herramienta lanza el CLI, que a su vez habla con el
**panel EN EJECUCIÓN** por D-Bus. No reimplementa lógica ni duplica estado.

- **Sin dependencias.** Un solo archivo Node (`server.mjs`), protocolo MCP por
  stdio implementado a mano. No hay `npm install` ni build.
- **Requisitos:** `node` ≥ 18, el **panel abierto** (salvo `project_logs php`),
  y el CLI resoluble (busca `~/.local/bin/wordpress-panel-cli`, luego
  `scripts/wordpress-panel-cli.sh` del repo; overridable con `WORDPRESS_PANEL_CLI`).

## Herramientas

| Herramienta | Qué hace |
|---|---|
| `list_projects` | Lista todos los proyectos con estado (activo/parado), dominio, grupo. |
| `start_project` / `stop_project` | Enciende / apaga un proyecto. |
| `project_containers` | Containers del proyecto (php/db/nginx/mailpit/minio) y estado. |
| `project_resources` | `docker stats` (CPU/mem) de esos containers. |
| `project_logs` | Logs de un container (`service`: php\|db\|nginx\|mailpit\|minio, `lines`). |
| `open_project` | Abre en escritorio `admin` (auto-login), `site` o `folder`. |
| `list_snapshots` / `create_snapshot` / `delete_snapshot` / `clone_snapshot` | Puntos de guardado. |
| `git_scan` / `git_status` / `git_pull` / `git_set_deploy` / `git_deploy` | Git y deploy directo. |
| `worktree_list` | Worktree-projects del proyecto. |

Todas reciben `project` = **id o nombre** (subcadena, case-insensitive). Las de
git reciben además `path` = ruta del repo relativa a `app/public/`.

## Configuración

Ya quedó configurado en esta máquina (ver abajo). Para replicarlo:

### Claude Code

Ámbito **usuario** (disponible desde cualquier carpeta):

```bash
claude mcp add wordpress-panel --scope user -- node /home/franklin/MEGA/dev/wordpress-panel/mcp/server.mjs
```

Verifica: `claude mcp list` → `wordpress-panel ... ✔ Connected`.

> Alternativa por proyecto: crea un `.mcp.json` en la raíz del repo con
> `{"mcpServers":{"wordpress-panel":{"command":"node","args":["mcp/server.mjs"]}}}`.
> No lo combines con el ámbito usuario o Claude Code avisará de scopes duplicados.

### opencode

En `~/.config/opencode/opencode.json` (o el `opencode.json` de un proyecto),
bajo la clave `mcp`:

```json
{
  "mcp": {
    "wordpress-panel": {
      "type": "local",
      "command": ["node", "/home/franklin/MEGA/dev/wordpress-panel/mcp/server.mjs"],
      "enabled": true
    }
  }
}
```

## Prueba manual

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}' \
  | node mcp/server.mjs
```

## Variables de entorno

- `WORDPRESS_PANEL_CLI` — ruta al CLI (default: wrapper instalado o script del repo).
- `PANEL_WP_ROOT` — carpeta de proyectos (default `~/panel-wp`). Debe coincidir con la del CLI.
