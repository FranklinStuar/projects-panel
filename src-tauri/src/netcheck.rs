//! Detección de qué ocupa los puertos del host, leyendo `/proc/net/tcp{,6}`.
//!
//! El panel publica el reverse-proxy compartido (`panel-nginx`) en el host. Si
//! otra herramienta (típicamente LocalWP) ya escucha en 80/443, el `bind` de
//! Docker falla con un error 500 opaco. Aquí decidimos *con antelación* un punto
//! de publicación libre (ver `docker::select_endpoint`).
//!
//! Detalle clave del kernel: un listener en `0.0.0.0:80` (wildcard) bloquea
//! CUALQUIER `127.0.0.x:80`. Por eso distinguimos tres estados por puerto:
//! libre, wildcard, o atado a IPs concretas. Solo en el caso "IPs concretas"
//! sirve la treta de usar otra IP loopback manteniendo el puerto 80.
//!
//! macOS no tiene `/proc`: allí se usa `lsof` (ver `mod mac`), con un `bind` de
//! prueba como respaldo para listeners de otros usuarios (root) que `lsof` no ve.

use std::net::Ipv4Addr;

const ST_LISTEN: &str = "0A";

/// Estado de un puerto del host (combinando IPv4 + IPv6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortStatus {
    /// Nadie escucha → cualquier IP loopback puede usar el puerto.
    Free,
    /// Hay un listener wildcard (`0.0.0.0` / `::`) → bloquea todas las IPs.
    Wildcard,
    /// Solo IPs concretas escuchan; otra IP loopback queda libre.
    Specific(Vec<Ipv4Addr>),
}

impl PortStatus {
    /// ¿Está libre el puerto para esta IP concreta?
    pub fn free_for(&self, ip: Ipv4Addr) -> bool {
        match self {
            PortStatus::Free => true,
            PortStatus::Wildcard => false,
            PortStatus::Specific(ips) => !ips.contains(&ip),
        }
    }

    pub fn is_wildcard(&self) -> bool {
        matches!(self, PortStatus::Wildcard)
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
/// Decodifica el `local_address` IPv4 hex de `/proc/net/tcp` (`0100007F` = 127.0.0.1).
fn parse_v4(hex: &str) -> Option<Ipv4Addr> {
    let raw = u32::from_str_radix(hex, 16).ok()?;
    let b = raw.to_le_bytes(); // los bytes se almacenan en little-endian
    Some(Ipv4Addr::new(b[0], b[1], b[2], b[3]))
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
/// `local_address` = `ADDR:PORT` en hex. Devuelve `(addr_hex, port)` si LISTEN
/// y el puerto coincide.
fn listen_addr<'a>(line: &'a str, port: u16) -> Option<&'a str> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 || cols[3] != ST_LISTEN {
        return None;
    }
    let (addr_hex, port_hex) = cols[1].split_once(':')?;
    let p = u16::from_str_radix(port_hex, 16).ok()?;
    (p == port).then_some(addr_hex)
}

#[cfg(not(target_os = "macos"))]
fn scan_v4(path: &str, port: u16, wildcard: &mut bool, specifics: &mut Vec<Ipv4Addr>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for line in content.lines().skip(1) {
        let Some(addr_hex) = listen_addr(line, port) else {
            continue;
        };
        match parse_v4(addr_hex) {
            Some(ip) if ip.is_unspecified() => *wildcard = true, // 0.0.0.0
            Some(ip) => specifics.push(ip),
            None => {}
        }
    }
}

/// En IPv6 solo nos interesa detectar el wildcard `::` (todo ceros), que en la
/// práctica (sin IPV6_V6ONLY) también cubre IPv4. Listeners en `::1` no chocan
/// con nuestros binds IPv4.
#[cfg(not(target_os = "macos"))]
fn scan_v6(path: &str, port: u16, wildcard: &mut bool) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for line in content.lines().skip(1) {
        let Some(addr_hex) = listen_addr(line, port) else {
            continue;
        };
        if addr_hex.chars().all(|c| c == '0') {
            *wildcard = true;
        }
    }
}

