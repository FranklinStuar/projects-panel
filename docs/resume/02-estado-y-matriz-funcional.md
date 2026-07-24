# Estado y matriz funcional

> **Estado:** `CURRENT-CONFIRMED` salvo filas marcadas `DEFERRED`, `STUB` o `IDEA`.  
> **Corte:** 2026-07-23 · `main` · `373841c`.

## Lectura de matriz

- **UI/IPC:** función accesible desde SPA y/o comando Tauri.
- **Ext.:** superficie externa: D-Bus, CLI, MCP o plasmoid.
- **Prueba:** cobertura automatizada conocida. “Mock” valida interacción, no Tauri/Docker real.
- **Paridad:** no asumir que todas las superficies ofrecen todas las funciones.

Catálogos exactos: [IPC/eventos](referencia/03-catalogo-ipc-y-eventos.md) y [D-Bus/CLI/MCP](referencia/04-catalogo-dbus-cli-y-mcp.md).

## Resumen de estado

| Área | Estado | Nota |
|---|---|---|
| Core local WordPress | `CURRENT-CONFIRMED` | Crear, arrancar, detener, borrar/desconectar. |
| Recursos Docker on-demand | `CURRENT-CONFIRMED` | Servicios compartidos + teardown. |
| DB durable y dumps | `CURRENT-CONFIRMED` | Datadir bind + dump manual/stop/auto. |
| Migración/importación | `CURRENT-CONFIRMED` | LocalWP, carpeta desconectada, dump SQL. |
| Git/snapshots/worktrees | `CURRENT-CONFIRMED` | Varias superficies; paridad parcial. |
| UI master-detail/grupos | `CURRENT-CONFIRMED` | Grupos persistentes y drag-and-drop. |
| CLI/D-Bus/MCP/plasmoid | `CURRENT-CONFIRMED` | Panel debe estar activo para mayoría de acciones. |
| Headless real | `DEFERRED` | Solo flags; no container frontend. |
| WordPress↔MinIO automático | `DEFERRED` | MinIO existe; plugin S3 no. |
| IA integrada | `DEFERRED` | Fase 5; `agent.rs` no existe. |
| Deploy remoto VPS/cPanel/AWS | `IDEA` | No confundir con deploy directo local actual. |

## Matriz funcional completa

### Proyecto y lifecycle

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| Listar proyectos y estado real | Current | `/`, `get_sites` | D-Bus, CLI, MCP, plasmoid parcial | Escanea `config.json`; consulta Docker | e2e dashboard | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Crear proyecto WordPress | Current | `/site/new`, `create_site` | — | Crea carpeta, schema, cert, config; instala WP | e2e mock + integración Docker | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Encender proyecto | Current | UI, `start_site` | D-Bus, CLI, MCP | PHP, DB, Mailpit, vhost/nginx, MinIO opcional | e2e mock + integración indirecta | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Detener proyecto | Current | UI, `stop_site` | D-Bus, CLI, MCP, plasmoid | Dump final, stop PHP, quita vhost, teardown | e2e mock | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Detener todos | Current | UI, `stop_all_sites` | D-Bus, CLI, plasmoid | Stop secuencial + teardown | cobertura parcial | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Borrar completamente | Current | modal, `delete_site(deleteFolder=true)` | — | Stop, DROP schema, borra carpeta | e2e delete-site | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Desconectar conservando carpeta | Current | modal, `delete_site(false)` | — | DROP schema; renombra config a sidecar; conserva archivos/dumps | e2e delete-site | [Importación](funciones/07-importacion-localwp-y-proyectos-desconectados.md) |
| Ventana de gracia al borrar | Current | `DeleteProjectModal`, `OpConsole` | — | Espera frontend 5 s antes de invoke destructivo | e2e delete-site | [Lifecycle](funciones/01-ciclo-de-vida-de-proyectos.md) |
| Abrir sitio/carpeta | Current | accesos rápidos IPC | D-Bus, CLI, MCP | Abre navegador/file manager host | cobertura mock parcial | [Herramientas](actual/08-herramientas-de-desarrollo.md) |

