import { test, expect, type Page } from '@playwright/test';

// Importar proyecto: el botón "Importar proyecto" (pie de la lista) abre un modal
// que lista las carpetas de ~/panel-wp/ desconectadas del panel. "Importar"
// re-conecta una y la deja como "pendiente de migración". Ver docs/TESTING.md §C.

async function openModal(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Importar proyecto' }).click();
  return page.getByRole('dialog');
}

test('el modal lista las carpetas desconectadas con su badge', async ({ page }) => {
  const dialog = await openModal(page);

  await expect(dialog).toContainText('Cliente Antiguo');
  await expect(dialog).toContainText('config conservada');
  await expect(dialog).toContainText('con dump');
  await expect(dialog).toContainText('sitio-copiado');
  await expect(dialog).toContainText('reconstruido');
});

test('importar deja el proyecto pendiente de migración en la lista', async ({ page }) => {
  const dialog = await openModal(page);

  // Importa la carpeta con config conservada.
  const row = dialog
    .locator('div.justify-between')
    .filter({ hasText: 'Cliente Antiguo' });
  await row.getByRole('button', { name: 'Importar' }).click();

  // La consola muestra el progreso y el ✓ final.
  await expect(page.getByText(/re-importado/)).toBeVisible({ timeout: 15000 });
  const cerrar = page.getByRole('button', { name: 'Cerrar' }).last();
  await expect(cerrar).toBeEnabled();
  await cerrar.click();

  // Cierra el modal de importación.
  await dialog.getByRole('button', { name: 'Cerrar' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // El proyecto aparece en la lista; al seleccionarlo, su detalle ofrece migrar.
  await page.getByText('Cliente Antiguo', { exact: true }).click();
  await expect(page.getByRole('button', { name: 'Migrar y encender' })).toBeVisible();
});
