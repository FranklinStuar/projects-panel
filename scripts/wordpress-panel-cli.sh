#!/bin/bash
# Helper CLI de Panel WP.
#   wordpress-panel-cli detect-project <ruta>
#   wordpress-panel-cli snapshot   {list|create|delete|clone} …
#   wordpress-panel-cli git        {scan|status|pull|set-deploy|deploy} …
#   wordpress-panel-cli worktree   {list|create|remove} …
#
# Todas las operaciones hablan con el panel EN EJECUCIÓN por D-Bus (reusan su
# lógica: container, nginx, BD, git). Si el panel no está abierto, fallan
# pidiendo abrirlo.
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

# Llama un método que devuelve un String JSON y lo desenvuelve para jq.
# gdbus envuelve el resultado en una tupla ('<json escapado>',); qdbus6 no.
dbus_json() {
    local raw pretty
    if command -v qdbus6 >/dev/null 2>&1; then
        # qdbus6 imprime el valor crudo, sin envoltura ni escapes.
        raw="$(qdbus6 "$DBUS_DEST" "$DBUS_PATH" "$DBUS_IFACE.$1" "${@:2}")"
    elif command -v gdbus >/dev/null 2>&1; then
        raw="$(gdbus call --session --dest "$DBUS_DEST" --object-path "$DBUS_PATH" \
            --method "$DBUS_IFACE.$1" "${@:2}")"
        # Desenvuelve ('...',) y desescapa a JSON limpio.
        raw="$(printf '%s' "$raw" | python3 -c 'import sys,ast; print(ast.literal_eval(sys.stdin.read())[0])')"
    else
        echo "error: ni gdbus ni qdbus6 disponibles para hablar con el panel" >&2
        exit 3
    fi
    if pretty="$(printf '%s' "$raw" | jq . 2>/dev/null)"; then
        printf '%s\n' "$pretty"
    else
        printf '%s\n' "$raw"
    fi
}

require_panel() {
    if command -v gdbus >/dev/null 2>&1; then
        gdbus call --session --dest "$DBUS_DEST" --object-path "$DBUS_PATH" \
            --method "$DBUS_IFACE.GetRunningSites" >/dev/null 2>&1 && return 0
    elif command -v qdbus6 >/dev/null 2>&1; then
        qdbus6 "$DBUS_DEST" "$DBUS_PATH" "$DBUS_IFACE.GetRunningSites" >/dev/null 2>&1 && return 0
    fi
    echo "error: el panel WordPress no está en ejecución (ábrelo para usar el panel)" >&2
    exit 4
}

# Resuelve el proyecto del CWD o falla con mensaje claro. Ecoa "pid|ppath".
project_or_die() {
    project_for "$PWD" || { echo "no se detectó proyecto en $PWD" >&2; exit 1; }
}

# Infiere la ruta del repo git (relativa a app/public/) del CWD, dado ppath.
# Ecoa la ruta relativa o falla (exit 2).
git_target_path() {
    local ppath="$1" top
    top="$(git rev-parse --show-toplevel 2>/dev/null || true)"
    [ -n "$top" ] || { echo "indica --path <ruta relativa a public/> (no estás dentro de un repo git)" >&2; exit 2; }
    local rel="${top#"$ppath"/app/public/}"
    [ "$rel" != "$top" ] || { echo "el repo $top no está bajo este proyecto" >&2; exit 2; }
    echo "$rel"
}

usage() {
    cat <<'EOF'
wordpress-panel-cli — CLI del Panel WP (habla con el panel EN EJECUCIÓN por D-Bus)

USO:
  wordpress-panel-cli <grupo> <subcomando> [opciones]

detect-project <ruta>
    Imprime el id del proyecto que contiene la ruta.

snapshot   (autodetecta el proyecto del directorio actual)
  snapshot list                     Lista los puntos de guardado.
  snapshot create <label>           Crea un punto de guardado.
  snapshot delete <snapshotId>      Borra un punto de guardado.
  snapshot clone <snapshotId>       Crea un clon temporal desde el snapshot.

git   (repo objetivo inferido del CWD; override con --path <ruta rel a public/>)
  git scan                                    Lista repos del proyecto.
  git status  [--path <p>] [--branch <b>]     Estado de la rama (ahead/behind/dirty).
  git pull    [--path <p>] [--branch <b>]     git pull de la rama.
  git set-deploy [--path <p>] --branch <b> [--build "<cmd>"] [--dirs a,b,c]
                                              Configura el deploy (build + carpetas).
  git deploy  [--path <p>]                    Ejecuta el deploy guardado.

worktree   (autodetecta el proyecto del directorio actual)
  worktree list
  worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]
  worktree remove <id-worktree> [--delete-branch]

start                               Enciende el proyecto (containers).
stop                                Apaga el proyecto.

open <qué>   qué ∈ admin|site|front|folder
  open admin                        Abre el wp-admin (auto-login) en el navegador.
  open site | open front            Abre el frontend en el navegador.
  open folder                       Abre la carpeta del proyecto en el explorador.

containers                          Lista los containers del proyecto (name/role/running).
resources                           docker stats de los containers del proyecto.

