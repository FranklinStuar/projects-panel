<?php
/**
 * Plugin Name: Panel WP — Mailpit
 * Description: Enruta todo el correo del proyecto al Mailpit compartido del panel
 *              y etiqueta cada mensaje con el id del proyecto (X-Project-ID), para
 *              que el panel pueda filtrar la bandeja por proyecto.
 *
 * El panel sustituye __PROJECT_ID__ por el id real al inyectar este archivo.
 */

defined( 'ABSPATH' ) || exit;

add_action( 'phpmailer_init', function ( $mailer ) {
    $mailer->isSMTP();
    $mailer->Host     = 'panel-mailpit';   // resoluble por panel-net
    $mailer->Port     = 1025;
    $mailer->SMTPAuth = false;
    $mailer->addCustomHeader( 'X-Project-ID', '__PROJECT_ID__' );
} );
