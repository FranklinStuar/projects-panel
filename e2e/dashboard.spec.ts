import { test, expect } from '@playwright/test';

// Master-detail de proyectos en modo mock. La lista izquierda muestra los grupos
// y proyectos; al seleccionar uno, su detalle se abre en el panel grande. Ver
// docs/TESTING.md §C.
test.describe('Proyectos', () => {
  test('lista proyectos agrupados; seleccionar abre el detalle con el host', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'Proyectos', level: 1 })).toBeVisible();

    // Grupos.
    await expect(page.getByRole('heading', { name: 'Cliente A' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'LocalWP' })).toBeVisible();

    // Proyectos en la lista.
    await expect(page.getByText('Tienda Demo', { exact: true })).toBeVisible();
    await expect(page.getByText('Blog Personal', { exact: true })).toBeVisible();

    // Al seleccionar un proyecto, el detalle muestra el host con puerto alterno
    // (el endpoint del fixture publica en :8443 para SSL).
    await page.getByText('Tienda Demo', { exact: true }).click();
    await expect(page.getByText('tienda-demo.test:8443')).toBeVisible();
  });

  test('encender un proyecto parado lo marca como corriendo', async ({ page }) => {
    await page.goto('/');

    // El botón power de la lista es un ícono con aria-label. Solo el proyecto
    // parado (Blog Personal) muestra "Encender".
    const encender = page.getByRole('button', { name: 'Encender', exact: true });
    await expect(encender).toHaveCount(1);
    await encender.click();

    // Tras encender ya no queda ningún "Encender" (Tienda Demo ya estaba arriba).
    await expect(page.getByRole('button', { name: 'Encender', exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Detener', exact: true })).toHaveCount(2);
  });
});
