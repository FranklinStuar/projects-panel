//! Resolución de dominios `.test` vía dnsmasq wildcard.
//!
//! En vez de editar `/etc/hosts` por proyecto (que requiere polkit cada vez),
//! se configura UNA sola regla wildcard `address=/test/127.0.0.1`. Eso resuelve
//! todos los `*.test` presentes y futuros sin tocar nada por proyecto.
//!
//! La instalación con permisos (copiar a `/etc/NetworkManager/dnsmasq.d/`) es un
//! paso de "primera configuración"; aquí se genera el snippet y se comprueba si
//! el sistema ya resuelve.
//!
//! macOS: dnsmasq de Homebrew (`$(brew --prefix)/etc/dnsmasq.d/`) + el resolver
//! por dominio `/etc/resolver/test` → `nameserver 127.0.0.1` (ver first-run.sh).

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
#[cfg(not(target_os = "macos"))]
pub fn install_target() -> &'static str {
    "/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf"
}

/// Prefijo de Homebrew (Apple Silicon o Intel).
#[cfg(target_os = "macos")]
pub fn brew_prefix() -> &'static str {
    if std::path::Path::new("/opt/homebrew").exists() {
        "/opt/homebrew"
    } else {
        "/usr/local"
    }
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
#[cfg(not(target_os = "macos"))]
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

/// macOS: reescribe el snippet en el dnsmasq de Homebrew (dir del usuario, sin
/// root) y reinicia el servicio, que sí corre como root (puerto 53) → diálogo
/// de administrador vía `osascript`. Idempotente.
#[cfg(target_os = "macos")]
pub fn install_wildcard(ip: &str) -> Result<()> {
    let prefix = brew_prefix();
    let dir = std::path::Path::new(prefix).join("etc").join("dnsmasq.d");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("wordpress-panel.conf"), wildcard_rule(ip))?;
    // `prefix` es una ruta fija conocida: sin comillas ni metacaracteres.
    let script = format!(
        "do shell script \"{prefix}/bin/brew services restart dnsmasq\" with administrator privileges"
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(|err| anyhow!("no se pudo ejecutar osascript: {err}"))?;
    if !status.success() {
        return Err(anyhow!(
            "no se pudo reiniciar dnsmasq (brew services) para apuntar *.test a {ip}"
        ));
    }
    Ok(())
}
