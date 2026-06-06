//! Orquestación Docker vía bollard.
//!
//! Materializa el principio rector: nada corre si no hace falta, y los servicios
//! con necesidades iguales se comparten. Containers por proyecto solo mientras el
//! proyecto está activo; nginx/DB/mailpit compartidos y on-demand.

use anyhow::{anyhow, Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogOutput, RemoveContainerOptions,
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

use crate::config::{DbService, DbType, SiteConfig, SiteStatus};
use crate::{domain, nginx};

pub const NETWORK: &str = "panel-net";
pub const NGINX: &str = "panel-nginx";
pub const MAILPIT: &str = "panel-mailpit";
pub const MINIO: &str = "panel-minio";
pub const ADMINER: &str = "panel-adminer";

/// Puertos host (solo 127.0.0.1) de las UIs de servicios compartidos.
pub const MAILPIT_UI_PORT: u16 = 8025;
pub const MINIO_API_PORT: u16 = 9100;
pub const MINIO_CONSOLE_PORT: u16 = 9101;
pub const ADMINER_UI_PORT: u16 = 8088;

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

    /// ¿Existe ya el bridge `panel-net`? (para el estado del sistema).
    pub async fn network_exists(&self) -> bool {
        self.docker
            .list_networks::<String>(None)
            .await
            .map(|nets| nets.iter().any(|n| n.name.as_deref() == Some(NETWORK)))
            .unwrap_or(false)
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

    /// Tag de imagen con el que se creó el container (`config.image`), si existe.
    pub async fn container_image(&self, name: &str) -> Option<String> {
        self.docker
            .inspect_container(name, None)
            .await
            .ok()
            .and_then(|info| info.config.and_then(|c| c.image))
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
        let name = db_container_name(db);
        if self.is_running(&name).await {
            return Ok(name);
        }

        let image = db.db_type.image(&db.version);
        self.ensure_image(&image).await?;

        let data_dir = db_data_dir(db)?;
        let datadir_in = db.db_type.datadir();

        if self.exists(&name).await {
            // Containers creados antes del almacenamiento durable no tienen el
            // bind: sus datos viven en la capa de escritura del container y se
            // pierden si se recrea. Migrarlos al host antes de seguir.
            if self.db_has_volume(&name, datadir_in).await {
                self.docker
                    .start_container(&name, None::<StartContainerOptions<String>>)
                    .await?;
                self.wait_db_ready(&name, db).await?;
                return Ok(name);
            }
            self.migrate_db_to_volume(&name, &data_dir, datadir_in).await?;
            // El container viejo ya fue eliminado; cae al create con bind abajo.
        }

        let env = db_env(db);
        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            binds: Some(vec![format!("{}:{}", data_dir.display(), datadir_in)]),
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
        self.wait_db_ready(&name, db).await?;
        Ok(name)
    }

    /// ¿El container DB ya tiene montado su datadir en el host? (bind durable).
    async fn db_has_volume(&self, name: &str, datadir_in: &str) -> bool {
        match self.docker.inspect_container(name, None).await {
            Ok(info) => info
                .mounts
                .map(|ms| {
                    ms.iter()
                        .any(|m| m.destination.as_deref() == Some(datadir_in))
                })
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Migra un container DB legado (sin bind) a almacenamiento durable: copia su
    /// datadir de la capa de escritura al host y elimina el container para que
    /// `ensure_db` lo recree con el bind. Copia lossless (archivos, agnóstica de
    /// versión), así no se pierde ninguna base de datos.
    ///
    /// Usa el CLI `docker cp` (excepción documentada al uso exclusivo de bollard):
    /// extraer un directorio por el stream tar de bollard es complejo y `cp` lo
    /// hace en un paso. Es una migración de una sola vez por container.
    async fn migrate_db_to_volume(
        &self,
        name: &str,
        host_dir: &std::path::Path,
        datadir_in: &str,
    ) -> Result<()> {
        // No copiar sobre un host_dir ya poblado (evita mezclar dos datadirs).
        let host_empty = std::fs::read_dir(host_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
        if host_empty {
            let src = format!("{name}:{datadir_in}/.");
            let status = std::process::Command::new("docker")
                .arg("cp")
                .arg(&src)
                .arg(host_dir)
                .status()
                .context("ejecutando `docker cp` para migrar la DB a volumen durable")?;
            if !status.success() {
                return Err(anyhow!(
                    "`docker cp {src}` falló al migrar la DB a almacenamiento durable"
                ));
            }
        }
        self.docker
            .remove_container(
                name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .with_context(|| format!("eliminando container DB legado {name}"))?;
        Ok(())
    }

    /// Espera a que la DB acepte conexiones **por TCP** antes de seguir. Crítico:
    /// en su primer arranque, la imagen oficial de MySQL acepta el socket local
    /// (fase de init `--skip-networking`) ANTES de abrir el puerto TCP; si
    /// `create_database`/`wp config create` corren en esa ventana, fallan. El
    /// chequeo fuerza `-h127.0.0.1` para gatear exactamente sobre el TCP.
    async fn wait_db_ready(&self, container: &str, db: &DbService) -> Result<()> {
        let check: Vec<&str> = match db.db_type {
            DbType::Mysql | DbType::Mariadb => {
                vec!["mysql", "-h127.0.0.1", "-uroot", "-ppanel", "-e", "SELECT 1"]
            }
            DbType::Postgres => vec!["pg_isready", "-h", "127.0.0.1", "-U", "panel"],
        };
        for _ in 0..120 {
            if self.exec(container, check.clone()).await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(anyhow!(
            "la base de datos '{container}' no estuvo lista (timeout 60s)"
        ))
    }

    /// Arranca el capturador de correo compartido `panel-mailpit` (axllent/mailpit).
    /// SMTP en `:1025` (solo red interna); UI web en `127.0.0.1:8025`.
    pub async fn ensure_mailpit(&self) -> Result<()> {
        if self.is_running(MAILPIT).await {
            return Ok(());
        }
        if self.exists(MAILPIT).await {
            self.docker
                .start_container(MAILPIT, None::<StartContainerOptions<String>>)
                .await?;
            return Ok(());
        }
        let image = "axllent/mailpit:latest";
        self.ensure_image(image).await?;

        let ports = host_port_map(&[(MAILPIT_UI_PORT, "8025/tcp")]);
        let mut exposed = HashMap::new();
        exposed.insert("8025/tcp".to_string(), HashMap::new());
        exposed.insert("1025/tcp".to_string(), HashMap::new());

        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            port_bindings: Some(ports),
            ..Default::default()
        };
        let config = Config {
            image: Some(image.to_string()),
            exposed_ports: Some(exposed),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: MAILPIT.to_string(),
                    platform: None,
                }),
                config,
            )
            .await
            .context("creando container panel-mailpit")?;
        self.docker
            .start_container(MAILPIT, None::<StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    /// Arranca el S3 local compartido `panel-minio` (on-demand, solo si un proyecto
    /// lo pide). API S3 en `127.0.0.1:9100`, consola web en `127.0.0.1:9101`.
    pub async fn ensure_minio(&self) -> Result<()> {
        if self.is_running(MINIO).await {
            return Ok(());
        }
        if self.exists(MINIO).await {
            self.docker
                .start_container(MINIO, None::<StartContainerOptions<String>>)
                .await?;
            return Ok(());
        }
        let image = "minio/minio:latest";
        self.ensure_image(image).await?;

        let data = crate::config::config_dir()?.join("minio-data");
        std::fs::create_dir_all(&data).ok();

        let ports = host_port_map(&[
            (MINIO_API_PORT, "9000/tcp"),
            (MINIO_CONSOLE_PORT, "9001/tcp"),
        ]);
        let mut exposed = HashMap::new();
        exposed.insert("9000/tcp".to_string(), HashMap::new());
        exposed.insert("9001/tcp".to_string(), HashMap::new());

        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            port_bindings: Some(ports),
            binds: Some(vec![format!("{}:/data", data.display())]),
            ..Default::default()
        };
        let config = Config {
            image: Some(image.to_string()),
            cmd: Some(vec![
                "server".to_string(),
                "/data".to_string(),
                "--console-address".to_string(),
                ":9001".to_string(),
            ]),
            env: Some(vec![
                "MINIO_ROOT_USER=panel".to_string(),
                "MINIO_ROOT_PASSWORD=panel-secret".to_string(),
            ]),
            exposed_ports: Some(exposed),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: MINIO.to_string(),
                    platform: None,
                }),
                config,
            )
            .await
            .context("creando container panel-minio")?;
        self.docker
            .start_container(MINIO, None::<StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    /// Arranca el visor de bases de datos compartido `panel-adminer` (Adminer 4).
    /// UI web en `127.0.0.1:8088`; habla con los containers DB por `panel-net`.
    /// On-demand: solo cuando un proyecto pide ver su base de datos.
    ///
    /// Monta `docker/adminer/autologin.php` como plugin (auto-login con las
    /// credenciales del entorno, ver el propio archivo).
    pub async fn ensure_adminer(&self) -> Result<()> {
        if self.is_running(ADMINER).await {
            return Ok(());
        }
        if self.exists(ADMINER).await {
            self.docker
                .start_container(ADMINER, None::<StartContainerOptions<String>>)
                .await?;
            return Ok(());
        }
        let image = "adminer:4";
        self.ensure_image(image).await?;

        let plugin = docker_assets_dir().join("adminer").join("autologin.php");

        let ports = host_port_map(&[(ADMINER_UI_PORT, "8080/tcp")]);
        let mut exposed = HashMap::new();
        exposed.insert("8080/tcp".to_string(), HashMap::new());

        let host_config = HostConfig {
            network_mode: Some(NETWORK.to_string()),
            port_bindings: Some(ports),
            binds: Some(vec![format!(
                "{}:/var/www/html/plugins-enabled/autologin.php:ro",
                plugin.display()
            )]),
            ..Default::default()
        };
        let config = Config {
            image: Some(image.to_string()),
            exposed_ports: Some(exposed),
            host_config: Some(host_config),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: ADMINER.to_string(),
                    platform: None,
                }),
                config,
            )
            .await
            .context("creando container panel-adminer")?;
        self.docker
            .start_container(ADMINER, None::<StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    /// Arranca el reverse-proxy nginx compartido si no está corriendo.
    /// Monta el directorio de vhosts (host) y la raíz de proyectos (ro).
    ///
    /// Antes de bindear elige un *endpoint* libre (ver `select_endpoint`) y hace
    /// preflight para fallar con un mensaje claro en vez del 500 opaco de Docker.
    pub async fn ensure_nginx(&self) -> Result<()> {
        if self.is_running(NGINX).await {
            return Ok(());
        }

        let mut ep = Self::select_endpoint()?;
        Self::ensure_endpoint_dns(&ep)?;
        // El endpoint persistido puede haber quedado inservible (p. ej. LocalWP
        // tomó el puerto mientras el panel estaba apagado). Si ya no es bindeable,
        // re-elegimos y re-persistimos en vez de fallar con "puerto ocupado".
        if Self::preflight_endpoint(&ep).is_err() {
            ep = Self::autoselect_endpoint();
            crate::config::save_endpoint(&ep)?;
            Self::ensure_endpoint_dns(&ep)?;
            Self::preflight_endpoint(&ep)?;
        }

        self.ensure_image("nginx:alpine").await?;

        // Un panel-nginx parado puede arrastrar un binding viejo (de un intento
        // anterior con el puerto ocupado): recrear siempre con el endpoint actual.
        if self.exists(NGINX).await {
            self.docker
                .remove_container(
                    NGINX,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
        }

        let conf_d = nginx::conf_d_dir()?;
        let projects = crate::config::projects_root()?;

        let mut ports = HashMap::new();
        ports.insert(
            "80/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some(ep.loopback_ip.clone()),
                host_port: Some(ep.http_port.to_string()),
            }]),
        );
        ports.insert(
            "443/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some(ep.loopback_ip.clone()),
                host_port: Some(ep.https_port.to_string()),
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

    // -- selección del punto de publicación (endpoint) ----------------------

    /// Endpoint a usar: el persistido (estable para sitios ya instalados) o uno
    /// autodetectado la primera vez. Ver `config::Endpoint`.
    fn select_endpoint() -> Result<crate::config::Endpoint> {
        if let Some(ep) = crate::config::load_endpoint()? {
            return Ok(ep);
        }
        let ep = Self::autoselect_endpoint();
        crate::config::save_endpoint(&ep)?;
        Ok(ep)
    }

    /// Cede 80/443 a LocalWP: el panel SIEMPRE publica en puertos altos para
    /// coexistir sin choques (las URLs del panel llevan el puerto, p. ej. :8443).
    /// Elige el primer par libre desde 8080/8443.
    fn autoselect_endpoint() -> crate::config::Endpoint {
        use crate::config::Endpoint;
        use crate::netcheck;

        let hp = netcheck::pick_alt_port(8080).unwrap_or(8080);
        let mut sp = netcheck::pick_alt_port(8443).unwrap_or(8443);
        if sp == hp {
            sp = netcheck::pick_alt_port(hp + 1).unwrap_or(hp + 1);
        }
        Endpoint {
            loopback_ip: crate::domain::DEFAULT_IP.to_string(),
            http_port: hp,
            https_port: sp,
        }
    }

    /// Si el endpoint usa una IP loopback alterna, asegura que dnsmasq la resuelve
    /// (instalación privilegiada vía pkexec, idempotente).
    fn ensure_endpoint_dns(ep: &crate::config::Endpoint) -> Result<()> {
        if ep.loopback_ip == crate::domain::DEFAULT_IP {
            return Ok(());
        }
        if crate::domain::resolves_to(&ep.loopback_ip) {
            return Ok(());
        }
        crate::domain::install_wildcard(&ep.loopback_ip).with_context(|| {
            format!("apuntando dnsmasq a {} para el panel", ep.loopback_ip)
        })?;
        Ok(())
    }

    /// Verifica que el endpoint elegido sea realmente bindeable; si no, error
    /// claro (nombrando al proceso que lo ocupa) en vez del 500 de Docker.
    fn preflight_endpoint(ep: &crate::config::Endpoint) -> Result<()> {
        use crate::netcheck;
        let ip: std::net::Ipv4Addr = ep
            .loopback_ip
            .parse()
            .map_err(|_| anyhow!("IP loopback inválida en la config: {}", ep.loopback_ip))?;
        for (port, label) in [(ep.http_port, "HTTP"), (ep.https_port, "HTTPS")] {
            if !netcheck::port_status(port).free_for(ip) {
                let who = netcheck::holder_name(port)
                    .map(|n| format!(" (lo usa: {n})"))
                    .unwrap_or_default();
                return Err(anyhow!(
                    "el puerto {port} ({label}) ya está ocupado en {ip}{who}. \
                     Apaga ese servicio (¿LocalWP?) o borra \
                     ~/.config/wordpress-panel/panel.json para reasignar puerto."
                ));
            }
        }
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
        // Mailpit captura el correo de todos los proyectos activos.
        self.ensure_mailpit().await.ok();
        if site.minio {
            self.ensure_minio().await.ok();
        }

        let cname = site.container_name();
        let image = crate::php::ensure_php_image(&site.services.php.version).await?;

        // Si el container existe pero se creó con OTRA imagen (p. ej. tras subir
        // IMAGE_REV), recrearlo para que tome la nueva (forzado: puede correr).
        if self.exists(&cname).await && self.container_image(&cname).await.as_deref() != Some(&image)
        {
            self.docker
                .remove_container(
                    &cname,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
        }

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
            // Export-al-detener: deja un dump fresco en app/sql/ (migración +
            // protección de datos) antes de apagar. Best-effort: no bloquea el stop.
            if let Ok(path) = crate::backup::export_db(self, site).await {
                crate::dumplog::append(site, &path, "stop").ok();
            }
            crate::backup::rotate_dumps(site, 3).ok();
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
    pub(crate) async fn teardown_unused_shared(
        &self,
        stopped: &SiteConfig,
        all: &[SiteConfig],
    ) -> Result<()> {
        // ¿Algún OTRO proyecto sigue su container php corriendo?
        let mut active_dbs = Vec::new();
        let mut any_active = false;
        let mut any_minio = false;
        for s in all {
            if s.id == stopped.id {
                continue;
            }
            if self.is_running(&s.container_name()).await {
                any_active = true;
                if s.minio {
                    any_minio = true;
                }
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

        // MinIO: apagar si ningún activo lo usa.
        if !any_minio && self.is_running(MINIO).await {
            self.docker
                .stop_container(MINIO, Some(StopContainerOptions { t: 10 }))
                .await
                .ok();
        }

        // nginx + mailpit + adminer: si no queda proyecto activo, apagarlos también.
        if !any_active {
            for svc in [NGINX, MAILPIT, ADMINER] {
                if self.is_running(svc).await {
                    self.docker
                        .stop_container(svc, Some(StopContainerOptions { t: 10 }))
                        .await
                        .ok();
                }
            }
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
        self.exec_as(container, cmd, None).await
    }

    /// Como `exec` pero fijando el usuario del proceso. WP-CLI DEBE correr como
    /// `www-data` (no root: WP-CLI lo rechaza, y así los archivos generados —
    /// wp-config.php, etc.— quedan con el dueño del host vía el remapeo de uid).
    pub async fn exec_as(
        &self,
        container: &str,
        cmd: Vec<&str>,
        user: Option<&str>,
    ) -> Result<String> {
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    user: user.map(|u| u.to_string()),
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

        // Comprobar el código de salida: un comando que falla (p.ej. `wp config
        // create`) NO debe tragarse en silencio, o el proyecto queda a medias
        // (WP sin instalar) y `create_site` devuelve Ok engañosamente.
        let inspect = self.docker.inspect_exec(&exec.id).await?;
        if let Some(code) = inspect.exit_code {
            if code != 0 {
                let cmd = cmd.join(" ");
                return Err(anyhow!(
                    "`{cmd}` falló en {container} (código {code}): {}",
                    out.trim()
                ));
            }
        }
        Ok(out)
    }

    // Nota: importar un dump por stdin se hace con el CLI `docker exec -i` en
    // `migrate::import_dump`, NO por bollard. El `exec` con stdin adjunto de
    // bollard se colgaba con dumps grandes (su stream de salida no emite `None`
    // al terminar el proceso). Ver el comentario en `migrate::import_dump`.

    /// Ejecuta un comando y captura su **stdout** como bytes (stderr aparte, para
    /// el mensaje de error). Para volcados binarios/grandes como `mysqldump`.
    pub async fn exec_capture(&self, container: &str, cmd: Vec<&str>) -> Result<Vec<u8>> {
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
            .with_context(|| format!("create_exec (capture) en {container}"))?;

        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr = String::new();
        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec.id, None).await?
        {
            while let Some(chunk) = output.next().await {
                match chunk? {
                    LogOutput::StdOut { message } => stdout.extend_from_slice(&message),
                    LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message))
                    }
                    _ => {}
                }
            }
        }

        let inspect = self.docker.inspect_exec(&exec.id).await?;
        if let Some(code) = inspect.exit_code {
            if code != 0 {
                let cmd = cmd.join(" ");
                return Err(anyhow!(
                    "`{cmd}` falló en {container} (código {code}): {}",
                    stderr.trim()
                ));
            }
        }
        Ok(stdout)
    }
}

// ---------------------------------------------------------------------------
// helpers libres
// ---------------------------------------------------------------------------

/// Nombre del container DB compartido para un servicio (`panel-mysql-80`).
pub fn db_container_name(db: &DbService) -> String {
    format!(
        "{}-{}",
        db.db_type.service_prefix(),
        db.version.replace('.', "")
    )
}

/// Directorio del host donde persiste el datadir de un container DB compartido
/// (`config_dir/db-data/{container}`). Lo crea si falta. Bindeado a
/// `DbType::datadir()` para almacenamiento durable (sobrevive recreado / apagón).
pub fn db_data_dir(db: &DbService) -> Result<std::path::PathBuf> {
    let dir = crate::config::config_dir()?
        .join("db-data")
        .join(db_container_name(db));
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

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

/// Mapea puertos del container a `127.0.0.1:{host}` (solo loopback).
/// `specs` = `(host_port, "container_port/tcp")`.
fn host_port_map(specs: &[(u16, &str)]) -> HashMap<String, Option<Vec<PortBinding>>> {
    let mut map = HashMap::new();
    for (host, cport) in specs {
        map.insert(
            cport.to_string(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some(host.to_string()),
            }]),
        );
    }
    map
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
