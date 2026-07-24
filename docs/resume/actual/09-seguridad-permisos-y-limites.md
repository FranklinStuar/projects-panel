# Seguridad, permisos y límites

## Modelo de confianza

Es una herramienta local para una sesión de escritorio de confianza, no un servicio multiusuario ni una plataforma de hosting. Sus fronteras son el usuario Unix, el socket Docker, el bus D-Bus de sesión, loopback y el filesystem del usuario.

```text
Internet ── descargas (imágenes, WP-CLI, WordPress/Git)
                         │
usuario local ── Tauri ──┼─ Docker daemon (privilegio efectivo alto)
                         ├─ procesos host/git/build
                         ├─ D-Bus sesión
                         └─ pkexec/sudo solo configuración de sistema

red pública X      servicios publicados solo en 127.0.0.1
```

El acceso al daemon Docker permite control equivalente a privilegios elevados en muchas instalaciones; pertenecer al grupo `docker` es una decisión de confianza relevante.

## Exposición de red

- Nginx publica sitios en `127.0.0.1` y **siempre puertos altos 8080/8443+** elegidos dinámicamente.
- Mailpit 8025, MinIO 9100/9101 y Adminer 8088 se bindean a loopback.
- DB y PHP no publican puertos host; se comunican por `panel-net`.
- DNS `.test` apunta a loopback; certificados son de una CA local mkcert.

Esto reduce exposición remota, pero cualquier proceso local puede alcanzar esos puertos.

## Credenciales y secretos

Las credenciales locales están codificadas para conveniencia: root DB `panel`, usuario Postgres `panel`, MinIO `panel/panel-secret`; Adminer auto-inicia sesión. Son inapropiadas para red no confiable o producción. Los certificados y claves viven en la carpeta del proyecto. GitHub reutiliza sesión, SSH keys y keyring del usuario; el panel no almacena token propio.

El mu-plugin de one-click admin es una superficie sensible aun en local: debe asumirse accesible solo a través de loopback y dominio local. La elección de usuario se guarda en localStorage, no una contraseña.

## Permisos del host

Operación ordinaria escribe en `~/panel-wp`, `~/.config/wordpress-panel` y `~/.local/bin`. Los containers PHP remapean `www-data` al UID/GID host y WP-CLI corre como `www-data`, evitando archivos root en binds.

Privilegio explícito:

- `scripts/first-run.sh` usa `sudo` para NetworkManager/dnsmasq y puede sugerir instalación de paquetes.
- `domain::install_wildcard` usa `pkexec sh -c` para instalar regla y recargar NetworkManager.
- `mkcert -install` modifica la confianza local de certificados.

No hay daemon root propio.

## Tauri y WebView

La ventana tiene decoraciones nativas y CSP configurada como `null`. La capability principal concede `core:default` y `core:event:default`; esta última habilita listeners. Los comandos Tauri propios no están protegidos por ese ACL. La app carga su SPA local y usa opener/shell plugins; revisar cualquier futura carga de contenido remoto es esencial.

`GTK_CSD=0` no forma parte del modelo actual: fue revertido. IA tampoco existe; no hay envío de código/datos a proveedor LLM.

## Validación de rutas y comandos

Hay defensas puntuales: `github::remove_dir` canonicaliza y solo borra bajo `wp-content`; importación valida carpeta bajo `projects_root`; snapshot/worktree limpian rutas en símbolos propios. Sin embargo, varias rutas relativas llegan desde UI/CLI y deben verificarse en cada módulo, no asumir una política central.

El deploy Git ejecuta un `build_cmd` persistido mediante `$SHELL -lc` con permisos del usuario. Es funcionalidad de ejecución arbitraria intencional para configuración confiable, no sandbox. MCP puede invocar las herramientas publicadas y por extensión operaciones destructivas; su autorización depende del cliente MCP y de la sesión local.

## D-Bus, CLI y plasmoid

D-Bus publica métodos en el bus de sesión sin comprobación adicional del caller. El CLI requiere panel abierto, pero no autentica más. El plasmoid ejecuta `qdbus6` mediante shell. MCP usa stdio y spawn del CLI; stdout se reserva al protocolo.

## Operaciones destructivas y durabilidad

Borrar proyecto puede eliminar carpeta y hacer `DROP DATABASE`; la UI solicita confirmación, pero el backend confía en parámetros. Desconectar preserva carpeta/config sidecar. Stop intenta dump final best-effort; DB durable y auto-dump reducen pérdida, no sustituyen backup externo. Snapshots se almacenan en el mismo disco del proyecto, por lo que no protegen frente a fallo del disco.

## Límites/deuda prioritaria

- CSP deshabilitada.
- Credenciales estáticas y auto-login, aceptables solo bajo premisa local.
- Socket Docker y builds de shell amplían mucho el poder del proceso.
- D-Bus no autoriza callers ni usa tipos estructurados para JSON.
- Tags `latest` reducen reproducibilidad/supply-chain control.
- Plugins/binarios descargados no muestran verificación criptográfica propia.
- No existe aislamiento para código WordPress, repos o builds potencialmente maliciosos más allá de containers parciales y permisos Unix.

No se propone aquí un rediseño; el inventario exhaustivo de permisos y comandos corresponde a `../referencia/*`.

## Fuentes primarias

- `src-tauri/src/docker.rs::host_port_map`, `create_php_container`, `db_env`, `autoselect_endpoint`
- `src-tauri/src/domain.rs::install_wildcard`
- `scripts/first-run.sh`
- `src-tauri/src/github.rs::remove_dir`, `deploy`
- `src-tauri/src/lib.rs::delete_site`, `run`
- `src-tauri/src/dbus.rs::serve`, `Manager`
- `src-tauri/capabilities/default.json::permissions`
- `src-tauri/tauri.conf.json::app.security`
- `docker/adminer/autologin.php`
