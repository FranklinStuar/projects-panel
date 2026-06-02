import { test, expect, type Page } from '@playwright/test';

// Borrar un proyecto: el botón "Eliminar" abre un modal de confirmación con el
// NOMBRE del proyecto y un checkbox para borrar también la carpeta. Al confirmar,
// una consola muestra una ventana de gracia de 5 s con "Cancelar borrado" antes
// de proceder. Ver docs/TESTING.md §C.

// Fila (card) de un proyecto en el dashboard.
function card(page: Page, name: string) {
  return page
    .locator('div.justify-between')
    .filter({ has: page.getByRole('link', { name, exact: true }) });
}

// Abre el modal de confirmación de borrado para un proyecto.
async function openDialog(page: Page, name: string) {
  await page.goto('/');
  await card(page, name).getByRole('button', { name: 'Eliminar' }).click();
  return page.getByRole('dialog');
}

test('modal de confirmación muestra el nombre del proyecto y el checkbox', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');

  // Título con el nombre (no la URL) + checkbox de borrar carpeta sin marcar.
  await expect(dialog).toContainText('Eliminar «Blog Personal»');
  await expect(dialog.getByRole('checkbox')).not.toBeChecked();

  // Cancelar cierra el modal sin borrar nada.
  await dialog.getByRole('button', { name: 'Cancelar', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();
});

test('eliminar: consola con ventana de gracia y luego desaparece de la lista', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');
  await dialog.getByRole('button', { name: 'Eliminar', exact: true }).click();

  // Consola: mensaje de preparación + botón para cancelar el borrado.
  await expect(page.getByText('Preparando proceso de eliminación…')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Cancelar borrado' })).toBeVisible();

  // Tras la gracia procede; aparece el ✓ y se habilita Cerrar; el botón de
  // cancelar desaparece.
  await expect(page.getByText(/Proyecto eliminado/)).toBeVisible({ timeout: 15000 });
  await expect(page.getByRole('button', { name: 'Cancelar borrado' })).toHaveCount(0);

  const cerrar = page.getByRole('button', { name: 'Cerrar' });
  await expect(cerrar).toBeEnabled();
  await cerrar.click();

  await expect(page.getByRole('link', { name: 'Blog Personal' })).toHaveCount(0);
});

test('cancelar borrado durante la gracia no borra nada', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');
  await dialog.getByRole('button', { name: 'Eliminar', exact: true }).click();

  // Aborta dentro de la ventana de gracia.
  await page.getByRole('button', { name: 'Cancelar borrado' }).click();
  await expect(page.getByText(/Borrado cancelado/)).toBeVisible({ timeout: 15000 });

  // Cerrar la consola: el proyecto sigue en la lista.
  await page.getByRole('button', { name: 'Cerrar' }).click();
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toBeVisible();
});

test('marcar el checkbox borra también la carpeta', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');
  await dialog.getByRole('checkbox').check();
  await dialog.getByRole('button', { name: 'Eliminar', exact: true }).click();

  await expect(page.getByText(/datos y carpeta borrados/)).toBeVisible({ timeout: 15000 });
  await page.getByRole('button', { name: 'Cerrar' }).click();
  await expect(page.getByRole('link', { name: 'Blog Personal' })).toHaveCount(0);
});
