//! Resolución de dominios `.test` vía dnsmasq wildcard.
//!
//! En vez de editar `/etc/hosts` por proyecto (que requiere polkit cada vez),
//! se configura UNA sola regla wildcard `address=/test/127.0.0.1`. Eso resuelve
//! todos los `*.test` presentes y futuros sin tocar nada por proyecto.
//!
//! La instalación con permisos (copiar a `/etc/NetworkManager/dnsmasq.d/`) es un
//! paso de "primera configuración"; aquí se genera el snippet y se comprueba si
//! el sistema ya resuelve.

use anyhow::{anyhow, Result};
use std::net::ToSocketAddrs;
use std::path::PathBuf;

/// IP por defecto a la que resuelven los `*.test`.
pub const DEFAULT_IP: &str = "127.0.0.1";

/// Regla dnsmasq wildcard para una IP loopback concreta.
pub fn wildcard_rule(ip: &str) -> String {
    format!("address=/test/{ip}\n")
}

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

/// ¿Los `.test` resuelven exactamente a `ip`? (necesario al usar IP alterna).
pub fn resolves_to(ip: &str) -> bool {
    match ("panel-probe.test", 0u16).to_socket_addrs() {
        Ok(addrs) => addrs.map(|a| a.ip().to_string()).any(|s| s == ip),
        Err(_) => false,
    }
}

/// Deja el snippet escrito en la config del panel. No instala (sin root aquí).
pub fn ensure_wildcard() -> Result<()> {
    if wildcard_active() {
        return Ok(());
    }
    let path = snippet_path()?;
    std::fs::write(&path, wildcard_rule(DEFAULT_IP))?;
    Ok(())
}

/// Instala/reescribe la regla wildcard apuntando a `ip` y recarga NetworkManager.
/// Requiere privilegios → usa `pkexec` (diálogo gráfico). Idempotente.
pub fn install_wildcard(ip: &str) -> Result<()> {
    let target = install_target();
    let rule = wildcard_rule(ip);
    // `ip` viene de IPs loopback generadas por el panel (127.0.0.x): sin metacaracteres.
    let script = format!(
        "install -d /etc/NetworkManager/dnsmasq.d && \
         printf '%s' '{rule}' > '{target}' && \
         (systemctl reload NetworkManager || systemctl restart NetworkManager)"
    );
    let status = std::process::Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(&script)
        .status()
        .map_err(|err| anyhow!("no se pudo ejecutar pkexec: {err}"))?;
    if !status.success() {
        return Err(anyhow!(
            "pkexec no pudo instalar la regla dnsmasq para {ip}"
        ));
    }
    Ok(())
}
