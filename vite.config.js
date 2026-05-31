import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [sveltekit()],

  // Tauri espera un puerto fijo y falla si no está disponible
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421
        }
      : undefined,
    watch: {
      // tauri vigila su propio código; ignorar src-tauri evita recargas dobles
      ignored: ['**/src-tauri/**']
    }
  }
});
