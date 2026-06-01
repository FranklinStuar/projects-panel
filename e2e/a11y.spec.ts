import { test, expect } from '@playwright/test';

// Accesibilidad básica (sin dependencias extra): cada ruta debe tener una región
// principal, navegación con enlaces nombrados y un único h1. Ver docs/TESTING.md §C.
const rutas = ['/', '/settings', '/site/new', '/domains', '/services'];

for (const ruta of rutas) {
  test(`a11y básica en ${ruta}`, async ({ page }) => {
    await page.goto(ruta);

    // Landmark principal + navegación lateral siempre presentes (layout).
    await expect(page.getByRole('main')).toBeVisible();
    await expect(page.getByRole('navigation')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Proyectos', exact: true })).toBeVisible();

    // Exactamente un encabezado de nivel 1.
    await expect(page.getByRole('heading', { level: 1 })).toHaveCount(1);

    // Ningún botón sin nombre accesible (texto vacío).
    const sinNombre = await page.getByRole('button').evaluateAll((els) =>
      els.filter((e) => !(e.textContent ?? '').trim() && !e.getAttribute('aria-label')).length
    );
    expect(sinNombre, 'no debe haber botones sin nombre accesible').toBe(0);
  });
}
