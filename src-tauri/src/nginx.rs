//! Generación de vhosts para el `panel-nginx` compartido.
//!
//! Los vhosts viven en `~/.config/wordpress-panel/nginx/conf.d/` (montado ro en
//! el container). Cada proyecto activo aporta `{id}.conf`. Al alta/baja se hace
//! `nginx -s reload`. El proyecto NO tiene su propia config nginx por container.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::SiteConfig;

pub fn conf_d_dir() -> Result<PathBuf> {
    let dir = crate::config::config_dir()?.join("nginx").join("conf.d");
    std::fs::create_dir_all(&dir).context("creando conf.d de nginx")?;
    Ok(dir)
}

fn vhost_path(site: &SiteConfig) -> Result<PathBuf> {
    Ok(conf_d_dir()?.join(format!("{}.conf", site.id)))
}

/// Nombre de la carpeta del proyecto bajo `~/panel-wp/` (= basename de path).
fn project_dirname(site: &SiteConfig) -> String {
    Path::new(&site.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&site.name)
        .to_string()
}

pub fn render_vhost(site: &SiteConfig) -> String {
    let server_name = &site.domain;
    let upstream = format!("{}:9000", site.container_name());
    // nginx ve los archivos en /srv/projects (ro); php los ve en /var/www/html.
    let root = format!("/srv/projects/{}/app/public", project_dirname(site));

    let mut conf = String::new();

    if site.services.nginx.ssl {
        // El cert vive en la carpeta del proyecto, visible por nginx en /srv/projects.
        let ssl_base = format!("/srv/projects/{}/ssl", project_dirname(site));
        conf.push_str(&format!(
            r#"server {{
    listen 80;
    server_name {server_name};
    return 301 https://$host$request_uri;
}}

server {{
    listen 443 ssl;
    http2 on;
    server_name {server_name};

    ssl_certificate     {ssl_base}/cert.pem;
    ssl_certificate_key {ssl_base}/key.pem;

    root {root};
    index index.php index.html;

    location / {{
        try_files $uri $uri/ /index.php?$args;
    }}

    location ~ \.php$ {{
        fastcgi_pass {upstream};
        fastcgi_index index.php;
        include fastcgi_params;
        # SCRIPT_FILENAME en la vista del container php (no la de nginx).
        fastcgi_param SCRIPT_FILENAME /var/www/html$fastcgi_script_name;
        fastcgi_param HTTPS on;
    }}

    location ~* \.(?:css|js|png|jpe?g|gif|svg|ico|woff2?)$ {{
        expires 7d;
        access_log off;
    }}
}}
"#
        ));
    } else {
        conf.push_str(&format!(
            r#"server {{
    listen 80;
    server_name {server_name};

    root {root};
    index index.php index.html;

    location / {{
        try_files $uri $uri/ /index.php?$args;
    }}

    location ~ \.php$ {{
        fastcgi_pass {upstream};
        fastcgi_index index.php;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME /var/www/html$fastcgi_script_name;
    }}

    location ~* \.(?:css|js|png|jpe?g|gif|svg|ico|woff2?)$ {{
        expires 7d;
        access_log off;
    }}
}}
"#
        ));
    }

    conf
}

pub fn write_vhost(site: &SiteConfig) -> Result<()> {
    let path = vhost_path(site)?;
    std::fs::write(&path, render_vhost(site))
        .with_context(|| format!("escribiendo vhost {:?}", path))?;
    Ok(())
}

pub fn remove_vhost(site: &SiteConfig) -> Result<()> {
    let path = vhost_path(site)?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("borrando vhost {:?}", path))?;
    }
    Ok(())
}
