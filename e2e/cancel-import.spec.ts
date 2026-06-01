import { test, expect } from '@playwright/test';

// Cancelar la importación de un proyecto pendiente borra su carpeta y lo quita
// de la lista (por si se importó el proyecto equivocado). Ver docs/TESTING.md §C.
test('cancelar importación elimina el proyecto pendiente', async ({ page }) => {
  page.on('dialog', (d) => d.accept());

  await page.goto('/');

  await expect(page.getByRole('link', { name: 'Sitio Importado' })).toBeVisible();

  const cancelar = page
    .locator('div')
    .filter({ hasText: 'Sitio Importado' })
    .getByRole('button', { name: 'Cancelar' });
  await cancelar.first().click();

  // El proyecto desaparece y, con él, su grupo "LocalWP".
  await expect(page.getByRole('link', { name: 'Sitio Importado' })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'LocalWP' })).toHaveCount(0);
});
