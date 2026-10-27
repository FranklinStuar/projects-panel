#!/bin/bash
# Primera configuración de Panel WP en un sistema Linux con KDE + NetworkManager
# o en macOS (Homebrew + Docker Desktop/OrbStack/Colima).
# Idempotente: se puede correr varias veces. Requiere sudo solo para dnsmasq y
# (si falta) instalar mkcert. Linux: pensado para Manjaro/Arch (pacman).
set -euo pipefail

echo "==> Panel WP — primera configuración"

IS_MAC=0
[ "$(uname -s)" = "Darwin" ] && IS_MAC=1

if [ "$IS_MAC" = 1 ]; then
    if ! command -v brew >/dev/null 2>&1; then
        echo "  [!] Homebrew es necesario en macOS: https://brew.sh" >&2
        exit 1
    fi
    if ! docker info >/dev/null 2>&1; then
        echo "  [!] Docker no responde. Abre Docker Desktop (u OrbStack/Colima) y reintenta." >&2
        exit 1
    fi
    # dnsmasq (wildcard *.test), mkcert + nss (CA local), jq (CLI), zstd
    # (bsdtar de macOS lo usa como compresor externo en snapshots).
    for pkg in dnsmasq mkcert nss jq zstd; do
        brew list --formula "$pkg" >/dev/null 2>&1 || { echo "  [+] brew install $pkg"; brew install "$pkg"; }
    done
fi

# 1. Red Docker compartida ------------------------------------------------------
if docker network inspect panel-net >/dev/null 2>&1; then
    echo "  [ok] red docker panel-net ya existe"
else
    docker network create --driver bridge panel-net >/dev/null
    echo "  [+] red docker panel-net creada"
fi

# 2. dnsmasq wildcard *.test -> 127.0.0.1 --------------------------------------
if [ "$IS_MAC" = 1 ]; then
    # dnsmasq de Homebrew escucha en 127.0.0.1:53 y /etc/resolver/test le manda
    # solo las consultas *.test (el resto del DNS del sistema no se toca).
    BREW_PREFIX="$(brew --prefix)"
    DNSMASQ_CONF="$BREW_PREFIX/etc/dnsmasq.conf"
    install -d "$BREW_PREFIX/etc/dnsmasq.d"
    touch "$DNSMASQ_CONF"
    if ! grep -qE "^conf-dir=$BREW_PREFIX/etc/dnsmasq.d" "$DNSMASQ_CONF"; then
        printf "conf-dir=%s/etc/dnsmasq.d/,*.conf\n" "$BREW_PREFIX" >> "$DNSMASQ_CONF"
    fi
    printf "address=/test/127.0.0.1\n" > "$BREW_PREFIX/etc/dnsmasq.d/wordpress-panel.conf"
    echo "  [+] wildcard *.test escrito en $BREW_PREFIX/etc/dnsmasq.d/wordpress-panel.conf"
    sudo brew services restart dnsmasq >/dev/null
    sudo install -d /etc/resolver
    printf "nameserver 127.0.0.1\n" | sudo tee /etc/resolver/test >/dev/null
    echo "  [+] /etc/resolver/test → 127.0.0.1"
    sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder 2>/dev/null || true
    sleep 2
    if dscacheutil -q host -a name panel-probe.test | grep -q 127.0.0.1; then
        echo "  [ok] *.test resuelve a 127.0.0.1"
    else
        echo "  [!] *.test aún no resuelve — revisar 'sudo brew services list' (dnsmasq)"
    fi
else
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
fi

# 3. mkcert (CA local para SSL .test) ------------------------------------------
if command -v mkcert >/dev/null 2>&1; then
    mkcert -install
    echo "  [ok] mkcert CA instalada"
else
    echo "  [i] mkcert no está instalado (SSL es opcional, Fase 2)."
    echo "      Instálalo con:  sudo pacman -S nss mkcert   (macOS: brew install mkcert nss)"
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
elif [ "$IS_MAC" = 0 ]; then
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
    *) echo "  [i] añade a tu shell (~/.zshrc en macOS):  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac

echo "==> Listo. Lanza el panel con:  pnpm tauri dev"
