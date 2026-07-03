import { test, expect, type Page } from '@playwright/test';

// Borrar un proyecto: se selecciona en la lista, y desde el menú "···" de la
// cabecera del detalle, "Eliminar" abre un modal de confirmación con el NOMBRE
// del proyecto y un checkbox para borrar también la carpeta. Al confirmar, una
// consola muestra una ventana de gracia de 5 s con "Cancelar borrado" antes de
// proceder. Ver docs/TESTING.md §C.

// Selecciona un proyecto en la lista izquierda (abre su detalle embebido).
async function select(page: Page, name: string) {
  await page.goto('/');
  await page.getByText(name, { exact: true }).click();
}

// Abre el modal de confirmación de borrado desde el menú "···" del detalle.
async function openDialog(page: Page, name: string) {
  await select(page, name);
  await page.getByRole('button', { name: 'Más acciones' }).click();
  await page.getByRole('button', { name: 'Eliminar' }).click();
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
  // Sigue en la lista (acotado: el nombre también aparece en el h1 del detalle).
  await expect(page.getByTestId('project-list').getByText('Blog Personal', { exact: true })).toBeVisible();
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

  await expect(page.getByTestId('project-list').getByText('Blog Personal', { exact: true })).toHaveCount(0);
});

test('cancelar borrado durante la gracia no borra nada', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');
  await dialog.getByRole('button', { name: 'Eliminar', exact: true }).click();

  // Aborta dentro de la ventana de gracia.
  await page.getByRole('button', { name: 'Cancelar borrado' }).click();
  await expect(page.getByText(/Borrado cancelado/)).toBeVisible({ timeout: 15000 });

  // Cerrar la consola: el proyecto sigue en la lista.
  await page.getByRole('button', { name: 'Cerrar' }).click();
  await expect(page.getByTestId('project-list').getByText('Blog Personal', { exact: true })).toBeVisible();
});

test('marcar el checkbox borra también la carpeta', async ({ page }) => {
  const dialog = await openDialog(page, 'Blog Personal');
  await dialog.getByRole('checkbox').check();
  await dialog.getByRole('button', { name: 'Eliminar', exact: true }).click();

  await expect(page.getByText(/datos y carpeta borrados/)).toBeVisible({ timeout: 15000 });
  await page.getByRole('button', { name: 'Cerrar' }).click();
  await expect(page.getByTestId('project-list').getByText('Blog Personal', { exact: true })).toHaveCount(0);
});
