//! Progreso de operaciones largas (migración, import) hacia el frontend.
//!
//! Emite líneas de texto en un único canal `op-log`; el frontend abre una
//! consola y las muestra en vivo. Operaciones como reconstruir la imagen php o
//! importar un dump de 100&nbsp;MB tardan, y sin esto la UI parece colgada.

use tauri::{AppHandle, Emitter, Runtime};

/// Nombre del evento (espejo en el componente `OpConsole.svelte`).
pub const EVENT: &str = "op-log";

/// Prefijo (carácter de control SOH) que marca una línea de progreso "viva". El
/// frontend (`OpConsole.svelte`) la **reemplaza en sitio** en vez de apilarla, así
/// un contador que tickea cada 2&nbsp;s (p. ej. el import) no inunda la consola.
/// Espejo del valor en `OpConsole.svelte`.
pub const PROGRESS_PREFIX: char = '\u{1}';

/// Emite una línea de progreso (best-effort: si falla el emit, se ignora).
///
/// Genérico sobre el runtime de Tauri para poder ejercitar los flujos que emiten
/// progreso (migración/import) en tests con `tauri::test::mock_app()`.
pub fn log<R: Runtime>(app: &AppHandle<R>, msg: impl AsRef<str>) {
    let _ = app.emit(EVENT, msg.as_ref().to_string());
}

/// Como [`log`] pero marca la línea como "viva": el frontend la sobreescribe en
/// sitio mientras lleguen más líneas vivas seguidas (contadores, barras), y la
/// fija al apilarse la siguiente línea normal.
pub fn log_progress<R: Runtime>(app: &AppHandle<R>, msg: impl AsRef<str>) {
    let _ = app.emit(EVENT, format!("{PROGRESS_PREFIX}{}", msg.as_ref()));
}
