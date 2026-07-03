import { test, expect } from '@playwright/test';

// Migrar un proyecto pendiente: se selecciona en la lista y, desde la cabecera
// de su detalle, "Migrar y encender" abre la consola de progreso (OpConsole) que
// se puebla en vivo con líneas `op-log`. Ver docs/TESTING.md §C.
test('migrar muestra la consola de progreso y enciende el sitio', async ({ page }) => {
  // El flujo pide confirm() antes de migrar.
  page.on('dialog', (d) => d.accept());

  await page.goto('/');

  // Selecciona el proyecto pendiente → su detalle muestra "Migrar y encender".
  await page.getByText('Sitio Importado', { exact: true }).click();
  await page.getByRole('button', { name: 'Migrar y encender' }).click();

  // La consola aparece con título "Migración".
  const consola = page.getByText('Migración', { exact: false });
  await expect(consola).toBeVisible();

  // Mientras corre, "Cerrar" está deshabilitado.
  const cerrar = page.getByRole('button', { name: 'Cerrar' });
  await expect(cerrar).toBeDisabled();

  // Llegan líneas de progreso en vivo.
  await expect(page.getByText('Importando base de datos', { exact: false })).toBeVisible();

  // Al terminar: "Cerrar" se habilita y aparece la línea final.
  await expect(page.getByText('migrado y encendido', { exact: false })).toBeVisible({
    timeout: 10_000
  });
  await expect(cerrar).toBeEnabled();

  await cerrar.click();
  await expect(page.getByText('Migración', { exact: false })).toBeHidden();
});
