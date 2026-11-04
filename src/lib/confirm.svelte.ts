// Confirmación modal propia, sustituto de `window.confirm()`.
//
// En macOS el WKWebView de Tauri (wry) no implementa los paneles JS
// (`runJavaScriptConfirmPanel…`), así que `confirm()` devuelve `false` en
// silencio y la acción nunca arranca. Este store + `ConfirmDialog.svelte`
// (montado una vez en `+layout.svelte`) da el mismo flujo en todas las
// plataformas: `if (!(await askConfirm('¿…?'))) return;`.

export type ConfirmRequest = {
  message: string;
  confirmLabel: string;
  danger: boolean;
  resolve: (ok: boolean) => void;
};

export const confirmState = $state<{ current: ConfirmRequest | null }>({ current: null });

export function askConfirm(
  message: string,
  opts: { confirmLabel?: string; danger?: boolean } = {}
): Promise<boolean> {
  // Una petición previa sin responder se da por cancelada.
  confirmState.current?.resolve(false);
  return new Promise((resolve) => {
    confirmState.current = {
      message,
      confirmLabel: opts.confirmLabel ?? 'Aceptar',
      danger: opts.danger ?? false,
      resolve
    };
  });
}

export function answerConfirm(ok: boolean) {
  const req = confirmState.current;
  confirmState.current = null;
  req?.resolve(ok);
}
