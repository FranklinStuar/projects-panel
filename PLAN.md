# Panel WP — Plan de Implementación

## Contexto

El usuario usa LocalWP para desarrollo WordPress local pero necesita reemplazarlo con una solución propia que sea más ligera, flexible y sin servicios innecesarios. Los problemas de LocalWP: peso excesivo, versiones de PHP/MySQL fijas, WP-CLI roto, servicios internos desconocidos consumiendo recursos, puertos bloqueados en 127.0.0.1.

**Objetivo**: Panel de escritorio en Rust, basado en Docker, para gestionar múltiples proyectos WordPress locales con control total sobre versiones, configuración y servicios — **optimizando al máximo el consumo de recursos**.

### Principio rector (lo que LocalWP hace mal)

El problema central de LocalWP es que **corre todos sus servicios siempre**, consumiendo RAM/CPU aunque no se use ningún sitio. Panel WP invierte ese modelo, y todas las decisiones de arquitectura se subordinan a estas tres reglas:

1. **Nada corre si no hace falta.** Los containers solo arrancan para proyectos *activos*. Proyecto parado = 0 recursos.
2. **Compartir antes que duplicar.** Servicios con necesidades iguales se comparten entre proyectos (1 nginx para todos, DB por versión, 1 mailpit, 1 minio) en vez de instanciarse por proyecto.
3. **Imágenes mínimas.** Alpine donde exista, sin servicios internos opacos consumiendo recursos en segundo plano.

Todo lo demás (UI, asistente IA, migración) está subordinado a este principio: si una funcionalidad obliga a romper estas reglas, se rediseña o se pospone.

---

## Stack Tecnológico

### Backend
- **Tauri 2** (Rust): Framework de escritorio. Binario nativo Rust, sin Electron. WebKitGTK para el renderer (ya presente en Manjaro KDE). Uso de RAM ~30-50MB base. Es la mejor opción para una UI compleja (logs en tiempo real, múltiples paneles) manteniendo el backend en Rust puro.
- **bollard**: Docker API client para Rust (gestión de containers sin CLI)
- **tokio**: Async runtime para operaciones concurrentes
- **serde / serde_json**: Serialización de configuraciones
- **zbus**: D-Bus bindings para la comunicación con el widget KDE
- **reqwest**: HTTP client para descarga de versiones WP desde wordpress.org (el auto-login es local, no usa reqwest)

### Frontend (dentro de Tauri)
- **SvelteKit + Svelte 5**: Frontend minimalista y de muy bajo peso
- **`@sveltejs/adapter-static`**: Tauri sirve archivos estáticos, no SSR. Obligatorio configurar el adapter estático con `ssr = false` y prerender en todas las rutas — de lo contrario el build no funciona dentro de Tauri.
- **TailwindCSS**: Dark/light theme nativo con variables CSS
- Comunicación con Rust via Tauri IPC commands/events

### KDE Widget
- **QML Plasmoid** separado (único camino para un widget nativo de KDE Plasma)
- Comunica con el daemon Tauri via D-Bus (zbus expone la interfaz)

---

## Arquitectura Docker

### Filosofía
- **Solo php-fpm por proyecto**, y solo mientras el proyecto está activo (parado = 0 recursos).
- **Servicios compartidos entre todos los proyectos**: 1 nginx reverse-proxy, DB por versión, 1 mailpit, 1 minio. Arrancan on-demand (cuando al menos un proyecto activo los necesita).
- No se genera un docker-compose por proyecto con su propio nginx. El panel gestiona los containers vía bollard y mantiene el reverse-proxy compartido.

### Red Docker
Todos los containers (compartidos y por proyecto) viven en un único bridge **`panel-net`**. Esto permite resolución por nombre entre containers sin exponer puertos al host:
- `panel-nginx` alcanza el php-fpm de cada proyecto por nombre: `wp-{site-id}:9000`
- el php-fpm del proyecto alcanza su DB por nombre: `panel-mysql-84:3306`

Sin esta red, ningún container puede comunicarse — es prerequisito de Fase 1.

### Containers Compartidos (una instancia para todos, on-demand)
| Container | Imagen | Función |
|---|---|---|
| `panel-nginx` | nginx:alpine | **Reverse-proxy único.** 1 vhost (`server_name`) por proyecto activo, termina SSL, hace `proxy_pass`/FastCGI al php-fpm del proyecto. Recarga con `nginx -s reload` al alta/baja de proyectos. |
| `panel-mysql-{ver}` | mysql:8.0 / mysql:8.4 | MySQL; **un container por versión solicitada**, arranca solo si hay un proyecto activo usándola. Una DB por proyecto dentro. |
| `panel-mariadb-{ver}` | mariadb:{ver} | MariaDB por versión (si algún proyecto activo lo usa) |
| `panel-postgres-{ver}` | postgres:{ver}-alpine | PostgreSQL por versión (si algún proyecto activo lo usa) |
| `panel-mailpit` | axllent/mailpit | Captura de correos (todos los proyectos) |
| `panel-minio` | minio/minio | Simulación AWS S3 (on-demand, solo si un proyecto lo pide) |

> **Nota de imágenes:** `mysql:8.0-alpine` y `mariadb:*-alpine` **no existen** — se usan las imágenes debian oficiales (`mysql:8.0`, `mysql:8.4`, `mariadb:11.4`). Solo PostgreSQL tiene variante alpine real.

