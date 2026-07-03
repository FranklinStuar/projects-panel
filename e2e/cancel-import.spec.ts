import { test, expect } from '@playwright/test';

// Cancelar la importación de un proyecto pendiente borra su carpeta y lo quita
// de la lista (por si se importó el proyecto equivocado). El botón "Cancelar"
// vive en la cabecera del detalle del proyecto pendiente. Ver docs/TESTING.md §C.
test('cancelar importación elimina el proyecto pendiente', async ({ page }) => {
  page.on('dialog', (d) => d.accept());

  await page.goto('/');

  // Selecciona el proyecto pendiente → su detalle muestra "Cancelar".
  await expect(page.getByText('Sitio Importado', { exact: true })).toBeVisible();
  await page.getByText('Sitio Importado', { exact: true }).click();

  await page.getByRole('button', { name: 'Cancelar', exact: true }).click();

  // El proyecto desaparece y, con él, su grupo "LocalWP".
  await expect(page.getByText('Sitio Importado', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'LocalWP' })).toHaveCount(0);
});
