//! Orquestación Docker vía bollard.
//!
//! Materializa el principio rector: nada corre si no hace falta, y los servicios
//! con necesidades iguales se comparten. Containers por proyecto solo mientras el
//! proyecto está activo; nginx/DB/mailpit compartidos y on-demand.

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding};
use bollard::network::CreateNetworkOptions;
use bollard::Docker;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{DbService, SiteConfig, SiteStatus};
use crate::{domain, nginx};

pub const NETWORK: &str = "panel-net";
pub const NGINX: &str = "panel-nginx";
pub const MAILPIT: &str = "panel-mailpit";

/// Prefijo común de todo lo que gestiona el panel (para detectar huérfanos).
pub const PANEL_PREFIXES: &[&str] = &["wp-", "panel-"];

pub struct DockerManager {
    docker: Docker,
}

impl DockerManager {
    pub fn connect() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("no se pudo conectar al daemon de Docker")?;
        Ok(Self { docker })
    }

    #[allow(dead_code)] // usado por logs.rs / dbus.rs en Fase 2
    pub fn raw(&self) -> &Docker {
        &self.docker
    }

    // -- red ----------------------------------------------------------------

    /// Crea el bridge `panel-net` si no existe. Prerequisito de todo lo demás.
    pub async fn ensure_network(&self) -> Result<()> {
        let nets = self.docker.list_networks::<String>(None).await?;
        if nets.iter().any(|n| n.name.as_deref() == Some(NETWORK)) {
            return Ok(());
        }
        self.docker
            .create_network(CreateNetworkOptions {
                name: NETWORK,
                driver: "bridge",
                ..Default::default()
            })
            .await
            .context("creando red panel-net")?;
        Ok(())
    }

    // -- introspección ------------------------------------------------------

    pub async fn is_running(&self, name: &str) -> bool {
        match self.docker.inspect_container(name, None).await {
            Ok(info) => info
                .state
                .and_then(|s| s.running)
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn exists(&self, name: &str) -> bool {
        self.docker.inspect_container(name, None).await.is_ok()
    }

    /// Containers del panel actualmente corriendo (nombres sin la `/` inicial).
    #[allow(dead_code)] // detección de huérfanos (shutdown.rs) en Fase 2
    pub async fn running_panel_containers(&self) -> Result<Vec<String>> {
        let opts = ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        };
        let list = self.docker.list_containers(Some(opts)).await?;
        let mut out = Vec::new();
        for c in list {
            if let Some(names) = c.names {
                for n in names {
                    let n = n.trim_start_matches('/').to_string();
                    if PANEL_PREFIXES.iter().any(|p| n.starts_with(p)) {
                        out.push(n);
                    }
                }
            }
        }
        Ok(out)
    }

    // -- imágenes -----------------------------------------------------------

    /// Hace pull de una imagen si no está localmente.
    pub async fn ensure_image(&self, image: &str) -> Result<()> {
        if self.docker.inspect_image(image).await.is_ok() {
            return Ok(());
        }
        let opts = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };
        let mut stream = self.docker.create_image(Some(opts), None, None);
        while let Some(item) = stream.next().await {
            item.with_context(|| format!("descargando imagen {image}"))?;
        }
        Ok(())
    }

    // -- servicios compartidos (on-demand) ----------------------------------

    /// Arranca (si hace falta) el container DB compartido para una versión.
    /// Devuelve el nombre del container (`panel-mysql-80`, ...).
    pub async fn ensure_db(&self, db: &DbService) -> Result<String> {
        let name = format!(
            "{}-{}",
            db.db_type.service_prefix(),
            db.version.replace('.', "")
        );
        if self.is_running(&name).await {
            return Ok(name);
        }

        let image = db.db_type.image(&db.version);
        self.ensure_image(&image).await?;

        if self.exists(&name).await {
            // existe parado → solo arrancar
            self.docker
                .start_container(&name, None::<StartContainerOptions<String>>)
                .await?;
            return Ok(name);
        }

        let env = db_env(db);
        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            ..Default::default()
        };
        let config = Config {
            image: Some(image),
            env: Some(env),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    platform: None,
                }),
                config,
            )
            .await
            .with_context(|| format!("creando container {name}"))?;
        self.docker
            .start_container(&name, None::<StartContainerOptions<String>>)
            .await?;
        Ok(name)
    }

    /// Arranca el reverse-proxy nginx compartido si no está corriendo.
    /// Monta el directorio de vhosts (host) y la raíz de proyectos (ro).
    pub async fn ensure_nginx(&self) -> Result<()> {
        if self.is_running(NGINX).await {
            return Ok(());
        }
        self.ensure_image("nginx:alpine").await?;

        if self.exists(NGINX).await {
            self.docker
                .start_container(NGINX, None::<StartContainerOptions<String>>)
                .await?;
            return Ok(());
        }

        let conf_d = nginx::conf_d_dir()?;
        let projects = crate::config::projects_root()?;

        let mut ports = HashMap::new();
        ports.insert(
            "80/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some("80".to_string()),
            }]),
        );
        ports.insert(
            "443/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some("443".to_string()),
            }]),
        );

        let binds = vec![
            format!("{}:/etc/nginx/conf.d:ro", conf_d.display()),
            format!("{}:/srv/projects:ro", projects.display()),
        ];

        let mut exposed = HashMap::new();
        exposed.insert("80/tcp".to_string(), HashMap::new());
        exposed.insert("443/tcp".to_string(), HashMap::new());

        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            binds: Some(binds),
            port_bindings: Some(ports),
            ..Default::default()
        };
        let config = Config {
            image: Some("nginx:alpine".to_string()),
            exposed_ports: Some(exposed),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: NGINX.to_string(),
                    platform: None,
                }),
                config,
            )
            .await
            .context("creando container panel-nginx")?;
        self.docker
            .start_container(NGINX, None::<StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    /// `nginx -s reload` sin cortar conexiones. No-op si nginx no corre.
    pub async fn reload_nginx(&self) -> Result<()> {
        if !self.is_running(NGINX).await {
            return Ok(());
        }
        self.exec(NGINX, vec!["nginx", "-s", "reload"]).await?;
        Ok(())
    }

    // -- ciclo de vida de un proyecto ---------------------------------------

    /// Enciende un proyecto: red + DB + nginx + container php + vhost + reload.
    pub async fn start_site(&self, site: &SiteConfig) -> Result<()> {
        self.ensure_network().await?;
        self.ensure_db(&site.services.db).await?;

        let cname = site.container_name();
        let image = crate::php::ensure_php_image(&site.services.php.version).await?;

        if !self.exists(&cname).await {
            self.create_php_container(site, &image).await?;
        }
        if !self.is_running(&cname).await {
            self.docker
                .start_container(&cname, None::<StartContainerOptions<String>>)
                .await?;
        }

        // Publicar vhost en panel-nginx y recargar.
        nginx::write_vhost(site)?;
        self.ensure_nginx().await?;
        self.reload_nginx().await?;

        // Asegurar que el dominio resuelve (dnsmasq wildcard *.test).
        domain::ensure_wildcard().ok();
        Ok(())
    }

    async fn create_php_container(&self, site: &SiteConfig, image: &str) -> Result<()> {
        let (uid, gid) = host_uid_gid();
        let public = site.public_dir();
        let php_ini = site.php_ini();
        let wp_cli = crate::php::wp_cli_phar_path().await?;

        let binds = vec![
            format!("{}:/var/www/html", public.display()),
            format!(
                "{}:/usr/local/etc/php/conf.d/zz-project.ini:ro",
                php_ini.display()
            ),
            format!("{}:/usr/local/bin/wp:ro", wp_cli.display()),
        ];

        let env = vec![format!("PUID={uid}"), format!("PGID={gid}")];

        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            binds: Some(binds),
            // NO se publican puertos al host: solo panel-nginx le habla por panel-net.
            ..Default::default()
        };
        let config = Config {
            image: Some(image.to_string()),
            env: Some(env),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: site.container_name(),
                    platform: None,
                }),
                config,
            )
            .await
            .with_context(|| format!("creando container {}", site.container_name()))?;
        Ok(())
    }

    /// Detiene un proyecto: quita vhost + reload, para el container php y, si ya
    /// nadie usa los servicios compartidos, los apaga (0 recursos).
    pub async fn stop_site(&self, site: &SiteConfig, others: &[SiteConfig]) -> Result<()> {
        let cname = site.container_name();
        if self.is_running(&cname).await {
            self.docker
                .stop_container(&cname, Some(StopContainerOptions { t: 10 }))
                .await
                .ok();
        }
        nginx::remove_vhost(site)?;
        self.reload_nginx().await.ok();

        self.teardown_unused_shared(site, others).await.ok();
        Ok(())
    }

    /// Apaga servicios compartidos que ya no necesita ningún proyecto activo.
    async fn teardown_unused_shared(
        &self,
        stopped: &SiteConfig,
        all: &[SiteConfig],
    ) -> Result<()> {
        // ¿Algún OTRO proyecto sigue su container php corriendo?
        let mut active_dbs = Vec::new();
        let mut any_active = false;
        for s in all {
            if s.id == stopped.id {
                continue;
            }
            if self.is_running(&s.container_name()).await {
                any_active = true;
                active_dbs.push(format!(
                    "{}-{}",
                    s.services.db.db_type.service_prefix(),
                    s.services.db.version.replace('.', "")
                ));
            }
        }

        // DB del proyecto detenido: apagar si ningún activo la comparte.
        let db_name = format!(
            "{}-{}",
            stopped.services.db.db_type.service_prefix(),
            stopped.services.db.version.replace('.', "")
        );
        if !active_dbs.contains(&db_name) && self.is_running(&db_name).await {
            self.docker
                .stop_container(&db_name, Some(StopContainerOptions { t: 10 }))
                .await
                .ok();
        }

        // nginx: si no queda ningún proyecto activo, apagarlo también.
        if !any_active && self.is_running(NGINX).await {
            self.docker
                .stop_container(NGINX, Some(StopContainerOptions { t: 10 }))
                .await
                .ok();
        }
        Ok(())
    }

    pub async fn site_status(&self, site: &SiteConfig) -> SiteStatus {
        if site.migration_pending {
            return SiteStatus::MigrationPending;
        }
        if self.is_running(&site.container_name()).await {
            SiteStatus::Running
        } else {
            SiteStatus::Stopped
        }
    }

    #[allow(dead_code)] // limpieza de huérfanos / recrear container en Fase 2
    pub async fn remove_container(&self, name: &str) -> Result<()> {
        if self.exists(name).await {
            self.docker
                .remove_container(
                    name,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await?;
        }
        Ok(())
    }

    // -- exec ---------------------------------------------------------------

    /// Ejecuta un comando en un container y devuelve stdout+stderr combinado.
    pub async fn exec(&self, container: &str, cmd: Vec<&str>) -> Result<String> {
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("create_exec en {container}"))?;

        let started = self.docker.start_exec(&exec.id, None).await?;
        let mut out = String::new();
        if let StartExecResults::Attached { mut output, .. } = started {
            while let Some(chunk) = output.next().await {
                let msg = chunk?;
                out.push_str(&msg.to_string());
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// helpers libres
// ---------------------------------------------------------------------------

/// uid/gid del usuario host, para mapear www-data dentro del container.
pub fn host_uid_gid() -> (u32, u32) {
    // SAFETY: getuid/getgid no fallan ni tienen efectos secundarios.
    unsafe { (libc_getuid(), libc_getgid()) }
}

// Evita una dependencia extra: bindings mínimos a libc.
extern "C" {
    fn getuid() -> u32;
    fn getgid() -> u32;
}
unsafe fn libc_getuid() -> u32 {
    getuid()
}
unsafe fn libc_getgid() -> u32 {
    getgid()
}

fn db_env(db: &DbService) -> Vec<String> {
    use crate::config::DbType::*;
    match db.db_type {
        Mysql | Mariadb => vec![
            "MYSQL_ROOT_PASSWORD=panel".to_string(),
            // root@% para que php-fpm del proyecto conecte por la red interna.
            "MYSQL_ROOT_HOST=%".to_string(),
        ],
        Postgres => vec![
            "POSTGRES_PASSWORD=panel".to_string(),
            "POSTGRES_USER=panel".to_string(),
        ],
    }
}

/// Ruta del Dockerfile/context de la imagen php (repo en dev).
pub fn docker_assets_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../docker"))
}
