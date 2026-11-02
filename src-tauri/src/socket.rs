//! Puente CLI/MCP ↔ panel en macOS (allí no hay D-Bus).
//!
//! Socket Unix en `config_dir/panel.sock`. Protocolo: una petición por conexión,
//! una línea JSON `{"method":"CreateWorktree","args":["…", …]}` con los mismos
//! nombres y argumentos (en texto) que la interfaz D-Bus. La respuesta es el
//! resultado del método tal cual (ver [`crate::dbus::Manager::call`]) y se cierra
//! la conexión. Errores de protocolo: `{"ok":false,"error":…}`.
//!
//! Solo vive mientras el panel está abierto, igual que el servicio D-Bus.

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixListener;

use crate::dbus::Manager;

#[derive(Deserialize)]
struct Request {
    method: String,
    #[serde(default)]
    args: Vec<String>,
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(crate::config::config_dir()?.join("panel.sock"))
}

/// Escucha en el socket hasta que el panel se cierre.
pub async fn serve(app: tauri::AppHandle) -> Result<()> {
    let path = socket_path()?;
    // Un socket huérfano de una ejecución anterior impide el bind.
    std::fs::remove_file(&path).ok();
    let listener = UnixListener::bind(&path).with_context(|| format!("bind {path:?}"))?;
    // Solo el usuario: el socket permite operar sus proyectos.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();

    let manager = Arc::new(Manager { app });
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(c) => c,
            Err(err) => {
                eprintln!("socket: accept falló: {err}");
                continue;
            }
        };
        let manager = manager.clone();
        tokio::spawn(async move {
            let (rd, mut wr) = stream.into_split();
            let mut line = String::new();
            if BufReader::new(rd).read_line(&mut line).await.is_err() {
                return;
            }
            let out = match serde_json::from_str::<Request>(&line) {
                Ok(req) => manager
                    .call(&req.method, &req.args)
                    .await
                    .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e }).to_string()),
                Err(e) => serde_json::json!({ "ok": false, "error": format!("petición inválida: {e}") })
                    .to_string(),
            };
            let _ = wr.write_all(out.as_bytes()).await;
            let _ = wr.write_all(b"\n").await;
            let _ = wr.shutdown().await;
        });
    }
}
