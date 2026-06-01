// Tauri sirve archivos estáticos como SPA: sin SSR. El routing (incluidas rutas
// dinámicas como /site/[id]) lo resuelve el cliente sobre el fallback index.html.
export const ssr = false;
export const prerender = false;

// Modo mock para testing de la GUI (VITE_MOCK_IPC=1): instala la capa de IPC
// simulada ANTES de renderizar, así los onMount de las páginas ya encuentran
// `window.__TAURI_INTERNALS__`. En producción/Tauri real no se incluye.
export const load = async () => {
  if (import.meta.env.VITE_MOCK_IPC === '1') {
    await import('$lib/dev/mock-ipc');
  }
  return {};
};