/// Estado del puerto en el host, combinando IPv4 + IPv6.
#[cfg(target_os = "macos")]
pub fn port_status(port: u16) -> PortStatus {
    mac::port_status(port)
}

/// Estado del puerto en el host, combinando IPv4 + IPv6.
#[cfg(not(target_os = "macos"))]
pub fn port_status(port: u16) -> PortStatus {
    let mut wildcard = false;
    let mut specifics: Vec<Ipv4Addr> = Vec::new();
    scan_v4("/proc/net/tcp", port, &mut wildcard, &mut specifics);
    scan_v6("/proc/net/tcp6", port, &mut wildcard);

    if wildcard {
        PortStatus::Wildcard
    } else if specifics.is_empty() {
        PortStatus::Free
    } else {
        specifics.sort();
        specifics.dedup();
        PortStatus::Specific(specifics)
    }
}

/// Primera IP loopback `127.0.0.x` (x ≥ 2) libre en AMBOS puertos. Solo tiene
/// sentido cuando ningún puerto está en estado wildcard.
#[allow(dead_code)] // el panel cede 80/443 a LocalWP y usa puertos alt (ver autoselect_endpoint)
pub fn pick_loopback_ip(http: u16, https: u16) -> Option<Ipv4Addr> {
    let sh = port_status(http);
    let ss = port_status(https);
    if sh.is_wildcard() || ss.is_wildcard() {
        return None;
    }
    (2u8..=254).find_map(|x| {
        let ip = Ipv4Addr::new(127, 0, 0, x);
        (sh.free_for(ip) && ss.free_for(ip)).then_some(ip)
    })
}

/// Primer puerto ≥ `start` libre en `127.0.0.1` (para el fallback de puerto).
pub fn pick_alt_port(start: u16) -> Option<u16> {
    let lo = Ipv4Addr::new(127, 0, 0, 1);
    (start..=65000).find(|&p| port_status(p).free_for(lo))
}

/// Mejor esfuerzo: nombre del proceso que escucha en `port` (para mensajes de
/// error legibles). Requiere poder leer `/proc/<pid>/fd` (mismo usuario). Si no
/// se puede determinar, devuelve `None` y el llamador usa solo IP:puerto.
#[cfg(target_os = "macos")]
pub fn holder_name(port: u16) -> Option<String> {
    mac::holder_name(port)
}

