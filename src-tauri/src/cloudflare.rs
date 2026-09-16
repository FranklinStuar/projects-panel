//! Cloudflare Quick Tunnel: expone un proyecto a internet sin cuenta, dominio
//! ni login. `docker.rs` orquesta el container `cf-{id}`; aquí solo vive la
//! extracción de la URL pública que `cloudflared` imprime en su log al
//! arrancar. Ver `docs/CLOUDFLARE_TUNNEL_PLAN.md`.

/// Busca la URL pública (`https://algo.trycloudflare.com`) en el log del
/// container. `cloudflared` la imprime dentro de una caja dibujada con `|` y
/// espacios; separar por esos caracteres basta, sin necesitar regex.
///
/// ponytail: asume el formato de log actual de cloudflared; si Cloudflare
/// cambia el layout de la caja, ajustar el split.
pub fn extract_url(log: &str) -> Option<String> {
    log.split(|c: char| c.is_whitespace() || c == '|')
        .map(str::trim)
        .find(|tok| tok.starts_with("https://") && tok.ends_with(".trycloudflare.com"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_url_in_boxed_log() {
        let log = "2024-01-01T00:00:00Z INF |  https://random-word-1234.trycloudflare.com  |\n";
        assert_eq!(
            extract_url(log),
            Some("https://random-word-1234.trycloudflare.com".to_string())
        );
    }

    #[test]
    fn none_when_absent() {
        let log = "2024-01-01T00:00:00Z INF Starting tunnel\n";
        assert_eq!(extract_url(log), None);
    }
}
