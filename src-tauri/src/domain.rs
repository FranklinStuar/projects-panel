//! Resolución de dominios `.test` vía dnsmasq wildcard.
//!
//! En vez de editar `/etc/hosts` por proyecto (que requiere polkit cada vez),
//! se configura UNA sola regla wildcard `address=/test/127.0.0.1`. Eso resuelve
//! todos los `*.test` presentes y futuros sin tocar nada por proyecto.
//!
//! La instalación con permisos (copiar a `/etc/NetworkManager/dnsmasq.d/`) es un
//! paso de "primera configuración"; aquí se genera el snippet y se comprueba si
//! el sistema ya resuelve.

use anyhow::Result;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

pub const WILDCARD_RULE: &str = "address=/test/127.0.0.1\n";

/// Ruta donde el panel deja el snippet listo para instalar.
pub fn snippet_path() -> Result<PathBuf> {
    Ok(crate::config::config_dir()?.join("dnsmasq-panel.conf"))
}

/// Destino recomendado en Manjaro/NetworkManager (requiere root al copiar).
#[allow(dead_code)] // usado por la pantalla de primera configuración (Fase 4)
pub fn install_target() -> &'static str {
    "/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf"
}

/// ¿El sistema ya resuelve los `.test` a loopback?
pub fn wildcard_active() -> bool {
    match ("panel-probe.test", 0u16).to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|a| a.ip().is_loopback()),
        Err(_) => false,
    }
}

/// Deja el snippet escrito en la config del panel. No instala (sin root aquí).
pub fn ensure_wildcard() -> Result<()> {
    if wildcard_active() {
        return Ok(());
    }
    let path = snippet_path()?;
    std::fs::write(&path, WILDCARD_RULE)?;
    Ok(())
}
