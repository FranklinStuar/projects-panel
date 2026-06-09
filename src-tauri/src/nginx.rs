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
pub(crate) fn project_dirname(site: &SiteConfig) -> String {
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
    let dirname = project_dirname(site);

    // Worktree-project: la raíz son los estáticos del PADRE (montados igual que en
    // el container php); el repo objetivo se sirve por `alias` desde el `git
    // worktree`, para que sus assets nuevos se vean. El bloque va ANTES del static
    // genérico para ganarle al match de regex. Ver worktree.rs.
    let (root, worktree_block) = if let Some(ref wt) = site.worktree_of {
        let parent = &wt.parent_dirname;
        let target = wt.target_path.trim_matches('/');
        let basename = crate::config::path_basename(target);
        let block = format!(
            r#"
    location ~ ^/{target}/(.+\.(?:css|js|mjs|png|jpe?g|gif|svg|ico|webp|woff2?|ttf|eot|map|json))$ {{
        alias /srv/projects/{dirname}/wt/{basename}/$1;
        expires 7d;
        access_log off;
    }}
"#
        );
        (format!("/srv/projects/{parent}/app/public"), block)
    } else {
        (format!("/srv/projects/{dirname}/app/public"), String::new())
    };

    // Para clones: uploads nuevos en el clone (rw), uploads viejos del padre ro vía fallback.
    let uploads_block = if let Some(ref ci) = site.clone_of {
        let parent = &ci.parent_dirname;
        format!(
            r#"
    location ^~ /wp-content/uploads/ {{
        root /srv/projects/{dirname}/app/public;
        try_files $uri @uploads_base;
    }}
    location @uploads_base {{
        root /srv/projects/{parent}/app/public;
        try_files $uri =404;
    }}
"#
        )
    } else {
        String::new()
    };

    let mut conf = String::new();

    if site.services.nginx.ssl {
        let ssl_base = format!("/srv/projects/{dirname}/ssl");
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
    }}{worktree_block}{uploads_block}
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
    }}{worktree_block}{uploads_block}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CloneInfo, DbService, DbType, GithubConfig, NginxService, PhpService, Services,
    };

    fn base_site(ssl: bool) -> SiteConfig {
        SiteConfig {
            id: "abc".into(),
            name: "Demo".into(),
            path: "/home/u/panel-wp/demo".into(),
            domain: "demo.test".into(),
            group: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            services: Services {
                php: PhpService { version: "8.3".into() },
                nginx: NginxService { ssl },
                db: DbService {
                    db_type: DbType::Mysql,
                    version: "8.0".into(),
                    db_name: "demo_db".into(),
                },
            },
            github: GithubConfig::default(),
            one_click_admin: true,
            xdebug_enabled: false,
            headless: false,
            frontend_framework: None,
            minio: false,
            migration_pending: false,
            last_migrated_at: None,
            clone_of: None,
            worktree_of: None,
            snapshot_excludes: vec![],
        }
    }

    #[test]
    fn vhost_normal_sin_uploads_block() {
        let site = base_site(false);
        let conf = render_vhost(&site);
        assert!(!conf.contains("uploads_base"), "no-clone no debe tener @uploads_base");
        assert!(conf.contains("server_name demo.test"), "falta server_name");
    }

    #[test]
    fn vhost_clone_incluye_uploads_fallback_http() {
        let mut site = base_site(false);
        site.path = "/home/u/panel-wp/demo-clone".into();
        site.domain = "demo-clone.test".into();
        site.clone_of = Some(CloneInfo {
            parent_id: "parent1".into(),
            parent_dirname: "demo".into(),
            snapshot_id: "snap1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        let conf = render_vhost(&site);
        assert!(conf.contains("@uploads_base"), "clone http debe tener @uploads_base");
        assert!(conf.contains("/srv/projects/demo-clone/app/public"), "debe tener ruta del clone");
        assert!(conf.contains("/srv/projects/demo/app/public"), "debe tener ruta del padre");
        assert!(conf.contains("^~ /wp-content/uploads/"), "debe tener location ^~");
    }

    #[test]
    fn vhost_clone_incluye_uploads_fallback_ssl() {
        let mut site = base_site(true);
        site.path = "/home/u/panel-wp/demo-clone".into();
        site.domain = "demo-clone.test".into();
        site.clone_of = Some(CloneInfo {
            parent_id: "parent1".into(),
            parent_dirname: "demo".into(),
            snapshot_id: "snap1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        let conf = render_vhost(&site);
        assert!(conf.contains("@uploads_base"), "clone ssl debe tener @uploads_base");
        assert!(conf.contains("listen 443 ssl"), "debe tener SSL");
    }

    #[test]
    fn vhost_worktree_root_padre_y_alias_objetivo() {
        use crate::config::WorktreeInfo;
        let mut site = base_site(true);
        site.path = "/home/u/panel-wp/demo-feat".into();
        site.domain = "demo-feat.test".into();
        site.worktree_of = Some(WorktreeInfo {
            parent_id: "parent1".into(),
            parent_dirname: "demo".into(),
            target_path: "wp-content/themes/mi-theme".into(),
            branch: "feat/x".into(),
            shared_db: true,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        let conf = render_vhost(&site);
        // root = padre; alias del objetivo = worktree del propio proyecto.
        assert!(conf.contains("root /srv/projects/demo/app/public"), "root debe ser el del padre:\n{conf}");
        assert!(
            conf.contains("alias /srv/projects/demo-feat/wt/mi-theme/$1"),
            "debe servir el objetivo por alias desde el worktree:\n{conf}"
        );
        assert!(
            conf.contains("^/wp-content/themes/mi-theme/"),
            "el location del objetivo debe usar la ruta del repo:\n{conf}"
        );
    }
}

pub fn remove_vhost(site: &SiteConfig) -> Result<()> {
    let path = vhost_path(site)?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("borrando vhost {:?}", path))?;
    }
    Ok(())
}
