//! WP-CLI dentro del container php del proyecto.
//!
//! El phar de WP-CLI se monta en `/usr/local/bin/wp` y corre con el mismo php
//! del container (misma versión que el sitio). Requiere el proyecto encendido.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::config::SiteConfig;
use crate::docker::DockerManager;

/// Tope para cualquier comando WP-CLI. WP-CLI arranca WordPress entero, así que
/// un plugin/mu-plugin del sitio que haga una llamada HTTP al cargar (licencia,
/// update-check; p. ej. UpdraftPlus) puede colgar el comando ~indefinidamente y
/// con él toda la migración. `--skip-plugins/--skip-themes` evita los plugins
/// normales pero NO los mu-plugins, así que acotamos igual con un timeout.
const WPCLI_TIMEOUT: Duration = Duration::from_secs(120);

/// Ejecuta `wp <args>` en el container del proyecto y devuelve la salida.
pub async fn run(docker: &DockerManager, site: &SiteConfig, args: &[String]) -> Result<String> {
    let cname = site.container_name();
    if !docker.is_running(&cname).await {
        return Err(anyhow!(
            "el proyecto '{}' no está encendido",
            site.name
        ));
    }

    let mut cmd = vec!["php", "/usr/local/bin/wp", "--path=/var/www/html"];
    for a in args {
        cmd.push(a.as_str());
    }
    // WP-CLI como www-data: root está prohibido y rompe la propiedad de archivos.
    tokio::time::timeout(WPCLI_TIMEOUT, docker.exec_as(&cname, cmd, Some("www-data")))
        .await
        .with_context(|| {
            format!(
                "WP-CLI excedió {}s en '{}' (¿un plugin/mu-plugin hace una llamada de red al cargar?)",
                WPCLI_TIMEOUT.as_secs(),
                site.name
            )
        })?
}
