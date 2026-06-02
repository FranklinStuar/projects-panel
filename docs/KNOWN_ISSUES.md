# Problemas conocidos (a revisar al final de todas las fases)

## Import LocalWP: la DB requiere el dump en disco

El importador (`localwp.rs`) copia `app/public` y el dump `app/sql/local.sql` que
LocalWP deja en disco. La DB **no** se extrae del MySQL de LocalWP en vivo: si ese
`local.sql` no existe (o está desactualizado), el sitio se migra con la base de
datos vacía. Mitigación: exportar la DB desde LocalWP antes de importar. La
migración repunta `home`/`siteurl` al dominio `.test`, pero **no** hace
`search-replace` del contenido (URLs `*.local` embebidas en posts siguen ahí);
si hace falta, correr `wp search-replace` manualmente tras migrar.

## Importar proyecto: carpetas sin config (`reconstructed`) son best-effort

`import_disconnected_site` re-importa sin pérdida las carpetas con
`config.disconnected.json` (`preserved`). Para carpetas viejas **sin** ninguna
config (`reconstructed`) la metadata se deduce best-effort: nombre = carpeta,
dominio `{slug}.test`, `dbName` parseado de `wp-config.php` (o el slug), y
**versiones PHP/DB por defecto** (8.3 / MySQL 8.0) — pueden no coincidir con las
originales. Tras importar, revisa dominio y versiones en `/site/[id]` antes de
«Migrar y encender». (Las carpetas desconectadas por el propio panel siempre son
`preserved`, así que esto solo aplica a carpetas traídas de fuera sin sidecar.)

## Botones de la barra de título no respetan la config de KDE

**Síntoma:** en KDE/Wayland los botones (cerrar/min/max) no aparecen donde el
usuario los tiene configurados (este equipo: izquierda, `kwinrc`
`ButtonsOnLeft=XAIH`). El objetivo es que la decoración sea nativa y portable
entre máquinas sin hardcodear el lado.

**Intentos hechos:**
- `decorations: true` en `tauri.conf.json` (decoración activada) — no bastó.
- `GTK_CSD=0` en el arranque (`lib.rs`) para forzar decoración del servidor
  (KWin) — **no tuvo efecto**; revertido para no dejar cambios inertes.

**Hipótesis a probar más adelante:**
- tao/GTK en Wayland fuerza CSD y no consume el protocolo `xdg-decoration`
  (server-side) que ofrece KWin. Posibles vías:
  - Forzar el protocolo de decoración del servidor a nivel de ventana GTK.
  - Sincronizar `gtk-decoration-layout` en runtime hacia el proceso (ya está en
    `~/.config/gtk-{3,4}.0/settings.ini` como `close,maximize,minimize:` = izq,
    pero la ventana no lo aplica).
  - Probar en sesión X11 para aislar si es específico de Wayland/KWin.
  - Revisar versión de webkit2gtk/tao y issues upstream de Tauri sobre CSD en KDE.

**Estado:** diferido por decisión del usuario hasta cerrar todas las fases.