logs [servicio] [-f] [-n N]         Ver logs de un container (default php, 200 líneas).
                                    servicio ∈ php|db|nginx|mailpit|minio o nombre crudo.

EJEMPLOS:
  cd ~/panel-wp/mi-sitio/app/public/wp-content/themes/mi-theme
  wordpress-panel-cli snapshot create "antes del refactor"
  wordpress-panel-cli snapshot list
  wordpress-panel-cli git status --branch develop
  wordpress-panel-cli git set-deploy --branch main --build "npm ci && npm run build" --dirs dist
  wordpress-panel-cli git deploy
  wordpress-panel-cli worktree create feature/nav --copy-db
  wordpress-panel-cli start
  wordpress-panel-cli open admin
  wordpress-panel-cli open folder
  wordpress-panel-cli containers
  wordpress-panel-cli resources
  wordpress-panel-cli logs nginx -f
  wordpress-panel-cli logs php -n 500
EOF
}

case "$CMD" in
""|-h|--help|help)
    usage
    exit 0
    ;;

detect-project)
    ARG="${2:-}"
    [ -n "$ARG" ] || { echo "uso: wordpress-panel-cli detect-project <ruta>" >&2; exit 2; }
    info="$(project_for "$ARG")" || exit 1
    echo "${info%%|*}"
    ;;

snapshot)
    SUB="${2:-}"
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"
    case "$SUB" in
    list)
        dbus_json ListSnapshots "$pid" | jq -r '
            (["ID","LABEL","FECHA","TAMAÑO"] | @tsv),
            (.[] | [ .id, .label, .createdAt, ((.sizeBytes/1048576*10|round/10|tostring)+" MB") ] | @tsv)
        ' | column -t -s $'\t'
        ;;
    create)
        LABEL="${3:-}"
        [ -n "$LABEL" ] || { echo "uso: snapshot create <label>" >&2; exit 2; }
        dbus_json CreateSnapshot "$pid" "$LABEL"
        ;;
    delete)
        SNAPID="${3:-}"
        [ -n "$SNAPID" ] || { echo "uso: snapshot delete <snapshotId>" >&2; exit 2; }
        if [ "$(dbus_call DeleteSnapshot "$pid" "$SNAPID" | tr -d ' ()' )" = "true" ]; then
            echo "ok: snapshot $SNAPID borrado"
        else
            echo "fallo: no se pudo borrar el snapshot $SNAPID" >&2; exit 1
        fi
        ;;
    clone)
        SNAPID="${3:-}"
        [ -n "$SNAPID" ] || { echo "uso: snapshot clone <snapshotId>" >&2; exit 2; }
        res="$(dbus_json CreateClone "$pid" "$SNAPID")"
        if [ "$(printf '%s' "$res" | jq -r '.ok')" = "true" ]; then
            echo "clon creado: $(printf '%s' "$res" | jq -r '.domain')"
        else
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        ;;
    *)
        echo "uso: wordpress-panel-cli snapshot {list|create <label>|delete <snapshotId>|clone <snapshotId>}" >&2
        exit 2
        ;;
    esac
    ;;

git)
    SUB="${2:-}"
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"; ppath="${info#*|}"
    shift || true   # descarta "git"
    shift || true   # descarta el subcomando
    # Opciones comunes.
    GPATH=""; GBRANCH=""; GBUILD=""; GDIRS=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --path) GPATH="$2"; shift 2 ;;
            --branch) GBRANCH="$2"; shift 2 ;;
            --build) GBUILD="$2"; shift 2 ;;
            --dirs) GDIRS="$2"; shift 2 ;;
            *) echo "opción desconocida: $1" >&2; exit 2 ;;
        esac
    done
    case "$SUB" in
    scan)
        dbus_json GhScan "$pid" | jq -r '
            (["PATH","BRANCH","REMOTE","REGISTRADO"] | @tsv),
            (.[] | [ (.path // "(raíz)"), .branch, .remote, (if .registered then "sí" else "no" end) ] | @tsv)
        ' | column -t -s $'\t'
        ;;
    status)
        [ -n "$GPATH" ] || GPATH="$(git_target_path "$ppath")"
        res="$(dbus_json GhBranchStatus "$pid" "$GPATH" "$GBRANCH")"
        if [ "$(printf '%s' "$res" | jq -r '.ok // true')" = "false" ]; then
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        printf '%s' "$res" | jq -r '
            .message,
            "  actual: \(.current)  objetivo: \(.target)",
            "  ahead: \(.ahead)  behind: \(.behind)  dirty: \(.dirty)  canPull: \(.canPull)"
        '
        ;;
    pull)
        [ -n "$GPATH" ] || GPATH="$(git_target_path "$ppath")"
        res="$(dbus_json GhPull "$pid" "$GPATH" "$GBRANCH")"
        if [ "$(printf '%s' "$res" | jq -r '.ok')" = "true" ]; then
            printf '%s\n' "$(printf '%s' "$res" | jq -r '.output')"
        else
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        ;;
    set-deploy)
        [ -n "$GPATH" ] || GPATH="$(git_target_path "$ppath")"
        [ -n "$GBRANCH" ] || { echo "uso: git set-deploy [--path <p>] --branch <b> [--build \"<cmd>\"] [--dirs a,b,c]" >&2; exit 2; }
        if [ "$(dbus_call GhSetDeploy "$pid" "$GPATH" "$GBRANCH" "$GBUILD" "$GDIRS" | tr -d ' ()' )" = "true" ]; then
            echo "ok: deploy configurado para $GPATH ($GBRANCH)"
        else
            echo "fallo: no se pudo configurar el deploy" >&2; exit 1
        fi
        ;;
    deploy)
        [ -n "$GPATH" ] || GPATH="$(git_target_path "$ppath")"
        res="$(dbus_json GhDeploy "$pid" "$GPATH")"
        if [ "$(printf '%s' "$res" | jq -r '.ok')" = "true" ]; then
            echo "ok: deploy ejecutado para $GPATH"
        else
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        ;;
    *)
        echo "uso: wordpress-panel-cli git {scan|status|pull|set-deploy|deploy} [--path <p>] [--branch <b>] …" >&2
        exit 2
        ;;
    esac
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

