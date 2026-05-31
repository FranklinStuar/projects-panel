import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    // Tauri sirve archivos estáticos, no SSR. adapter-static + fallback SPA.
    adapter: adapter({
      fallback: 'index.html'
    })
  }
};

export default config;
