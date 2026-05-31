<?php
/**
 * Plugin Name: Panel WP — Auto-login
 * Description: Login de un clic desde el panel mediante un token efímero de un
 *              solo uso (transient). El panel genera el token, lo guarda como
 *              transient (60s) y abre /?panel_autologin=TOKEN. Aquí se valida,
 *              se borra (un solo uso) y se loguea al primer administrador.
 */

defined( 'ABSPATH' ) || exit;

add_action( 'init', function () {
    if ( empty( $_GET['panel_autologin'] ) ) {
        return;
    }
    $token = preg_replace( '/[^a-z0-9]/i', '', (string) $_GET['panel_autologin'] );
    if ( $token === '' ) {
        return;
    }

    $key = 'panel_autologin_' . $token;
    if ( get_transient( $key ) === false ) {
        return; // token inválido o ya usado
    }
    delete_transient( $key ); // un solo uso

    $admins = get_users( array( 'role' => 'administrator', 'number' => 1 ) );
    if ( empty( $admins ) ) {
        return;
    }
    $user = $admins[0];

    wp_set_current_user( $user->ID );
    wp_set_auth_cookie( $user->ID, true );

    wp_safe_redirect( admin_url() );
    exit;
} );
