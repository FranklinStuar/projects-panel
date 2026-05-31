#!/bin/bash
# Empaqueta el plasmoid KDE en un .plasmoid distribuible (un zip de metadata.json
# + contents/). Idempotente: recrea el archivo en cada ejecución.
#
# Uso:
#   bash scripts/package-plasmoid.sh
# Instalar el artefacto resultante:
#   kpackagetool6 --type Plasma/Applet --install dist/wordpress-panel.plasmoid
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$REPO/plasma/applets/wordpress-panel-plasmoid"
OUT_DIR="$REPO/dist"
OUT="$OUT_DIR/wordpress-panel.plasmoid"

if [ ! -f "$SRC/metadata.json" ]; then
    echo "error: no se encontró $SRC/metadata.json" >&2
    exit 1
fi
command -v zip >/dev/null 2>&1 || { echo "error: falta 'zip' (instálalo con tu gestor de paquetes)" >&2; exit 1; }

mkdir -p "$OUT_DIR"
rm -f "$OUT"

# El zip debe tener metadata.json y contents/ en la raíz del archivo.
( cd "$SRC" && zip -r -q "$OUT" metadata.json contents -x '*/.*' )

echo "[+] plasmoid empaquetado: ${OUT#"$REPO"/}"
echo "    instalar:  kpackagetool6 --type Plasma/Applet --install ${OUT#"$REPO"/}"
echo "    actualizar: kpackagetool6 --type Plasma/Applet --upgrade ${OUT#"$REPO"/}"
