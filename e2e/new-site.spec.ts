import { test, expect } from '@playwright/test';

// Alta de un proyecto nuevo: llenado del formulario y vuelta al dashboard con el
// proyecto creado. Ver docs/TESTING.md §C.
test('crear un proyecto nuevo y verlo en el dashboard', async ({ page }) => {
  await page.goto('/site/new');

  await expect(page.getByRole('heading', { name: 'Nuevo proyecto' })).toBeVisible();

  // Nombre → dominio se autocompleta.
  await page.getByPlaceholder('my-project', { exact: true }).fill('Mi Nuevo Sitio');
  await expect(page.getByPlaceholder('my-project.test')).toHaveValue('mi-nuevo-sitio.test');

  // PHP y datos de admin obligatorios.
  await page.getByRole('combobox').first().waitFor(); // versión WP ya cargada (mock)
  await page.locator('input[type="password"]').fill('secret123');
  await page.locator('input[type="email"]').fill('admin@demo.test');

  await page.getByRole('button', { name: 'Crear proyecto' }).click();

  // El mock crea el sitio y la app navega al dashboard.
  await expect(page).toHaveURL('http://localhost:1420/');
  await expect(page.getByRole('link', { name: 'Mi Nuevo Sitio' })).toBeVisible();
});
