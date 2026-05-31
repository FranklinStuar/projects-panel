//! Streaming de logs del container de un proyecto hacia el frontend vía eventos.
//!
//! `spawn_stream` arranca una tarea que sigue (`follow`) los logs del container
//! `wp-{id}` y emite cada línea como evento `log:{id}`. El frontend se suscribe
//! con `listen()`. La tarea se cancela con `JoinHandle::abort()` (ver lib.rs).

use anyhow::Result;
use bollard::container::LogsOptions;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

use crate::docker::DockerManager;

pub fn event_name(id: &str) -> String {
    format!("log:{id}")
}

/// Lanza la tarea de streaming. Emite las últimas 200 líneas y luego sigue en vivo.
pub fn spawn_stream(app: AppHandle, id: String) -> Result<JoinHandle<()>> {
    let docker = DockerManager::connect()?;
    let cname = format!("wp-{id}");
    let ev = event_name(&id);

    let handle = tokio::spawn(async move {
        let opts = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            tail: "200".to_string(),
            ..Default::default()
        };
        let mut stream = docker.raw().logs(&cname, Some(opts));
        while let Some(item) = stream.next().await {
            match item {
                Ok(out) => {
                    let line = out.to_string();
                    if !line.is_empty() {
                        let _ = app.emit(&ev, line);
                    }
                }
                Err(_) => break, // container parado o error → fin del stream
            }
        }
    });
    Ok(handle)
}
