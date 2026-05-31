# Imagen php-fpm por versión. El core de WordPress NO va aquí: lo gestiona el
# panel por tarball. Solo php-fpm + extensiones + entrypoint de mapeo UID/GID.
ARG PHP_VERSION=8.4
FROM php:${PHP_VERSION}-fpm-alpine

# Dependencias de build para extensiones comunes de WordPress.
RUN set -eux; \
    apk add --no-cache --virtual .build-deps \
        $PHPIZE_DEPS freetype-dev libjpeg-turbo-dev libpng-dev libzip-dev icu-dev; \
    docker-php-ext-configure gd --with-freetype --with-jpeg; \
    docker-php-ext-install -j"$(nproc)" mysqli pdo_mysql gd zip intl exif opcache; \
    apk add --no-cache freetype libjpeg-turbo libpng libzip icu-libs; \
    # shadow aporta usermod/groupmod para remapear www-data en runtime
    apk add --no-cache shadow su-exec; \
    # mariadb-client da los binarios `mysql`/`mysqldump` (symlinks) que WP-CLI
    # usa para `wp db import`/`export` (backup, migración, import LocalWP).
    apk add --no-cache mariadb-client; \
    apk del .build-deps

COPY entrypoint.sh /usr/local/bin/panel-entrypoint
RUN chmod +x /usr/local/bin/panel-entrypoint

ENTRYPOINT ["panel-entrypoint"]
CMD ["php-fpm"]
