#!/bin/sh
# Mapea www-data al uid/gid del host (PUID/PGID) para que WordPress pueda
# escribir uploads/plugins en los bind-mounts y el usuario pueda editar los
# archivos clonados con gh sin conflictos de permisos.
set -e

PUID="${PUID:-82}"   # 82 = www-data por defecto en alpine
PGID="${PGID:-82}"

CUR_UID="$(id -u www-data)"
CUR_GID="$(id -g www-data)"

if [ "$PGID" != "$CUR_GID" ]; then
    groupmod -o -g "$PGID" www-data
fi
if [ "$PUID" != "$CUR_UID" ]; then
    usermod -o -u "$PUID" www-data
fi

# php-fpm corre el master como root y los workers como www-data (pool www.conf).
exec "$@"