### Container por Proyecto (solo php-fpm, solo mientras activo)
```
wp-{site-id}:
  image: php:{php-version}-fpm-alpine   # ~30MB idle; el core WP lo gestiona el panel (tarball)
  networks: [panel-net]
  # NO expone puertos al host: solo panel-nginx le habla por la red interna
  volumes:
    - {project-path}/app/public:/var/www/html
    - {project-path}/conf/php/php.ini:/usr/local/etc/php/conf.d/zz-project.ini
    - wp-cli:/usr/local/bin/wp            # WP-CLI (phar) compartido; corre con el php del container
  environment:
    - PUID={host-uid}                     # ver "Permisos de archivos" abajo
    - PGID={host-gid}
```

> **Por qué `php:{ver}-fpm-alpine` y no `wordpress:*`:** la imagen `wordpress:*` ya trae el core de WordPress dentro, lo que choca con la descarga por tarball de la versión elegida. Además no existe un tag `wordpress:8.4-fpm-alpine` (esas imágenes taggean por versión de WP, no de PHP). Usar php-fpm puro da control total de la versión de WP vía el selector de wordpress.org, e imágenes más pequeñas. nginx ya no va aquí: es compartido.

### Permisos de archivos (UID/GID) — crítico
`www-data` dentro del container (uid 82 en alpine) ≠ uid del usuario host (típicamente 1000). Con bind-mounts de `app/public`, esto rompe WordPress: o www-data no puede escribir uploads/plugins, o el usuario no puede editar archivos clonados con `gh`. **Solución:** entrypoint del container ajusta el uid/gid de www-data al del host (`PUID`/`PGID`). Es requisito de Fase 1, no un detalle posterior.

### Versiones Soportadas
Basadas en lo que usa LocalWP actualmente en este sistema:
- **PHP**: 7.4, 8.0, 8.1, 8.2, 8.3, 8.4 (imágenes `php:{ver}-fpm-alpine`)
- **MySQL**: 8.0, 8.4
- **MariaDB**: 10.6, 10.11, 11.4
- **PostgreSQL**: 15, 16, 17

---

## Estructura de Archivos

### Configuración del Panel
```
~/.config/wordpress-panel/
├── panel.json           # Config global (paths, theme, etc.)
├── sites.json           # Registro de todos los sitios (equivalente al de LocalWP)
└── site-groups.json     # Grupos de proyectos
```

### Por Proyecto (replicando estructura LocalWP)
```
~/panel-wp/{site-name}/
├── config.json          # Metadata del sitio (ver formato abajo)
├── app/
│   ├── public/          # WordPress files (wp-admin, wp-content, etc.)
│   └── sql/             # Exports de DB
├── conf/
│   └── php/
│       └── php.ini      # PHP config específica del proyecto (montada en el container php-fpm)
│                        # El vhost nginx NO vive aquí: se genera en panel-nginx (compartido)
├── logs/
│   └── php/             # logs nginx son del panel-nginx compartido, no por proyecto
├── ssl/
│   ├── cert.pem         # Generado por mkcert
│   └── key.pem
└── data/                # Docker volumes (DB data si es proyecto-local)
```

### Formato config.json por Proyecto
```json
{
  "id": "uuid-v4",
  "name": "my-project",
  "path": "/home/user/panel-wp/my-project",
  "domain": "my-project.test",
  "group": "group-uuid",
  "createdAt": "2026-05-30T...",
  "services": {
    "php": { "version": "8.4" },
    "nginx": { "ssl": true },
    "db": { "type": "mysql", "version": "8.0", "dbName": "my_project_db" }
  },
  "github": {
    "theme": null,           // { "repo": "owner/repo", "branch": "main", "path": "wp-content/themes/my-theme" }
    "plugins": []            // [{ "repo": "owner/repo", "branch": "main", "path": "wp-content/plugins/my-plugin" }]
  },
  "oneClickAdmin": true,
  "xdebugEnabled": false,
  "headless": false,
  "frontendFramework": null
}
```

> **Sin puertos de host por proyecto.** Con nginx compartido, php-fpm no expone ningún puerto al host — solo `panel-nginx` le habla por `panel-net`. `panel-nginx` expone un único par de puertos host (http/https globales). Al activar un proyecto, el panel genera su vhost en `panel-nginx` (server_name = dominio, FastCGI → `wp-{id}:9000`, cert SSL del proyecto) y hace `nginx -s reload` (recarga sin cortar conexiones). Al desactivarlo, quita el vhost y recarga. La asignación de puertos por proyecto del plan anterior queda obsoleta.

---

## Módulos Rust (`src-tauri/src/`)

| Módulo | Función |
|---|---|
| `config.rs` | Leer/escribir sites.json, panel.json, config.json por sitio |
| `docker.rs` | Start/stop containers via bollard, red `panel-net`, arranque on-demand de servicios compartidos, regeneración de vhosts en `panel-nginx` + reload, streaming logs |
| `wpcli.rs` | Ejecutar WP-CLI dentro del container, export/import DB |
| `domain.rs` | Configurar dnsmasq wildcard (`*.test → 127.0.0.1`) una sola vez; no edita /etc/hosts por proyecto |
| `autologin.rs` | Auto-login al admin via mu-plugin (token efímero de un solo uso) → abrir navegador. No usa "magic link" de WP-CLI (no existe en core) |
| `github.rs` | Integración con `gh` CLI del sistema: clonar/pull repos de themes y plugins, sin OAuth propio |
| `logs.rs` | Stream logs de containers via Tauri events (async) |
| `dbus.rs` | Servidor D-Bus para comunicación con el plasmoid KDE |
| `ssl.rs` | Integración con mkcert para generar certificados .test (cargados por `panel-nginx` por vhost) |
| `ports.rs` | Tracking de los puertos host globales de `panel-nginx` (http/https) y de servicios compartidos que deban exponerse (mailpit UI, minio). Ya no asigna puertos por proyecto |
| `migrate.rs` | Detección de proyectos por escaneo, flujo de migración bajo demanda, export automático de DB al detener |
| `shutdown.rs` | Cierre graceful: export-al-stop, diálogo de advertencia al cerrar con proyectos activos, detección y adopción/limpieza de containers huérfanos al arrancar |
| `wordpress.rs` | Descarga e instalación de WordPress: fetch de versiones desde wordpress.org, descarga del tarball, `wp core install` via WP-CLI, cache local de versiones 24h |
| `agent.rs` | Integración con proveedores de IA (Claude, GPT, DeepSeek, Minimax): contexto del proyecto, tool use con aprobación, historial de chat por proyecto |

