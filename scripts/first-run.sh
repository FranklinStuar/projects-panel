#!/bin/bash
# Primera configuración de Panel WP en un sistema Linux con KDE + NetworkManager.
# Idempotente: se puede correr varias veces. Requiere sudo solo para dnsmasq y
# (si falta) instalar mkcert. Pensado para Manjaro/Arch (pacman).
set -euo pipefail

echo "==> Panel WP — primera configuración"

# 1. Red Docker compartida ------------------------------------------------------
if docker network inspect panel-net >/dev/null 2>&1; then
    echo "  [ok] red docker panel-net ya existe"
else
    docker network create --driver bridge panel-net >/dev/null
    echo "  [+] red docker panel-net creada"
fi

# 2. dnsmasq wildcard *.test -> 127.0.0.1 --------------------------------------
# NetworkManager debe usar su dnsmasq integrado (dns=dnsmasq).
NM_CONF="/etc/NetworkManager/conf.d/dns.conf"
if ! grep -rqs "dns=dnsmasq" /etc/NetworkManager/ ; then
    echo "  [+] activando backend dnsmasq en NetworkManager"
    sudo install -d /etc/NetworkManager/conf.d
    printf "[main]\ndns=dnsmasq\n" | sudo tee "$NM_CONF" >/dev/null
fi

SNIPPET="/etc/NetworkManager/dnsmasq.d/wordpress-panel.conf"
sudo install -d /etc/NetworkManager/dnsmasq.d
printf "address=/test/127.0.0.1\n" | sudo tee "$SNIPPET" >/dev/null
echo "  [+] wildcard *.test escrito en $SNIPPET"

sudo systemctl restart NetworkManager
sleep 3
if getent hosts panel-probe.test | grep -q 127.0.0.1; then
    echo "  [ok] *.test resuelve a 127.0.0.1"
else
    echo "  [!] *.test aún no resuelve — revisar NetworkManager/dnsmasq"
fi

# 3. mkcert (CA local para SSL .test) ------------------------------------------
if command -v mkcert >/dev/null 2>&1; then
    mkcert -install
    echo "  [ok] mkcert CA instalada"
else
    echo "  [i] mkcert no está instalado (SSL es opcional, Fase 2)."
    echo "      Instálalo con:  sudo pacman -S nss mkcert"
fi

# 4. Plasmoid KDE -------------------------------------------------------------
PLASMOID_DIR="$(dirname "$0")/../plasma/applets/wordpress-panel-plasmoid"
if command -v kpackagetool6 >/dev/null 2>&1 && [ -d "$PLASMOID_DIR" ]; then
    if kpackagetool6 --type Plasma/Applet --install "$PLASMOID_DIR" >/dev/null 2>&1; then
        echo "  [+] plasmoid KDE instalado"
    else
        kpackagetool6 --type Plasma/Applet --upgrade "$PLASMOID_DIR" >/dev/null 2>&1 \
            && echo "  [ok] plasmoid KDE actualizado" \
            || echo "  [i] plasmoid ya instalado o sin cambios"
    fi
    echo "      Añádelo al panel: clic derecho en el panel → Añadir widgets → 'Panel WP'"
    echo "      (Para distribuir: bash scripts/package-plasmoid.sh → dist/wordpress-panel.plasmoid)"
else
    echo "  [i] kpackagetool6 no disponible; omitiendo plasmoid (no es KDE?)"
fi

# 5. Wrapper WP-CLI -----------------------------------------------------------
BIN="$HOME/.local/bin"
SCRIPTS="$(dirname "$0")"
install -d "$BIN"
install -m755 "$SCRIPTS/wordpress-panel-cli.sh" "$BIN/wordpress-panel-cli"
install -m755 "$SCRIPTS/wp-wrapper.sh" "$BIN/wp"
echo "  [+] wrappers wp / wordpress-panel-cli instalados en $BIN"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "  [i] añade a tu shell:  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac

echo "==> Listo. Lanza el panel con:  pnpm tauri dev"
