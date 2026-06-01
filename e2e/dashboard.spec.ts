import { test, expect } from '@playwright/test';

// Dashboard de proyectos en modo mock. Ver docs/TESTING.md §C.
test.describe('Dashboard', () => {
  test('lista proyectos agrupados con estado y host', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'Proyectos', level: 1 })).toBeVisible();

    // Grupos.
    await expect(page.getByRole('heading', { name: 'Cliente A' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'LocalWP' })).toBeVisible();

    // Proyectos.
    await expect(page.getByRole('link', { name: 'Tienda Demo' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();

    // El endpoint usa puerto alterno → la URL muestra :8443 en el sitio SSL.
    await expect(page.getByText('tienda-demo.test:8443')).toBeVisible();
  });

  test('encender un proyecto parado lo marca como corriendo', async ({ page }) => {
    await page.goto('/');

    // Solo el proyecto parado muestra "Encender" (exact: evita casar con el
    // botón "Migrar y encender" del proyecto pendiente).
    const encender = page.getByRole('button', { name: 'Encender', exact: true });
    await expect(encender).toHaveCount(1);
    await encender.click();

    // Tras encender ya no queda ningún "Encender" (Tienda Demo ya estaba arriba).
    await expect(page.getByRole('button', { name: 'Encender', exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Detener', exact: true })).toHaveCount(2);
  });
});
