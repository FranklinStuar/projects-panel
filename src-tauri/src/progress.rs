//! Progreso de operaciones largas (migración, import) hacia el frontend.
//!
//! Emite líneas de texto en un único canal `op-log`; el frontend abre una
//! consola y las muestra en vivo. Operaciones como reconstruir la imagen php o
//! importar un dump de 100&nbsp;MB tardan, y sin esto la UI parece colgada.

use tauri::{AppHandle, Emitter};

/// Nombre del evento (espejo en el componente `OpConsole.svelte`).
pub const EVENT: &str = "op-log";

/// Emite una línea de progreso (best-effort: si falla el emit, se ignora).
pub fn log(app: &AppHandle, msg: impl AsRef<str>) {
    let _ = app.emit(EVENT, msg.as_ref().to_string());
}