### Comandos Tauri (IPC Frontend ↔ Backend)
```rust
// Ejemplos de comandos expuestos al frontend
get_sites() -> Vec<SiteConfig>
create_site(config: NewSiteRequest) -> SiteConfig
start_site(id: String) -> Result<()>
stop_site(id: String) -> Result<()>
stop_all_sites() -> Result<()>
get_site_logs(id: String) -> Event stream
exec_wpcli(id: String, args: Vec<String>) -> String
open_admin(id: String) -> Result<()>
get_themes(id: String) -> Vec<ThemeInfo>
get_plugins(id: String) -> Vec<PluginInfo>
```

---

## Interfaz D-Bus para KDE Widget

Nombre del servicio: `com.goldmediatech.WordpressPanel`

```
Interface: com.goldmediatech.WordpressPanel.Manager
Methods:
  GetRunningSites() -> Array<{id, name, domain, port}>
  StopSite(id: String) -> Boolean
  StopAll() -> Boolean
  Quit() -> void
Signals:
  SiteStatusChanged(id: String, running: Boolean)
```

---

## Frontend Svelte (páginas)

| Página | Ruta | Contenido |
|---|---|---|
| Dashboard | `/` | Lista de proyectos agrupados, estado on/off, botones start/stop |
| Proyecto | `/site/:id` | Tabs: Info, Logs, Themes/Plugins, GitHub, Asistente IA |
| Nuevo Proyecto | `/site/new` | Form completo: nombre, dominio, versión WP, PHP, DB, admin — ver sección abajo |
| Dominios | `/domains` | Lista de dominios + puertos, edit hosts |
| GitHub | (tab dentro de `/site/:id`) | Repos de theme y plugins del proyecto, configurado post-instalación |
| MinIO | `/minio` | File browser del bucket MinIO |
| Configuración | `/settings` | Path de sitios, tema dark/light, preferencias |

---

## Instalación Automática de WordPress

### Formulario "Nuevo Proyecto"

Campos del formulario (en orden de aparición):

```
Nombre del proyecto     [my-project          ]
Dominio local           [my-project.test      ]  ← generado automático, editable

── WordPress ──────────────────────────────────
Versión de WordPress    [6.7.2 (latest) ▾     ]  ← lista cargada desde wordpress.org
Idioma                  [es_ES ▾              ]

── Entorno ────────────────────────────────────
Versión de PHP          [8.4 ▾                ]
Motor de base de datos  [MySQL ▾              ]  → MySQL / MariaDB / PostgreSQL
Versión del motor       [8.0 ▾                ]

── Administrador ──────────────────────────────
Usuario admin           [admin                ]
Contraseña              [••••••••    👁        ]
Email                   [franklin@...         ]
Título del sitio        [My Project           ]

── Opciones ───────────────────────────────────
☑ SSL (HTTPS)
☑ Auto-login al admin al encender
☐ XDebug
☐ Proyecto headless (añade frontend separado)
Grupo                   [Sin grupo ▾          ]
```

### Selector de versión de WordPress

Las versiones se obtienen de la API pública de wordpress.org al abrir el formulario (con cache local de 24h para no depender de internet):

```
GET https://api.wordpress.org/core/stable-check/1.0/
```

Respuesta: mapa de todas las versiones con su estado (`latest`, `outdated`, `insecure`).

Para el listado completo de versiones antiguas se complementa con:
```
https://wordpress.org/download/releases/
```

El selector agrupa visualmente las versiones:
```
Versión de WordPress
├── Última versión
│   └── 6.7.2 (latest) ← seleccionada por defecto
├── Serie 6.x
│   ├── 6.7.1
│   ├── 6.6.2
│   ├── 6.5.5
│   └── ...
├── Serie 5.x
│   ├── 5.9.10
│   └── ...
└── Versiones antiguas
    ├── 4.9.25
    └── ...
```

Las versiones marcadas como `insecure` en la API muestran un ícono de advertencia pero siguen siendo seleccionables (el usuario puede necesitarlas para replicar entornos de producción).

### Flujo de instalación tras confirmar el formulario

Se muestra una pantalla de progreso con pasos en tiempo real:

```
Creando proyecto "my-project"

✓ Creando estructura de carpetas
✓ Iniciando MySQL 8.0 compartido (on-demand)
✓ Creando base de datos  my_project_db
⟳ Descargando WordPress 6.5.5...      [=====>    ] 58%
  Iniciando container PHP 8.2
  Generando wp-config.php + mu-plugin Mailpit
  Ejecutando instalación de WordPress
  Generando certificado SSL
  Publicando vhost en panel-nginx
  Encendiendo proyecto
```

### Pasos internos (`wordpress.rs`)

