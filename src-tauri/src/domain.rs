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

// -- /etc/hosts para túneles de Cloudflare -----------------------------------
//
// Workaround de red, no de DNS del panel: cuando el DNS del sistema no
// resuelve bien un subdominio *.trycloudflare.com recién creado (ISPs que
// devuelven solo AAAA sin ruta IPv6 → ERR_NAME_NOT_RESOLVED pese a que el
// túnel está sano, visto en producción), se fija esa IP en `/etc/hosts` en
// vez de tocar la configuración de DNS del sistema. Una línea por proyecto,
// marcada para poder reemplazarla/borrarla sin tocar el resto del archivo.

const HOSTS_PATH: &str = "/etc/hosts";

fn tunnel_host_marker(site_id: &str) -> String {
    format!("# panel-tunnel:{site_id}")
}

/// Reemplaza (por proyecto) la entrada `/etc/hosts` que resuelve `hostname` a
/// `ip`. Requiere pkexec (diálogo gráfico). Valida el formato de ambos para
/// no interpolar nada raro en el script de shell.
pub fn set_tunnel_host(site_id: &str, hostname: &str, ip: &str) -> Result<()> {
    if !is_safe_hostname(hostname) {
        return Err(anyhow!("hostname con caracteres inesperados: {hostname}"));
    }
    if ip.parse::<std::net::Ipv4Addr>().is_err() {
        return Err(anyhow!("IP inválida: {ip}"));
    }
    let marker = tunnel_host_marker(site_id);
    let line = format!("{ip} {hostname} {marker}");
    run_pkexec_hosts_edit(&marker, Some(&line))
}

/// Borra la entrada `/etc/hosts` de un proyecto (al apagar/expirar el túnel).
/// Best-effort: se llama también cuando puede que nunca se haya llegado a
/// crear (p. ej. el DoH de `resolve_ipv4_via_doh` falló).
pub fn clear_tunnel_host(site_id: &str) -> Result<()> {
    run_pkexec_hosts_edit(&tunnel_host_marker(site_id), None)
}

fn is_safe_hostname(h: &str) -> bool {
    !h.is_empty()
        && h.len() < 256
        && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Quita cualquier línea con `marker` de `/etc/hosts` y, si `new_line` viene,
/// la agrega. Vía pkexec, sin dejar el archivo a medio escribir si falla.
fn run_pkexec_hosts_edit(marker: &str, new_line: Option<&str>) -> Result<()> {
    let append = new_line.map(|l| format!("echo '{l}' >> {HOSTS_PATH}.tmp && ")).unwrap_or_default();
    let script = format!(
        "grep -vF '{marker}' {HOSTS_PATH} > {HOSTS_PATH}.tmp || true; \
         {append}install -m 644 {HOSTS_PATH}.tmp {HOSTS_PATH} && rm -f {HOSTS_PATH}.tmp"
    );
    let status = std::process::Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(&script)
        .status()
        .map_err(|err| anyhow!("no se pudo ejecutar pkexec: {err}"))?;
    if !status.success() {
        return Err(anyhow!("pkexec no pudo editar /etc/hosts"));
    }
    Ok(())
}
