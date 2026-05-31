//! WP-CLI dentro del container php del proyecto.
//!
//! El phar de WP-CLI se monta en `/usr/local/bin/wp` y corre con el mismo php
//! del container (misma versión que el sitio). Requiere el proyecto encendido.

use anyhow::{anyhow, Result};

use crate::config::SiteConfig;
use crate::docker::DockerManager;

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
    docker.exec_as(&cname, cmd, Some("www-data")).await
}
