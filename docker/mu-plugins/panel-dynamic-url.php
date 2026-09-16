<?php
/**
 * Plugin Name: Panel WP — URL dinámica
 * Description: Sirve `siteurl`/`home` según el host real de la petición en vez
 *              del valor fijo guardado en la base de datos. Respeta
 *              X-Forwarded-Host (lo pone `cloudflared` con el hostname público
 *              real del túnel, aunque el panel fuerce el Host: local para que
 *              nginx enrute al vhost correcto — ver docs/CLOUDFLARE_TUNNEL_PLAN.md).
 *              Así los assets/enlaces funcionan igual por `*.test` que por la
 *              URL pública temporal del túnel. Sin efecto en WP-CLI/cron (no
 *              hay request HTTP: se usa el valor guardado).
 *
 * `WP_CONTENT_URL`/`WP_PLUGIN_URL` se calculan UNA vez al arrancar WP, antes
 * de que carguen los mu-plugins — filtrar `option_siteurl`/`option_home` no
 * los alcanza. `content_url()`/`plugins_url()`/`get_theme_root_uri()` sí se
 * re-evalúan por request y aplican su propio filtro sobre el resultado ya
 * armado, así que reescribirlos igual (solo esquema+host) basta para que
 * plugins y temas también apunten al host correcto.
 */

defined( 'ABSPATH' ) || exit;

foreach ( array( 'option_siteurl', 'option_home', 'content_url', 'plugins_url', 'theme_root_uri' ) as $panel_url_filter ) {
    add_filter( $panel_url_filter, 'panel_dynamic_url' );
}

function panel_dynamic_url( $url ) {
    if ( php_sapi_name() === 'cli' ) {
        return $url;
    }
    $host = $_SERVER['HTTP_X_FORWARDED_HOST'] ?? $_SERVER['HTTP_HOST'] ?? '';
    if ( $host === '' ) {
        return $url;
    }
    $forwarded_https = ( $_SERVER['HTTP_X_FORWARDED_PROTO'] ?? '' ) === 'https';
    $scheme = ( $forwarded_https || is_ssl() ) ? 'https' : 'http';
    $path = (string) wp_parse_url( $url, PHP_URL_PATH );
    $query = wp_parse_url( $url, PHP_URL_QUERY );
    return $scheme . '://' . $host . $path . ( $query ? '?' . $query : '' );
}

/**
 * Media insertada en contenido (post_content, guid de adjuntos, ACF, etc.)
 * guarda su URL completa LITERAL en la base de datos — eso no pasa por
 * `content_url()`/`option_home` en cada request, así que los filtros de
 * arriba no la alcanzan. Solo para esos casos, reescribe el HTML final del
 * front-end (nunca wp-admin) cambiando el dominio local guardado por el del
 * túnel. Gateado en `X-Forwarded-Host`: sin túnel de por medio esto no hace
 * nada (cero costo en el uso local normal).
 *
 * Un worktree-project (`worktree.rs`) con DB compartida NO tiene su propia
 * fila en `wp_options` — fija su dominio por constantes `WP_HOME`/
 * `WP_SITEURL` en su wp-config propio, sobre la MISMA base del padre.
 * Contenido guardado mientras se editaba con el dominio del worktree (no el
 * del padre) queda con ESE dominio literal en la base compartida. Por eso
 * se reemplazan TODOS los dominios locales conocidos (el de `wp_options` +
 * los de las constantes, si están definidas), no solo uno.
 */
add_action( 'template_redirect', function () {
    if ( is_admin() || php_sapi_name() === 'cli' ) {
        return;
    }
    $host = $_SERVER['HTTP_X_FORWARDED_HOST'] ?? '';
    if ( $host === '' ) {
        return;
    }
    global $wpdb;
    $raw_home = $wpdb->get_var( "SELECT option_value FROM {$wpdb->options} WHERE option_name = 'home' LIMIT 1" );
    $candidates = array( $raw_home, defined( 'WP_HOME' ) ? WP_HOME : null, defined( 'WP_SITEURL' ) ? WP_SITEURL : null );
    $raw_hosts = array();
    foreach ( $candidates as $raw ) {
        if ( ! $raw ) {
            continue;
        }
        $h = wp_parse_url( $raw, PHP_URL_HOST );
        if ( ! $h ) {
            continue;
        }
        $p = wp_parse_url( $raw, PHP_URL_PORT );
        $raw_hosts[] = $p ? "{$h}:{$p}" : $h;
    }
    $raw_hosts = array_unique( $raw_hosts );
    if ( empty( $raw_hosts ) ) {
        return;
    }
    // Solo dominio:puerto (sin esquema): así funciona igual si el HTML trae la
    // URL con `/` normales o escapadas (`\/`, común en JSON-LD/atributos data-*
    // de plugins de galería) — el esquema no cambia entre local y túnel aquí.
    ob_start( function ( $html ) use ( $raw_hosts, $host ) {
        return str_ireplace( $raw_hosts, $host, $html );
    } );
} );
