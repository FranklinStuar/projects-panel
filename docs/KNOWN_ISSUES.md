# Problemas conocidos (a revisar al final de todas las fases)

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
