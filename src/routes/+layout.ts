// Tauri sirve archivos estáticos como SPA: sin SSR. El routing (incluidas rutas
// dinámicas como /site/[id]) lo resuelve el cliente sobre el fallback index.html.
export const ssr = false;
export const prerender = false;