```
1.  Crear carpetas del proyecto (app/public, conf, logs, ssl, app/sql)
2.  Iniciar container DB compartido si no está corriendo
3.  Crear base de datos vacía con el nombre del proyecto
4.  Descargar WordPress de la versión elegida:
      https://wordpress.org/wordpress-{version}.tar.gz
      → extraer en app/public/
5.  Generar wp-config.php con las credenciales del proyecto
6.  Generar conf/php/php.ini desde template (la config nginx es vhost en panel-nginx, no por container)
7.  Iniciar container php-fpm del proyecto en panel-net + añadir su vhost a panel-nginx (reload)
8.  Ejecutar dentro del container:
      wp core install
        --url=https://my-project.test
        --title="My Project"
        --admin_user=admin
        --prompt=admin_password   # password por stdin, no en argv (no visible en ps del container)
        --admin_email=...
9.  Inyectar mu-plugin panel-mailpit.php (SMTP Mailpit + X-Project-ID)
10. Generar certificado SSL con mkcert (si SSL activado) → cargado por el vhost de panel-nginx
11. (dominio ya resuelto por dnsmasq wildcard *.test — sin tocar /etc/hosts)
12. Guardar config.json del proyecto
13. Registrar en sites.json global
14. Marcar instalación como completada → mostrar botón "Abrir admin"
```

### Módulo `wordpress.rs`

Nuevo módulo que centraliza todo lo relacionado con la descarga e instalación de WordPress:

| Función | Descripción |
|---|---|
| `fetch_versions()` | Llama a la API de wordpress.org, retorna lista con estado de cada versión |
| `download_core(version)` | Descarga y extrae el tarball en la carpeta del proyecto |
| `install_core(site)` | Ejecuta `wp core install` via WP-CLI en el container |
| `configure_smtp(site)` | Inyecta config de Mailpit en wp-config.php |
| `cache_versions()` | Guarda la lista de versiones en `~/.config/wordpress-panel/wp-versions.json` (TTL 24h) |

---

## GitHub — Repos de Themes y Plugins

### Filosofía

GitHub se configura **después de crear el proyecto**, desde la vista de proyecto. No forma parte del formulario de instalación. El panel no maneja autenticación propia — usa el `gh` CLI que ya está instalado y autenticado en la máquina, aprovechando la sesión y las SSH keys existentes.

```
# El panel ejecuta internamente comandos como:
gh repo clone owner/my-theme wp-content/themes/my-theme
gh repo clone owner/my-plugin wp-content/plugins/my-plugin
git -C wp-content/themes/my-theme pull
```

### Requisito previo

Al abrir la sección GitHub de un proyecto, el panel verifica que `gh` esté instalado y autenticado:

```
┌────────────────────────────────────────┐
│  ✓ gh CLI detectado (v2.x)             │
│  ✓ Autenticado como @franklin          │
└────────────────────────────────────────┘
```

Si `gh` no está instalado o no tiene sesión, se muestra un aviso con el comando a ejecutar en terminal (`gh auth login`), sin intentar gestionar auth desde el panel.

### UI dentro de la vista de proyecto (tab "GitHub")

```
Theme
  Repositorio: [ owner/my-theme        ] [branch: main ▾]  [Clonar] [Pull]
  Carpeta:     wp-content/themes/my-theme  (detectada automáticamente)

Plugins
  [+ Agregar plugin]

  ┌─ owner/my-plugin-1  branch: main ──────────────────[Pull] [✕]─┐
  │  Carpeta: wp-content/plugins/my-plugin-1                       │
  └────────────────────────────────────────────────────────────────┘
  ┌─ owner/my-plugin-2  branch: develop ───────────────[Pull] [✕]─┐
  │  Carpeta: wp-content/plugins/my-plugin-2                       │
  └────────────────────────────────────────────────────────────────┘

[Pull todo]   ← ejecuta git pull en theme + todos los plugins
```

### Operaciones disponibles

| Acción | Comando ejecutado |
|---|---|
| **Clonar theme** | `gh repo clone owner/repo {project}/app/public/wp-content/themes/{name}` |
| **Clonar plugin** | `gh repo clone owner/repo {project}/app/public/wp-content/plugins/{name}` |
| **Pull theme** | `git -C {themes-path} pull origin {branch}` |
| **Pull plugin** | `git -C {plugin-path} pull origin {branch}` |
| **Pull todo** | Pull en paralelo sobre theme + todos los plugins configurados |

El panel ejecuta estos comandos en el **host** (no dentro del container), ya que los archivos están montados via volumen Docker — cualquier cambio en el host se refleja de inmediato en el container sin reiniciarlo.

### Carpeta detectada automáticamente

Al ingresar `owner/repo`, el panel propone la carpeta basándose en el nombre del repo:
- `owner/my-awesome-theme` → `wp-content/themes/my-awesome-theme`
- `owner/woo-my-plugin` → `wp-content/plugins/woo-my-plugin`

La carpeta es editable antes de clonar por si el nombre del directorio debe ser diferente al del repo.

### Guardado en config.json

```json
"github": {
  "theme": {
    "repo": "owner/my-theme",
    "branch": "main",
    "path": "wp-content/themes/my-theme"
  },
  "plugins": [
    { "repo": "owner/plugin-1", "branch": "main",    "path": "wp-content/plugins/plugin-1" },
    { "repo": "owner/plugin-2", "branch": "develop",  "path": "wp-content/plugins/plugin-2" }
  ]
}
```

### Comportamiento de la Ventana
- Cerrar ventana → app sigue corriendo como daemon (Tauri hide_window)
- System tray icon para reabrir
- El plasmoid KDE muestra estado en todo momento

---

## KDE Plasmoid (`plasma/applets/wordpress-panel-plasmoid/`)

```
metadata.json          # Declaración del plasmoid
contents/ui/
├── main.qml           # Widget root: lista de proyectos activos + botón "Cerrar todo"
└── ProjectRow.qml     # Fila: nombre, dominio, botón stop
```

