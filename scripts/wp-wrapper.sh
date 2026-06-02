#!/bin/bash
# Wrapper WP-CLI para terminal. El panel lo instala en ~/.local/bin/wp.
# Detecta el proyecto por el directorio actual y ejecuta wp dentro de su container.
set -e

PROJECT_ID="$(wordpress-panel-cli detect-project "$PWD" 2>/dev/null || true)"

if [ -z "$PROJECT_ID" ]; then
    echo "wp: no se detectó ningún proyecto Panel WP en $PWD" >&2
    exit 1
fi

# Como www-data: WP-CLI prohíbe correr WordPress como root y rompería la
# propiedad de los archivos del sitio (paridad con el comando in-app exec_wpcli).
exec docker exec -i --user www-data "wp-${PROJECT_ID}" php /usr/local/bin/wp --path=/var/www/html "$@"