### WordPress y PHP

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| Consultar versiones WordPress | Current | form, `list_wp_versions` | — | Cache 24 h `wp-versions.json` | unit parcial | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Descargar/instalar versión elegida | Current | creación | — | Tarball en `app/public`, core install | integración Docker | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Ejecutar WP-CLI desde app | Current | `exec_wpcli` | — | Exec como `www-data` | integración indirecta | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Wrapper host `wp` | Current | instalación automática/manual | CLI shell | Detecta proyecto por CWD y usa `docker exec` | validación histórica live | [Git/terminal](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Listar plugins/themes | Current | tab, `list_plugins/themes` | — | WP-CLI JSON | mock parcial | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Auto-login de un uso | Current | `open_admin` | D-Bus, CLI, MCP | transient 60 s + mu-plugin | validación histórica; sin e2e real | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Elegir usuario auto-login | Current | `list_wp_users`, `open_admin(userId)` | Ext. usa default | Selección frontend en localStorage | mock IPC | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Reparar auto-login | Current | maintenance/acción, `repair_autologin` | — | Reinyecta mu-plugins y activa flag | mock | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Ajustar límite subida PHP | Current | UI, `set_php_upload_limit` | D-Bus, CLI, MCP | Config + php.ini + reload caliente | unit serde; operación sin e2e real | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Reparar php.ini de todos | Current | Settings, `repair_all_php_ini` | — | Regenera templates; reinicio requerido | mock | [Mantenimiento](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |
| Xdebug como selección/config | Current parcial | form/config | — | Flag + php.ini según implementación | sin cobertura específica | [WordPress](funciones/02-wordpress-wpcli-autologin.md) |
| Frontend headless administrado | Deferred | flags en form/config | — | No crea container frontend | — | [Deuda](historia/03-limitaciones-deuda-y-trabajo-diferido.md) |

### Dominios, red y SSL

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| DNS wildcard `.test` | Current | first-run/Settings | — | Snippet NetworkManager dnsmasq vía privilegio | unit helper + validación histórica | [Dominios](funciones/03-dominios-endpoints-y-ssl.md) |
| Endpoint global estable | Current | `panel_endpoint`, Settings | — | `panel.json`; puertos altos desde 8080/8443 | unit `site_url`/netcheck | [Dominios](funciones/03-dominios-endpoints-y-ssl.md) |
| Detectar puerto ocupado | Current | error backend | — | Lee `/proc/net/tcp*`; identifica proceso best-effort | unit netcheck | [Dominios](funciones/03-dominios-endpoints-y-ssl.md) |
| SSL local por proyecto | Current | creación, `regenerate_ssl` | — | `ssl/cert.pem`, `key.pem`; reload nginx | sin integración automatizada | [Dominios](funciones/03-dominios-endpoints-y-ssl.md) |
| Reparar nginx zombie/vhosts | Current | Settings, `repair_nginx` | — | Poda vhosts y recrea container | fix histórico; sin test específico | [Mantenimiento](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |

### DB, backups e importación

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| MySQL/MariaDB/Postgres por versión | Current | form/config | listado en Ext. | Container compartido por motor/versión | integración DB principalmente MySQL | [DB](funciones/04-bases-de-datos-y-migracion.md) |
| Datadir DB durable | Current | interno | — | Bind `config_dir/db-data/{container}` | lógica/fix histórico | [DB](funciones/04-bases-de-datos-y-migracion.md) |
| Migrar DB legacy a bind | Current | interno `ensure_db` | — | `docker cp`, recreación | sin test específico | [DB](funciones/04-bases-de-datos-y-migracion.md) |
| Dump manual | Current | Servicios, `export_db` | — | `app/sql/db-*.sql`, dump log | integración export | [Backups](funciones/06-backups-autodump-y-dumplog.md) |
| Dump al detener | Current | implícito | todas superficies stop | Dump best-effort + rotación | integración indirecta | [Backups](funciones/06-backups-autodump-y-dumplog.md) |
| Auto-dump por cambios | Current | interno | — | Watcher 20 s, gate, hash, rotación | sin integración específica | [Backups](funciones/06-backups-autodump-y-dumplog.md) |
| Consultar/limpiar dump log | Current | Settings, `dump_log/clean_dump_log` | — | Modifica JSONL; nunca borra SQL | mock parcial | [Backups](funciones/06-backups-autodump-y-dumplog.md) |
| Migrar proyecto desde dump | Current | `migrate_site` + consola | — | DB, wp-config, SSL, URL, start | integración crear→export→migrar | [DB](funciones/04-bases-de-datos-y-migracion.md) |
| Watchdog/rollback de import | Current | progreso `op-log` | — | Mata docker exec inactivo y resetea schema | validación histórica | [DB](funciones/04-bases-de-datos-y-migracion.md) |
| Importar desde LocalWP | Current | Settings, `import_localwp_site` | — | Copia public/dump; config pending | integración hermética + e2e mock | [LocalWP](funciones/07-importacion-localwp-y-proyectos-desconectados.md) |
| Reimportar carpeta desconectada | Current | modal, `import_disconnected_site` | — | Restaura/reconstruye config pending | integración hermética + e2e mock | [LocalWP](funciones/07-importacion-localwp-y-proyectos-desconectados.md) |
| Search-replace completo LocalWP | Deferred | — | — | Solo fija `home`/`siteurl`; URLs embebidas quedan | — | [Limitaciones](historia/03-limitaciones-deuda-y-trabajo-diferido.md) |

### Servicios

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| Mailpit compartido | Current | abrir UI | logs/containers por CLI/MCP | Arranca con proyecto; SMTP interno | sin e2e real | [Servicios](funciones/05-mailpit-minio-adminer-y-servicios.md) |
| MinIO on-demand | Current | toggle/abrir | logs/containers por CLI/MCP | Flag proyecto + datos globales | sin e2e real | [Servicios](funciones/05-mailpit-minio-adminer-y-servicios.md) |
| Plugin S3 WordPress↔MinIO | Deferred | — | — | No implementado | — | [Limitaciones](historia/03-limitaciones-deuda-y-trabajo-diferido.md) |
| Adminer compartido auto-login | Current | abrir DB | container visible | Arranca bajo demanda, puerto loopback | validación histórica | [Servicios](funciones/05-mailpit-minio-adminer-y-servicios.md) |
| Estado/recursos/logs de containers | Current | UI parcial | CLI, MCP | Docker inspect/stats/logs | sin test integral | [Logs](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |

### Git, snapshots, clones y worktrees

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| Estado `gh` | Current | Git tab, `gh_status` | — | Lee CLI host | mock parcial | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Clone repo por `gh` | Current | `gh_clone` | — | Carpeta bajo public + registro config | mock parcial | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Scan/register repo existente | Current | `gh_scan/register` | scan en CLI/MCP | Lee `.git`, actualiza config | sin test específico | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Pull/pull-all/remove | Current | Git tab | pull por D-Bus/CLI/MCP | Muta checkout/config | mock parcial | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Branch status ahead/behind/dirty | Current | Git tab | D-Bus, CLI, MCP | `git fetch`, lectura estado | sin test específico | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Deploy directo local | Current | Git tab | D-Bus, CLI, MCP | checkout + pull FF + build host | sin integración específica | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Deploy VPS/cPanel/AWS | Idea | — | — | No existe | — | [Backlog](reconstruccion/07-roadmap-backlog-y-migracion.md) |
| Workspace VS Code multi-root | Current | `open_vscode` | — | Crea una vez `.code-workspace` | sin test específico | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Abrir terminal en proyecto | Current | `open_terminal` | — | Lanza emulator host | sin test automatizado | [Git](funciones/08-git-github-vscode-terminal-y-deploy.md) |
| Crear/listar/borrar snapshot | Current | Snapshots tab | D-Bus, CLI, MCP | `code.tar.zst`, `db.sql`, `meta.json` | sin integración específica | [Snapshots](funciones/09-puntos-de-guardado-y-clones.md) |
| Exclusiones snapshot | Current | Snapshots tab | — | `snapshotExcludes` + meta | sin test específico | [Snapshots](funciones/09-puntos-de-guardado-y-clones.md) |
| Restaurar snapshot in-place | No existe | — | — | Clone es flujo disponible | — | [Snapshots](funciones/09-puntos-de-guardado-y-clones.md) |
| Crear clone desde snapshot | Current | Snapshots tab | D-Bus, CLI, MCP | Sitio + schema temporal | sin test específico | [Snapshots](funciones/09-puntos-de-guardado-y-clones.md) |
| Crear/listar/eliminar worktree-project | Current | Git tab | D-Bus, CLI, MCP | Git worktree, config, mounts, DB opcional | unit validación rama/slug | [Worktrees](funciones/10-worktree-projects.md) |

### UI, observabilidad e interfaces

| Capacidad | Estado | UI / IPC | Ext. | Persistencia/efecto | Prueba conocida | Detalle |
|---|---|---|---|---|---|---|
| Master-detail | Current | `/`, `ProjectDetail` | — | selectedId frontend | e2e dashboard/a11y | [UI](funciones/11-grupos-y-ui-master-detail.md) |
| Grupos persistentes | Current | create/rename/delete/reorder/DnD | — | `groups.json` + `config.group` | e2e parcial | [UI](funciones/11-grupos-y-ui-master-detail.md) |
| Logs live PHP | Current | Logs tab, `stream_logs` | CLI/MCP logs por servicio | Evento `log:{id}` | mock no valida Docker/event ACL | [Logs](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |
| Progreso operaciones largas | Current | `OpConsole` | algunos métodos externos emiten cambios | Evento global `op-log` | e2e mock | [Logs](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |
| Recarga UI por mutación externa | Current | listener `sites-changed` | D-Bus→UI | Evento sin payload | sin Tauri e2e | [Interfaces](actual/07-cli-dbus-mcp-y-plasmoid.md) |
| Estado del sistema | Current | Settings, `system_status` | — | Checks best-effort | e2e settings mock | [Mantenimiento](funciones/12-logs-progreso-mantenimiento-y-recuperacion.md) |
| CLI de administración | Current | — | D-Bus CLI | Script global del usuario | pruebas manuales | [CLI/MCP](actual/07-cli-dbus-mcp-y-plasmoid.md) |
| MCP para agentes | Current | — | MCP→CLI→D-Bus | Node stdio sin deps | prueba manual documentada | [CLI/MCP](actual/07-cli-dbus-mcp-y-plasmoid.md) |
| Plasmoid KDE | Current-unverified visual | — | qdbus6 | Poll 3 s | backend validado; visual pendiente | [CLI/MCP](actual/07-cli-dbus-mcp-y-plasmoid.md) |
| Asistente IA embebido | Deferred | — | MCP es control externo, no chat interno | No existe `agent.rs` | — | [Backlog](reconstruccion/07-roadmap-backlog-y-migracion.md) |

## Capacidades destructivas

Requieren tratamiento explícito en rebuild:

| Operación | Destrucción actual | Reversibilidad |
|---|---|---|
| Borrar proyecto con carpeta | Schema + carpeta | Solo backup externo/dump previo. |
| Desconectar | Schema; conserva carpeta/config sidecar | Reimportable, DB desde dump. |
| Borrar snapshot | Directorio snapshot | No recuperable desde app. |
| Quitar repo | Carpeta repo | Re-clonar si remoto contiene todo. |
| Eliminar worktree | Carpeta worktree + schema copiado | Rama se conserva por defecto; opción puede borrarla. |
| Limpiar dump log | Entradas JSONL | SQL no se borra; auditoría se pierde. |
| Reset endpoint | Config endpoint | Sitios existentes pueden conservar URLs antiguas. |

## Gaps principales para rebuild

1. contratos duplicados manualmente;
2. operaciones sin journal general;
3. progreso global sin `operationId`;
4. escrituras config no atómicas;
5. locks/concurrencia no formalizados;
6. políticas Docker mezcladas con adapter bollard;
7. credenciales locales fijas;
8. validación de paths/build commands mejorable;
9. dumps capturados en memoria en algunos caminos;
10. UI mock no prueba Tauri ACL/eventos reales;
11. soporte de plataformas acoplado a Linux/KDE;
12. paridad incompleta entre interfaces.

## Fuentes primarias

- `src-tauri/src/lib.rs::run`
- `src-tauri/src/config.rs`
- `src-tauri/src/docker.rs`
- `src-tauri/src/wordpress.rs`
- `src-tauri/src/migrate.rs`
- `src-tauri/src/github.rs`
- `src-tauri/src/snapshot.rs`
- `src-tauri/src/clone.rs`
- `src-tauri/src/worktree.rs`
- `src-tauri/src/dbus.rs`
- `src/lib/api.ts`
- `src/lib/components/ProjectDetail.svelte`
- `scripts/wordpress-panel-cli.sh`
- `mcp/server.mjs`
- `docs/TESTING.md`
