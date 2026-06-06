# /deploy — Build y deploy de Panel WP

Construye el release completo (Tauri + plasmoid) e instala la app localmente.
Acepta un argumento opcional: `check` para solo verificar sin compilar.

## Pasos

### 1. Contexto inicial

```bash
git status
git log --oneline -5
```

Reporta rama actual y últimos commits. Si hay cambios sin commitear, avisa pero continúa.

### 2. Verificación de tipos (rápida, siempre corre)

```bash
cd /home/franklin/MEGA/dev/wordpress-panel
pnpm check 2>&1
```

Si falla con errores de TypeScript/Svelte, detente y reporta. Warnings no bloquean.

```bash
cd src-tauri && cargo check 2>&1
```

Si falla con errores de Rust, detente y reporta. Warnings no bloquean.

Si el argumento fue `check`, termina aquí con reporte de estado.

### 3. Build Tauri release

```bash
cd /home/franklin/MEGA/dev/wordpress-panel
NO_STRIP=1 pnpm tauri build 2>&1
```

`NO_STRIP=1` es necesario en Manjaro/Arch: el `strip` bundleado en linuxdeploy no soporta la sección `.relr.dyn` de las libs modernas del sistema.

Esto tarda varios minutos (compila Rust en release + empaqueta frontend).
Genera en `src-tauri/target/release/bundle/`:
- `appimage/Panel WP_*.AppImage`
- `deb/Panel WP_*.deb`
- `rpm/Panel WP-*.rpm`

### 4. Empaquetar plasmoid

```bash
bash scripts/package-plasmoid.sh 2>&1
```

Genera `dist/wordpress-panel.plasmoid`.

### 5. Instalar localmente

Detecta qué método de instalación está disponible:

**Opción A — .deb (dpkg/apt):**
```bash
sudo dpkg -i "src-tauri/target/release/bundle/deb/Panel WP_"*".deb" 2>&1
```

**Opción B — AppImage (sin dpkg):**
```bash
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/512x512/apps

cp "src-tauri/target/release/bundle/appimage/Panel WP_"*".AppImage" ~/.local/bin/panel-wp.AppImage
chmod +x ~/.local/bin/panel-wp.AppImage

cp src-tauri/icons/icon.png ~/.local/share/icons/hicolor/512x512/apps/panel-wp.png

cat > ~/.local/share/applications/panel-wp.desktop << 'EOF'
[Desktop Entry]
Name=Panel WP
Comment=Gestor de proyectos WordPress locales
Exec=env WEBKIT_DISABLE_DMABUF_RENDERER=1 /home/franklin/.local/bin/panel-wp.AppImage
Icon=panel-wp
Terminal=false
Type=Application
Categories=Development;WebDevelopment;
StartupWMClass=panel-wp
EOF

update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

Usa la opción A si `dpkg` está disponible. Reporta cuál se usó.

### 6. Instalar/actualizar plasmoid (opcional)

Solo si `kpackagetool6` está disponible:

```bash
kpackagetool6 --type Plasma/Applet --upgrade dist/wordpress-panel.plasmoid 2>&1 \
  || kpackagetool6 --type Plasma/Applet --install dist/wordpress-panel.plasmoid 2>&1
```

### 7. Reporte final

Lista los artefactos generados con tamaño:

```bash
ls -lh \
  "src-tauri/target/release/bundle/appimage/Panel WP_"*".AppImage" \
  "src-tauri/target/release/bundle/deb/Panel WP_"*".deb" \
  "src-tauri/target/release/bundle/rpm/Panel WP-"*".rpm" \
  dist/wordpress-panel.plasmoid 2>/dev/null
```

Termina con resumen: versión, artefactos creados, método de instalación usado, errores si hubo.
