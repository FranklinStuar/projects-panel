import { defineConfig, devices } from '@playwright/test';

// E2E de la GUI contra el SPA en modo mock (VITE_MOCK_IPC=1): sin backend Tauri
// ni Docker. Ver docs/TESTING.md §B.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://localhost:1420',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure'
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  // Levanta el SPA mock automáticamente y lo reutiliza si ya está corriendo.
  webServer: {
    command: 'pnpm dev:mock',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000
  }
});