start)
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"
    if [ "$(dbus_call StartSite "$pid" | tr -d ' ()')" = "true" ]; then
        echo "✓ Encendido"
    else
        echo "✗ falló" >&2; exit 1
    fi
    ;;

stop)
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"
    if [ "$(dbus_call StopSite "$pid" | tr -d ' ()')" = "true" ]; then
        echo "✓ Apagado"
    else
        echo "✗ falló" >&2; exit 1
    fi
    ;;

open)
    WHAT="${2:-}"
    info="$(project_or_die)"; pid="${info%%|*}"; ppath="${info#*|}"
    case "$WHAT" in
    admin)
        require_panel
        res="$(dbus_json OpenAdmin "$pid")"
        if [ "$(printf '%s' "$res" | jq -r '.ok')" = "true" ]; then
            echo "ok: wp-admin abierto"
        else
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        ;;
    site|front)
        require_panel
        res="$(dbus_json OpenSite "$pid")"
        if [ "$(printf '%s' "$res" | jq -r '.ok')" = "true" ]; then
            echo "ok: abierto $(printf '%s' "$res" | jq -r '.url // ""')"
        else
            echo "fallo: $(printf '%s' "$res" | jq -r '.error // "error desconocido"')" >&2; exit 1
        fi
        ;;
    folder)
        xdg-open "$ppath" >/dev/null 2>&1 &
        echo "ok: abierto $ppath"
        ;;
    *)
        echo "uso: wordpress-panel-cli open {admin|site|front|folder}" >&2
        exit 2
        ;;
    esac
    ;;

containers)
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"
    dbus_json ProjectContainers "$pid" | jq -r '
        (["NAME","ROLE","RUNNING"] | @tsv),
        (.[] | [ .name, .role, (if .running then "sí" else "no" end) ] | @tsv)
    ' | column -t -s $'\t'
    ;;

resources)
    require_panel
    info="$(project_or_die)"; pid="${info%%|*}"
    names="$(dbus_json ProjectContainers "$pid" | jq -r '.[].name')"
    existing=()
    for n in $names; do
        docker inspect "$n" >/dev/null 2>&1 && existing+=("$n")
    done
    if [ "${#existing[@]}" -eq 0 ]; then
        echo "no hay containers del proyecto corriendo" >&2; exit 1
    fi
    docker stats --no-stream "${existing[@]}"
    ;;

logs)
    info="$(project_or_die)"; pid="${info%%|*}"
    shift || true   # descarta "logs"
    SERVICE=""; FOLLOW=""; TAIL="200"
    while [ $# -gt 0 ]; do
        case "$1" in
            -f|--follow) FOLLOW="-f"; shift ;;
            -n|--tail) TAIL="$2"; shift 2 ;;
            *)
                if [ -z "$SERVICE" ]; then SERVICE="$1"; shift
                else echo "opción desconocida: $1" >&2; exit 2; fi
                ;;
        esac
    done
    [ -n "$SERVICE" ] || SERVICE="php"
    case "$SERVICE" in
    php)
        CONTAINER="wp-$pid"
        ;;
    db|nginx|mailpit|minio)
        require_panel
        CONTAINER="$(dbus_json ProjectContainers "$pid" | jq -r --arg r "$SERVICE" '.[] | select(.role==$r) | .name' | head -1)"
        [ -n "$CONTAINER" ] || { echo "error: no hay container con rol '$SERVICE' en este proyecto" >&2; exit 1; }
        ;;
    *)
        CONTAINER="$SERVICE"   # nombre de container literal
        ;;
    esac
    docker inspect "$CONTAINER" >/dev/null 2>&1 || { echo "error: el container '$CONTAINER' no existe" >&2; exit 1; }
    docker logs --tail "$TAIL" $FOLLOW "$CONTAINER" || true
    ;;

*)
    echo "wordpress-panel-cli: comando desconocido '$CMD' (usa -h para ayuda)" >&2
    exit 2
    ;;
esac