#[cfg(not(target_os = "macos"))]
pub fn holder_name(port: u16) -> Option<String> {
    let inodes = listen_inodes(port);
    if inodes.is_empty() {
        return None;
    }
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let pid_dir = entry.path();
        let Ok(fd_dir) = std::fs::read_dir(pid_dir.join("fd")) else {
            continue;
        };
        for fd in fd_dir.flatten() {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                let t = target.to_string_lossy();
                if let Some(ino) = t.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
                    if inodes.iter().any(|i| i == ino) {
                        if let Ok(comm) = std::fs::read_to_string(pid_dir.join("comm")) {
                            return Some(comm.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_little_endian() {
        // /proc/net/tcp guarda la IP en little-endian: 0100007F = 127.0.0.1.
        assert_eq!(parse_v4("0100007F"), Some(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(parse_v4("00000000"), Some(Ipv4Addr::new(0, 0, 0, 0)));
        assert_eq!(parse_v4("0200007F"), Some(Ipv4Addr::new(127, 0, 0, 2)));
    }

    #[test]
    fn listen_addr_matches_port_and_state() {
        // sl local rem st ... (st 0A = LISTEN), puerto 80 = 0x0050.
        let line = "   0: 0100007F:0050 00000000:0000 0A 00000000:00000000 00:0 0 0 12345 1";
        assert_eq!(listen_addr(line, 80), Some("0100007F"));
        assert_eq!(listen_addr(line, 443), None);
        // estado distinto de LISTEN se ignora.
        let estab = line.replacen(" 0A ", " 01 ", 1);
        assert_eq!(listen_addr(&estab, 80), None);
    }

    #[test]
    fn free_for_semantics() {
        let lo1 = Ipv4Addr::new(127, 0, 0, 1);
        let lo2 = Ipv4Addr::new(127, 0, 0, 2);
        assert!(PortStatus::Free.free_for(lo1));
        assert!(!PortStatus::Wildcard.free_for(lo2)); // 0.0.0.0 bloquea todo
        let s = PortStatus::Specific(vec![lo1]);
        assert!(!s.free_for(lo1));
        assert!(s.free_for(lo2)); // listener en .1 no bloquea .2
    }
}

/// Inodos (col 9 de `/proc/net/tcp{,6}`) de los sockets en LISTEN en `port`.
#[cfg(not(target_os = "macos"))]
fn listen_inodes(port: u16) -> Vec<String> {
    let mut out = Vec::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines().skip(1) {
            if listen_addr(line, port).is_none() {
                continue;
            }
            if let Some(inode) = line.split_whitespace().nth(9) {
                out.push(inode.to_string());
            }
        }
    }
    out
}

/// Implementación macOS vía `lsof -F` (salida por campos: `p<pid>`, `c<cmd>`,
/// `n<addr:port>`). `lsof` sin root solo ve los sockets del usuario; por eso, si
/// no ve nada, se confirma con un `bind` de prueba en `127.0.0.1:port`.
#[cfg(target_os = "macos")]
mod mac {
    use super::PortStatus;
    use std::net::{Ipv4Addr, TcpListener};

    fn lsof(port: u16) -> String {
        std::process::Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fcn"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    /// `n*:80` / `n[::]:80` = wildcard; `n127.0.0.1:80` = IP concreta;
    /// `n[::1]:80` (loopback v6) no choca con binds IPv4.
    pub(super) fn parse(out: &str, port: u16) -> (bool, Vec<Ipv4Addr>) {
        let suffix = format!(":{port}");
        let mut wildcard = false;
        let mut specifics = Vec::new();
        for line in out.lines() {
            let Some(addr) = line.strip_prefix('n').and_then(|a| a.strip_suffix(&suffix)) else {
                continue;
            };
            match addr {
                "*" | "[::]" | "0.0.0.0" => wildcard = true,
                a => {
                    if let Ok(ip) = a.parse::<Ipv4Addr>() {
                        specifics.push(ip);
                    }
                }
            }
        }
        (wildcard, specifics)
    }

    pub fn port_status(port: u16) -> PortStatus {
        let (wildcard, mut specifics) = parse(&lsof(port), port);
        if wildcard {
            return PortStatus::Wildcard;
        }
        if specifics.is_empty() {
            // Listener invisible para lsof (otro usuario): el bind lo delata.
            // Sin saber en qué IP escucha, se trata como wildcard (conservador).
            return match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
                Ok(_) => PortStatus::Free,
                Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => PortStatus::Wildcard,
                Err(_) => PortStatus::Free,
            };
        }
        specifics.sort();
        specifics.dedup();
        PortStatus::Specific(specifics)
    }

    pub fn holder_name(port: u16) -> Option<String> {
        lsof(port)
            .lines()
            .find_map(|l| l.strip_prefix('c').map(str::to_string))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parse_lsof_fields() {
            let out = "p10\ncnginx\nf5\nn127.0.0.1:80\nf6\nn[::1]:80\n";
            assert_eq!(parse(out, 80), (false, vec![Ipv4Addr::LOCALHOST]));
            assert_eq!(parse("p1\ncx\nn*:80\n", 80), (true, vec![]));
            assert_eq!(parse("p1\ncx\nn[::]:443\n", 443), (true, vec![]));
            // otro puerto que termina igual no cuenta (8080 ≠ 80).
            assert_eq!(parse("p1\ncx\nn*:8080\n", 80), (false, vec![]));
        }
    }
}
