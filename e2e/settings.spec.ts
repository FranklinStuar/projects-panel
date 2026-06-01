import { test, expect } from '@playwright/test';

// Pantalla de configuración: checklist del sistema, endpoint e import LocalWP.
// Ver docs/TESTING.md §C.
test.describe('Configuración', () => {
  test('muestra checklist, endpoint y sitios de LocalWP', async ({ page }) => {
    await page.goto('/settings');

    await expect(page.getByRole('heading', { name: 'Configuración', level: 1 })).toBeVisible();

    // Checklist de prerequisitos.
    for (const item of [
      'Docker',
      'Red panel-net',
      'dnsmasq *.test',
      'CA de mkcert',
      'Wrappers WP-CLI',
      'Plasmoid KDE'
    ]) {
      await expect(page.getByText(item, { exact: false }).first()).toBeVisible();
    }

    // Endpoint con puerto alterno (fixture 127.0.0.1:8080/8443).
    await expect(page.getByText('puerto alterno')).toBeVisible();

    // LocalWP: uno importable, otro ya importado.
    await expect(page.getByText('Proyecto Viejo')).toBeVisible();
    await expect(page.getByText('Ya importado')).toBeVisible();
  });

  test('importar un sitio de LocalWP abre la consola de progreso', async ({ page }) => {
    await page.goto('/settings');

    const importar = page
      .locator('div')
      .filter({ hasText: 'Proyecto Viejo' })
      .getByRole('button', { name: 'Importar' });
    await importar.first().click();

    // Consola con título de import + línea final de éxito.
    await expect(page.getByText('Importar desde LocalWP', { exact: false }).first()).toBeVisible();
    await expect(page.getByText('importado', { exact: false }).first()).toBeVisible({
      timeout: 10_000
    });
  });
});