**Comportamiento**:
- Al cargar: conecta via D-Bus y suscribe a señales de cambio de estado
- Muestra solo proyectos activos (running)
- Botón stop por proyecto → llama `StopSite(id)`
- Botón "Apagar todo y cerrar" → llama `StopAll()` luego `Quit()`
- Sin botón "Encender todos" (requisito del usuario)
- Click en proyecto → activa la ventana principal enfocando ese proyecto

---

## SSL con mkcert

```bash
# Al crear un proyecto con SSL activado:
mkcert {domain}.test
# Resultado:
#   {project}/ssl/cert.pem
#   {project}/ssl/key.pem
# mkcert -install debe correrse una vez al instalar el panel
```

---

## WP-CLI en Terminal

Para usar WP-CLI desde cualquier terminal del host, el panel instala un wrapper script:

```bash
# ~/.local/bin/wp (script instalado por el panel)
#!/bin/bash
# Detecta el proyecto según el directorio actual
PROJECT_ID=$(wordpress-panel-cli detect-project "$PWD")
docker exec -it wp-${PROJECT_ID} wp "$@" --allow-root
```

El panel también expone `wordpress-panel-cli` como binario CLI para operaciones básicas desde terminal.

---

## Mailpit — Separación de Correos por Proyecto

Un solo container Mailpit captura los correos de todos los proyectos. Para distinguirlos, el panel instala en cada proyecto un **mu-plugin** (`wp-content/mu-plugins/panel-mailpit.php`) que hace dos cosas vía el hook `phpmailer_init`:

```php
// mu-plugin inyectado automáticamente por el panel
add_action( 'phpmailer_init', function ( $mailer ) {
    $mailer->isSMTP();
    $mailer->Host = 'panel-mailpit';   // resoluble por panel-net
    $mailer->Port = 1025;
    $mailer->SMTPAuth = false;
    $mailer->addCustomHeader( 'X-Project-ID', 'my-project' );  // id real del proyecto
} );
```

> **Por qué mu-plugin y no `WP_SMTP_FROM_NAME`:** WP core no lee esa constante (es de plugins SMTP), y el "from name" no es un header `X-Project-ID`. Además `mail()` de PHP no llega a Mailpit sin configurar SMTP. El hook `phpmailer_init` resuelve ambas cosas: enruta a Mailpit por SMTP y añade el header real por el que filtra el UI del panel.

---

## Fases de Implementación

### Fase 1 — MVP Core (cimientos de la optimización de recursos)
1. Scaffold Tauri 2 + SvelteKit con `adapter-static` (`ssr=false`)
2. Sistema de configuración: `config.rs`, modelos de datos, sites.json
3. **Red `panel-net`** + arranque/parada on-demand de containers (base del "0 recursos si parado")
4. **Mapeo UID/GID host↔www-data** en el container php-fpm (sin esto WP no escribe archivos)
5. Container por proyecto: `php:{ver}-fpm-alpine` + DB compartida por versión, start/stop
6. **`panel-nginx` compartido**: generar vhost por proyecto activo + `nginx -s reload`
7. Dashboard UI: lista de proyectos, start/stop
8. Gestión de dominios: dnsmasq wildcard `*.test`
9. WP-CLI básico dentro del container

### Fase 2 — Funcionalidades Completas
1. D-Bus server (`dbus.rs`) + KDE Plasmoid
2. Logs en tiempo real via Tauri events
3. SSL con mkcert
4. Auto-login admin (one-click)
5. Lista de themes/plugins desde WP-CLI
6. GitHub via `gh` CLI: clonar/pull themes y plugins por proyecto
7. Selección de versión PHP/DB al crear proyecto
8. Grupos de proyectos

### Fase 3 — Servicios Adicionales
1. MinIO container + file browser UI
2. Soporte MariaDB y PostgreSQL
3. Headless WordPress + frontend (Next.js, etc.)
4. Botones stub: Cloudflare tunnel, backup, deploy, packaging (UI preparada, lógica posterior)
5. Wrapper WP-CLI para terminal

### Fase 4 — Polish
1. Panel de configuración completo
2. Migración entre sistemas: escaneo de `~/panel-wp/`, diálogo de migración bajo demanda, export automático de DB al detener
3. Migración desde LocalWP (leer `~/.config/Local/sites.json` para importar proyectos existentes)
4. Empaquetado del plasmoid para instalación

### Fase 5 — Asistente IA (opcional, expansible)
Fuera del núcleo de optimización de recursos. `agent.rs` y el tab "Asistente IA" se construyen una vez el resto es estable (ver sección "Agentes de IA"). Es la base mínima sobre la que se iterará.

---

## Estructura del Repositorio

```
wordpress-panel/             # sin Cargo.toml raíz: el crate Tauri vive en src-tauri/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── docker.rs        # incluye panel-net, on-demand, vhosts panel-nginx
│       ├── wpcli.rs
│       ├── domain.rs        # dnsmasq wildcard
│       ├── autologin.rs
│       ├── github.rs
│       ├── logs.rs
│       ├── dbus.rs
│       ├── ssl.rs
│       └── ports.rs
├── src/                  # Svelte frontend
│   ├── routes/
│   │   ├── +page.svelte        # Dashboard
│   │   ├── site/
│   │   │   ├── new/+page.svelte
│   │   │   └── [id]/+page.svelte
│   │   ├── domains/+page.svelte
│   │   ├── github/+page.svelte
│   │   ├── minio/+page.svelte
│   │   └── settings/+page.svelte
│   └── lib/
│       ├── components/
│       └── stores/
├── plasma/
│   └── applets/
│       └── wordpress-panel-plasmoid/
│           ├── metadata.json
│           └── contents/ui/
│               ├── main.qml
│               └── ProjectRow.qml
├── docker/
│   ├── php/
│   │   ├── Dockerfile        # php:{ver}-fpm-alpine + entrypoint UID/GID + WP-CLI phar
│   │   └── entrypoint.sh     # ajusta uid/gid de www-data a PUID/PGID del host
│   ├── nginx/
│   │   └── vhost.conf.tmpl   # template de vhost por proyecto (server_name, SSL, FastCGI)
│   ├── php.ini.tmpl          # config PHP base por proyecto
│   └── mu-plugins/
│       └── panel-mailpit.php # inyectado en cada proyecto (SMTP Mailpit + X-Project-ID)
├── scripts/
│   └── wp-wrapper.sh         # Script WP-CLI para instalar en ~/.local/bin/
└── PLAN.md                   # Este archivo
```

