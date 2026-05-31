#!/bin/bash
# Wrapper WP-CLI para terminal. El panel lo instala en ~/.local/bin/wp.
# Detecta el proyecto por el directorio actual y ejecuta wp dentro de su container.
set -e

PROJECT_ID="$(wordpress-panel-cli detect-project "$PWD" 2>/dev/null || true)"

if [ -z "$PROJECT_ID" ]; then
    echo "wp: no se detectó ningún proyecto Panel WP en $PWD" >&2
    exit 1
fi

exec docker exec -i "wp-${PROJECT_ID}" php /usr/local/bin/wp --path=/var/www/html "$@"
