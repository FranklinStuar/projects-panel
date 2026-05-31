#!/bin/bash
# Helper CLI de Panel WP. Por ahora solo resuelve a qué proyecto pertenece una ruta.
# Uso: wordpress-panel-cli detect-project <ruta>
set -e

CMD="${1:-}"
ARG="${2:-}"
ROOT="${PANEL_WP_ROOT:-$HOME/panel-wp}"

case "$CMD" in
detect-project)
    [ -n "$ARG" ] || { echo "uso: wordpress-panel-cli detect-project <ruta>" >&2; exit 2; }
    for cfg in "$ROOT"/*/config.json; do
        [ -f "$cfg" ] || continue
        # path del proyecto (campo "path") e id ("id")
        ppath="$(sed -n 's/.*"path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$cfg" | head -1)"
        pid="$(sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$cfg" | head -1)"
        [ -n "$ppath" ] || continue
        case "$ARG/" in
            "$ppath"/*) echo "$pid"; exit 0 ;;
        esac
    done
    exit 1
    ;;
*)
    echo "wordpress-panel-cli: comando desconocido '$CMD'" >&2
    exit 2
    ;;
esac
