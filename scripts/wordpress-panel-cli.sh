#!/bin/bash
# Helper CLI de Panel WP.
#   wordpress-panel-cli detect-project <ruta>
#   wordpress-panel-cli worktree list
#   wordpress-panel-cli worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]
#   wordpress-panel-cli worktree remove <id-worktree> [--delete-branch]
#
# Las operaciones de worktree hablan con el panel EN EJECUCIÓN por D-Bus (reusan
# su lógica: container, nginx, BD). Si el panel no está abierto, fallan pidiendo
# abrirlo.
set -e

CMD="${1:-}"
ROOT="${PANEL_WP_ROOT:-$HOME/panel-wp}"

DBUS_DEST="com.goldmediatech.WordpressPanel"
DBUS_PATH="/com/goldmediatech/WordpressPanel"
DBUS_IFACE="com.goldmediatech.WordpressPanel.Manager"

# Resuelve "id|path" del proyecto que contiene la ruta dada (o vacío).
project_for() {
    local arg="$1"
    local cfg ppath pid
    for cfg in "$ROOT"/*/config.json; do
        [ -f "$cfg" ] || continue
        ppath="$(sed -n 's/.*"path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$cfg" | head -1)"
        pid="$(sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$cfg" | head -1)"
        [ -n "$ppath" ] || continue
        case "$arg/" in
            "$ppath"/*) echo "$pid|$ppath"; return 0 ;;
        esac
    done
    return 1
}

# Llama un método del panel por D-Bus. Imprime el resultado crudo en stdout.
dbus_call() {
    local method="$1"; shift
    if command -v gdbus >/dev/null 2>&1; then
        gdbus call --session --dest "$DBUS_DEST" --object-path "$DBUS_PATH" \
            --method "$DBUS_IFACE.$method" "$@"
    elif command -v qdbus6 >/dev/null 2>&1; then
        qdbus6 "$DBUS_DEST" "$DBUS_PATH" "$DBUS_IFACE.$method" "$@"
    else
        echo "error: ni gdbus ni qdbus6 disponibles para hablar con el panel" >&2
        exit 3
    fi
}

require_panel() {
    if command -v gdbus >/dev/null 2>&1; then
        gdbus call --session --dest "$DBUS_DEST" --object-path "$DBUS_PATH" \
            --method "$DBUS_IFACE.GetRunningSites" >/dev/null 2>&1 && return 0
    elif command -v qdbus6 >/dev/null 2>&1; then
        qdbus6 "$DBUS_DEST" "$DBUS_PATH" "$DBUS_IFACE.GetRunningSites" >/dev/null 2>&1 && return 0
    fi
    echo "error: el panel WordPress no está en ejecución (ábrelo para usar worktrees)" >&2
    exit 4
}

case "$CMD" in
detect-project)
    ARG="${2:-}"
    [ -n "$ARG" ] || { echo "uso: wordpress-panel-cli detect-project <ruta>" >&2; exit 2; }
    info="$(project_for "$ARG")" || exit 1
    echo "${info%%|*}"
    ;;

worktree)
    SUB="${2:-}"
    require_panel
    case "$SUB" in
    list)
        info="$(project_for "$PWD")" || { echo "no se detectó proyecto en $PWD" >&2; exit 1; }
        dbus_call ListWorktrees "${info%%|*}"
        ;;
    create)
        BRANCH="${3:-}"
        [ -n "$BRANCH" ] || { echo "uso: worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]" >&2; exit 2; }
        shift 3 || true
        TARGET=""; BASE=""; SHARED="true"
        while [ $# -gt 0 ]; do
            case "$1" in
                --target) TARGET="$2"; shift 2 ;;
                --base) BASE="$2"; shift 2 ;;
                --copy-db) SHARED="false"; shift ;;
                *) echo "opción desconocida: $1" >&2; exit 2 ;;
            esac
        done
        info="$(project_for "$PWD")" || { echo "no se detectó proyecto en $PWD" >&2; exit 1; }
        pid="${info%%|*}"; ppath="${info#*|}"
        # Si no se dio --target, inferirlo del repo git que contiene el CWD.
        if [ -z "$TARGET" ]; then
            top="$(git rev-parse --show-toplevel 2>/dev/null || true)"
            [ -n "$top" ] || { echo "indica --target <ruta relativa a public/> (no estás dentro de un repo git)" >&2; exit 2; }
            TARGET="${top#"$ppath"/app/public/}"
            [ "$TARGET" != "$top" ] || { echo "el repo $top no está bajo este proyecto" >&2; exit 2; }
        fi
        echo "Creando worktree «$BRANCH» de $TARGET (BD $([ "$SHARED" = true ] && echo compartida || echo copia))…"
        dbus_call CreateWorktree "$pid" "$TARGET" "$BRANCH" "$BASE" "$SHARED"
        ;;
    remove)
        WTID="${3:-}"
        [ -n "$WTID" ] || { echo "uso: worktree remove <id-worktree> [--delete-branch]" >&2; exit 2; }
        DELBR="false"
        [ "${4:-}" = "--delete-branch" ] && DELBR="true"
        dbus_call RemoveWorktree "$WTID" "$DELBR"
        ;;
    *)
        echo "uso: wordpress-panel-cli worktree {list|create|remove} …" >&2
        exit 2
        ;;
    esac
    ;;

*)
    echo "wordpress-panel-cli: comando desconocido '$CMD'" >&2
    exit 2
    ;;
esac