---

## Protección contra Pérdida de Datos

### El problema (y por qué no hay que sobre-diseñar)
Si hay proyectos corriendo y el sistema se apaga sin detener los containers, en teoría la DB podría quedar inconsistente. En la práctica el riesgo es bajo: **InnoDB es crash-safe** (redo log) y Docker detiene los containers en el shutdown ordenado del sistema. Los archivos de WordPress en `app/public/` siempre están en disco. Por eso la protección se mantiene **mínima y simple** (coherente con el principio rector), no con 5 capas redundantes.

### Capas de protección (3, no 5)

#### 1. Export siempre al detener — primera línea de defensa
Al hacer Stop normal (UI o plasmoid), la DB se exporta a `app/sql/` antes de apagar el container (ya definido en la sección de migración). Mantiene el dump siempre fresco sin maquinaria extra.

#### 2. Diálogo de advertencia al cerrar Panel WP
Si hay proyectos activos al intentar cerrar la aplicación (o al hacer "Apagar todo" desde el plasmoid):

```
┌──────────────────────────────────────────────────────┐
│  Hay 3 proyectos encendidos                          │
│                                                      │
│  • my-project      (exportando DB...)                │
│  • client-site     (exportando DB...)                │
│  • otro-proyecto   (exportando DB...)                │
│                                                      │
│  Se exportará la base de datos de cada uno           │
│  antes de apagarlos.                                 │
│                                                      │
│  [Cancelar]   [Apagar proyectos y cerrar]            │
└──────────────────────────────────────────────────────┘
```

- **Cancelar** → no se hace nada, el usuario vuelve al panel
- **Apagar proyectos y cerrar** → exporta DB de cada proyecto en paralelo, luego detiene los containers ordenadamente, luego cierra

#### 3. Detección de cierre sucio al arrancar
Al iniciar Panel WP, se comprueba si algún container del panel quedó corriendo (del sistema anterior o de un crash). Si se detectan containers huérfanos:

```
┌──────────────────────────────────────────────────────┐
│  Se detectaron proyectos que no se cerraron bien     │
│                                                      │
│  • my-project  (container activo sin registro)       │
│                                                      │
│  ¿Qué deseas hacer?                                  │
│                                                      │
│  [Detener y exportar DB]   [Adoptar como activos]    │
└──────────────────────────────────────────────────────┘
```

- **Detener y exportar DB** → para el container limpiamente y guarda un dump antes de apagarlo
- **Adoptar como activos** → los registra como proyectos en ejecución normal en el panel

> **Opcionales descartados del MVP:** un servicio systemd `Before=shutdown.target` (frágil con el user manager / lingering) y el export periódico cada N minutos. Solo se añaden si el uso real demuestra que hacen falta — no por defecto.

### Resumen de escenarios cubiertos

| Escenario | Protección |
|---|---|
| Usuario cierra Panel WP con proyectos activos | Diálogo de advertencia + export antes de cerrar |
| Usuario apaga el PC sin cerrar Panel WP | Docker para los containers en shutdown; InnoDB crash-safe; al reiniciar se detectan huérfanos |
| Fallo de luz / crash del OS | InnoDB es crash-safe; si el container paró sucio, se detecta al reiniciar |
| Crash del propio Panel WP | Containers siguen corriendo; al reiniciar el panel los adopta o detiene |

---

## Migración entre Sistemas

### Filosofía
Todo lo necesario para reconstruir un proyecto vive dentro de `~/panel-wp/{site-name}/`. Al cambiar de disco o formatear, basta con copiar esa carpeta al nuevo sistema. El panel detecta los proyectos automáticamente y los provisiona bajo demanda, justo antes de encenderlos, sin intervención manual previa.

### Qué contiene cada carpeta de proyecto (autosuficiente)
```
~/panel-wp/{site-name}/
├── config.json        # versión PHP, tipo/versión DB, dominio, puertos, grupos, etc.
├── app/public/        # archivos WordPress completos
├── app/sql/           # dump de la base de datos (exportado automáticamente al detener)
├── conf/php/php.ini   # configuración PHP del proyecto
└── conf/nginx/        # configuración Nginx del proyecto
```
Los certificados SSL (`ssl/`) no se migran — se regeneran con mkcert en el nuevo sistema.

### Flujo de migración

#### Paso 1 — Copiar carpeta
```bash
# En el sistema nuevo, simplemente:
cp -r ~/panel-wp/ /destino/
# o mover el disco y montar la misma ruta
```

#### Paso 2 — Instalar Panel WP en el sistema nuevo
Corre la primera configuración del panel (mkcert, dirs, plasmoid). El panel escanea `~/panel-wp/*/config.json` al iniciar y reconstruye `sites.json` automáticamente. Los proyectos encontrados aparecen marcados como **"Pendiente de migración"** (ícono distinto, no iniciables aún).

#### Paso 3 — Migración bajo demanda (al intentar encender un proyecto)
Cuando el usuario intenta iniciar un proyecto pendiente, aparece un diálogo:

