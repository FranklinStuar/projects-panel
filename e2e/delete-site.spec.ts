import { test, expect, type Page } from '@playwright/test';

// Borrar un proyecto: el botón "Eliminar" pide dos confirmaciones (1: borrar
// datos; 2: ¿borrar también la carpeta?) y luego lo quita de la lista.
// Ver docs/TESTING.md §C.

// Fila (card) de un proyecto: el div que contiene su enlace y sus botones.
function card(page: Page, name: string) {
  return page
    .locator('div.justify-between')
    .filter({ has: page.getByRole('link', { name, exact: true }) });
}

test('eliminar proyecto: dos confirmaciones y desaparece de la lista', async ({ page }) => {
  const dialogs: string[] = [];
  page.on('dialog', (d) => {
    dialogs.push(d.message());
    d.accept(); // acepta ambas: borrar datos + borrar carpeta
  });

  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();

  await card(page, 'Blog Personal').getByRole('button', { name: 'Eliminar' }).click();

  // Aparecieron las dos confirmaciones (datos + carpeta) y el proyecto se fue.
  await expect.poll(() => dialogs.length).toBe(2);
  expect(dialogs[0]).toContain('todos los datos');
  expect(dialogs[1]).toContain('carpeta');
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toHaveCount(0);
});

test('conservar carpeta: cancelar la 2ª confirmación igual desconecta el proyecto', async ({
  page
}) => {
  let count = 0;
  page.on('dialog', (d) => {
    count += 1;
    // 1ª (borrar datos) → aceptar; 2ª (borrar carpeta) → cancelar = conservarla.
    if (count === 1) d.accept();
    else d.dismiss();
  });

  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();

  await card(page, 'Blog Personal').getByRole('button', { name: 'Eliminar' }).click();

  // El panel lo olvida (desconecta) aunque se conserve la carpeta.
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toHaveCount(0);
});

test('cancelar la 1ª confirmación no borra nada', async ({ page }) => {
  page.on('dialog', (d) => d.dismiss());

  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();

  await card(page, 'Blog Personal').getByRole('button', { name: 'Eliminar' }).click();

  // Sigue ahí.
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();
});