```
┌─────────────────────────────────────────────┐
│  Migrar "my-project"                        │
│                                             │
│  Este proyecto necesita configuración       │
│  para funcionar en este sistema.            │
│                                             │
│  Se realizará:                              │
│  ✓ Crear base de datos MySQL (local)        │
│  ✓ Importar dump desde app/sql/             │
│  ✓ Generar certificado SSL (.test)          │
│  ✓ Publicar vhost en panel-nginx            │
│  ✓ Descargar imagen PHP 8.4 (si falta)      │
│                                             │
│         [Cancelar]    [Migrar y encender]   │
└─────────────────────────────────────────────┘
```

- **Cancelar** → el proyecto permanece en estado "Pendiente de migración", no se toca nada, no se enciende. El usuario puede ajustar config.json manualmente antes de reintentar.
- **Migrar y encender** → ejecuta los pasos en secuencia con barra de progreso, luego enciende el proyecto normalmente.

### Pasos internos de la migración (`migrate.rs`)

```
1. Validar config.json — detectar inconsistencias antes de empezar
2. Iniciar container compartido DB (si no está corriendo)
3. Crear base de datos vacía con el nombre del proyecto
4. Importar app/sql/{latest}.sql via WP-CLI o mysql CLI dentro del container
5. Actualizar wp-config.php con las credenciales del nuevo sistema (si cambiaron)
6. Regenerar certificados SSL con mkcert para el dominio del proyecto
7. (dominio resuelto por dnsmasq wildcard *.test — nada que migrar en DNS)
8. Publicar vhost del proyecto en panel-nginx (reload)
9. Marcar proyecto como migrado en sites.json (migrationPending: false)
10. Encender el container del proyecto
```

### Exportación automática al detener un proyecto
Para que el dump siempre esté actualizado al momento de migrar, el panel exporta la DB automáticamente cada vez que se detiene un proyecto:

```
Al hacer Stop en un proyecto:
  1. Exportar DB → app/sql/{YYYY-MM-DD}.sql (WP-CLI db export)
  2. Mantener solo los últimos 3 dumps (rotar automáticamente)
  3. Detener el container
```

Así, al copiar la carpeta a otro sistema, el dump es reciente sin requerir pasos manuales.

### Campo `migrationPending` en config.json
```json
{
  "id": "uuid-v4",
  "migrationPending": true,   // true si fue detectado por escaneo, false si ya migró
  "lastMigratedAt": null,     // fecha de la última migración exitosa
  ...
}
```

### Casos edge
| Situación | Comportamiento |
|---|---|
| Puerto http/https global de panel-nginx ya ocupado | El panel elige otro par libre para panel-nginx (los proyectos no tienen puertos host propios) |
| Imagen Docker de la versión PHP no existe | Se descarga durante la migración (se muestra en el progreso) |
| El dump SQL está corrupto o no existe | Se advierte al usuario, se ofrece iniciar sin DB (WordPress en blanco) |
| El usuario cambia de MySQL a MariaDB entre sistemas | Se detecta el cambio en config.json y se avisa antes de migrar |
| La carpeta `~/panel-wp/` no existe en el nuevo sistema | El panel pregunta la ruta en primera configuración |

---

## Verificación

1. Crear un proyecto WordPress, verificar que inicia con `docker ps`
2. Acceder al dominio `.test` en el navegador → WordPress instalado
3. WP-CLI desde terminal host: `cd ~/panel-wp/mi-proyecto && wp post list`
4. Auto-login admin: click en botón → abre `/wp-admin` logueado
5. Cerrar ventana → app sigue en system tray, plasmoid muestra proyecto activo
6. Desde plasmoid: stop del proyecto → container se apaga
7. Logs en tiempo real desde UI del panel
8. Múltiples proyectos simultáneos servidos por un único panel-nginx, ruteo correcto por dominio
9. **Recursos**: con N proyectos parados, `docker ps` no muestra ningún container del panel (0 consumo). Al encender 1, solo arrancan su php-fpm + la DB de su versión + panel-nginx
10. **Permisos**: subir un archivo desde wp-admin y editarlo desde el host (y viceversa) sin errores de permisos
11. **Migración**: copiar `~/panel-wp/` a sistema nuevo → panel detecta proyectos → intentar encender uno → diálogo de migración → aceptar → arranca con DB importada y SSL regenerado
12. **Export automático**: detener un proyecto → verificar que `app/sql/` tiene un dump reciente

---

## Notas de Instalación Inicial

El panel necesitará ejecutar una vez al instalar:
- `mkcert -install` (agregar CA al sistema)
- Configurar **dnsmasq wildcard** `*.test → 127.0.0.1` (una sola vez, integrado con NetworkManager)
- Crear la red Docker **`panel-net`**
- Construir la imagen base php-fpm (Dockerfile con entrypoint UID/GID + WP-CLI phar)
- Crear directorios de configuración en `~/.config/wordpress-panel/`
- Instalar el script WP-CLI wrapper en `~/.local/bin/`
- Instalar el plasmoid KDE (`kpackagetool6 --install`)

Estas acciones se exponen como pantalla de "Primera configuración" en el panel.

---

## Futuro: versión macOS

> **Nota pendiente** — cuando se migre a Mac, retomar este punto desde el sistema macOS para que la configuración se adapte nativamente.

Tauri 2 es multiplataforma por diseño, por lo que la base del proyecto ya soporta compilación en macOS sin cambios mayores en la lógica. Los puntos que requerirán adaptación específica al hacer el port:

- **KDE Plasmoid → macOS menu bar widget**: reemplazar el plasmoid QML por un widget de barra de menú nativo de macOS (Tauri ya soporta system tray en macOS con menú desplegable)
- **DNS `.test`**: dnsmasq funciona igual en macOS (vía Homebrew + un resolver en `/etc/resolver/test`), sin editar `/etc/hosts`. La config inicial de dnsmasq de Linux se reemplaza por el flujo equivalente de macOS
- **Docker**: en macOS se usa Docker Desktop en lugar del daemon nativo — la API de bollard es compatible, pero el socket está en una ruta distinta (`/var/run/docker.sock` via ruta de VM)
- **mkcert**: funciona igual en macOS, sin cambios
- **Rutas de configuración**: `~/.config/wordpress-panel/` → `~/Library/Application Support/wordpress-panel/` siguiendo la convención de macOS
- **`~/panel-wp/`**: la carpeta de proyectos es idéntica — la migración entre Linux y macOS funcionará con el mismo mecanismo de escaneo y diálogo de migración

---

## Agentes de IA

> **Esta sección es un punto de partida.** La integración de agentes es una funcionalidad que se irá expandiendo con el tiempo conforme se identifiquen casos de uso concretos — lo que está aquí es la base mínima sobre la que se construirán mejoras futuras.

### Propósito

Hay configuraciones en WordPress, PHP, Nginx y la base de datos que son complejas de hacer a mano: reglas de caché, configuración de multisite, ajuste de límites de memoria, problemas con permisos, conflictos de plugins, etc. Un agente de IA con acceso al contexto del proyecto puede hacer esos cambios directamente sin que el usuario tenga que buscar documentación ni editar archivos manualmente.

### Panel de chat por proyecto

Cada proyecto tiene un tab **"Asistente IA"** con un chat contextual. El agente conoce:
- La configuración actual del proyecto (`config.json`, `php.ini`, `nginx.conf`, `wp-config.php`)
- Los logs recientes (PHP, Nginx, MySQL)
- Los plugins y themes instalados
- El estado del container (corriendo, errores, uso de recursos)

Con ese contexto puede diagnosticar problemas y aplicar cambios directamente si el usuario lo aprueba.

### Flujo de aprobación

El agente nunca aplica cambios sin confirmación explícita:

```
Usuario: el sitio está dando error 500 después de activar WooCommerce

Agente: Revisé los logs de PHP y encontré el problema — el límite de
        memoria está en 128M y WooCommerce necesita al menos 256M.
        Propongo este cambio en php.ini:

        - memory_limit = 128M
        + memory_limit = 256M

        ¿Aplico el cambio y reinicio PHP?

        [Cancelar]  [Aplicar]
```

Todos los cambios propuestos se muestran como diff antes de aplicarse. Al aplicar, el panel hace el cambio en el archivo correspondiente y recarga el servicio dentro del container.

### Proveedores soportados

El usuario configura su proveedor y API key desde el panel de configuración. Soportados desde el inicio:

| Proveedor | Modelos |
|---|---|
| **Anthropic (Claude)** | claude-sonnet-4-6, claude-opus-4-8, claude-haiku-4-5 |
| **OpenAI (GPT)** | gpt-4o, gpt-4o-mini, o3-mini |
| **DeepSeek** | deepseek-chat, deepseek-reasoner |
| **Minimax** | MiniMax-Text-01, abab6.5s |

La API key se guarda en el keyring del sistema operativo (libsecret en Linux, Keychain en macOS), no en texto plano en el disco.

```
Configuración → Agente IA
Proveedor activo:  [Claude (Anthropic) ▾]
Modelo:            [claude-sonnet-4-6  ▾]
API Key:           [••••••••••••••••••   ] [Verificar]
```

El proveedor se puede cambiar por proyecto si se necesita usar uno distinto para un caso específico.

### Herramientas disponibles para el agente (`agent_tools`)

El agente recibe un conjunto de herramientas que puede llamar (con aprobación del usuario para las que modifican):

| Herramienta | Tipo | Descripción |
|---|---|---|
| `read_config` | lectura | Lee php.ini, nginx.conf, wp-config.php del proyecto |
| `read_logs` | lectura | Obtiene los últimos N líneas de logs |
| `list_plugins` | lectura | Lista plugins instalados/activos via WP-CLI |
| `list_themes` | lectura | Lista themes via WP-CLI |
| `get_container_stats` | lectura | CPU, RAM, estado del container |
| `write_config` | **escritura** | Modifica php.ini, nginx.conf (requiere aprobación) |
| `write_wpconfig` | **escritura** | Modifica wp-config.php (requiere aprobación) |
| `exec_wpcli` | **escritura** | Ejecuta comando WP-CLI (requiere aprobación) |
| `restart_service` | **escritura** | Recarga PHP-FPM o Nginx en el container (requiere aprobación) |

Las herramientas de lectura se ejecutan automáticamente para dar contexto. Las de escritura siempre piden confirmación mostrando el diff o el comando antes de proceder.

### Módulo `agent.rs`

```
agent.rs
  - Gestión de providers (Anthropic, OpenAI, DeepSeek, Minimax)
  - Construcción del contexto del proyecto para el system prompt
  - Loop de tool use / function calling
  - Serialización del historial de chat por proyecto
    → ~/.config/wordpress-panel/chats/{site-id}.json
```

### Lo que viene después

> Las siguientes mejoras se definirán en futuras versiones de esta sección a medida que se use el panel en producción y se identifiquen los casos más frecuentes:

- Acciones proactivas: el agente detecta anomalías en logs sin que el usuario pregunte
- Agente de optimización: analiza configuración y sugiere mejoras de rendimiento
- Soporte para MCP (Model Context Protocol) para conectar herramientas externas
- Historial de cambios aplicados por el agente con opción de revertir
- Modo "piloto automático" para tareas repetitivas pre-aprobadas (ej: siempre aumentar memory_limit si hay OOM)
