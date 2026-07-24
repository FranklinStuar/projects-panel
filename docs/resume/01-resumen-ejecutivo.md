# Resumen ejecutivo

> **Estado:** `CURRENT-CONFIRMED` para sistema descrito; `TARGET` para conclusiones de reconstrucción.  
> **Corte:** 2026-07-23 · `main` · `373841c`.

## Qué es Panel WP

Panel WP es aplicación de escritorio para desarrollo WordPress local. Reemplaza LocalWP con control explícito sobre versiones, containers, dominios y herramientas, priorizando bajo consumo.

Stack principal:

- **Tauri 2 + Rust**: shell, comandos y orquestación.
- **SvelteKit + Svelte 5**: SPA embebida en Tauri.
- **Docker + bollard**: ejecución de PHP y servicios.
- **D-Bus + QML**: integración con plasmoid KDE.
- **Shell/Node**: wrappers CLI y servidor MCP.

## Principios que explican diseño actual

1. **Nada corre si no hace falta.** Proyecto parado no conserva container PHP activo.
2. **Compartir antes que duplicar.** nginx, Mailpit, MinIO, Adminer y motores DB se comparten cuando es viable.
3. **Imágenes mínimas.** Alpine donde imagen oficial lo permite.
4. **Proyectos portables.** Cada carpeta conserva `config.json`, WordPress, configuración, certificados y dumps.
5. **Backend como autoridad.** Frontend invoca funciones; no orquesta Docker directamente.

Estas decisiones funcionaron y deben sobrevivir al rebuild.

## Modelo operativo

Todos los containers viven en `panel-net`:

```text
navegador
    │
    ▼
panel-nginx ─────► wp-{site-id}:9000 ─────► panel-{db}-{version}
    │                       │
    │                       ├────► panel-mailpit:1025
    │                       └────► panel-minio (si proyecto lo pide)
    │
    ├──── UI Mailpit :8025
    ├──── Adminer :8088
    └──── MinIO :9100/:9101
```

Solo nginx publica HTTP/HTTPS del sitio. Código actual cede 80/443 a LocalWP y selecciona puertos altos desde 8080/8443, persistidos en `panel.json`.

Container `wp-{id}` monta código del host, php.ini y WP-CLI; no publica puerto host. `PUID/PGID` alinea `www-data` con usuario host. DB compartida usa datadir durable en `~/.config/wordpress-panel/db-data/`.

Al detener último proyecto, panel apaga servicios compartidos innecesarios. Meta: **N proyectos parados = 0 containers del panel ejecutándose**.

## Fuente de verdad y estado

Fuente de verdad de cada proyecto:

```text
~/panel-wp/{slug}/config.json
```

Panel escanea carpetas; no mantiene `sites.json` central. Estado adicional existe fuera del proyecto:

- endpoint global en `panel.json`;
- orden y grupos vacíos en `groups.json`;
- datadirs DB compartidos;
- vhosts nginx generados;
- cache de versiones WP;
- `dump-log.jsonl`;
- datos MinIO.

Estado runtime se deriva de Docker. Esta distribución da portabilidad, pero complica consistencia entre filesystem, Docker y archivos auxiliares.

## Funciones actuales

### Proyectos y WordPress

- crear WordPress con versión elegida;
- seleccionar PHP, motor y versión DB;
- encender, detener, detener todo;
- abrir sitio, admin, carpeta, terminal y VS Code;
- ejecutar WP-CLI;
- listar plugins, themes y usuarios;
- auto-login efímero de un uso;
- ajustar límite PHP de subida;
- borrar completamente o desconectar conservando carpeta.

### Infraestructura local

- dominios wildcard `.test` con dnsmasq;
- SSL local con mkcert;
- nginx compartido;
- MySQL, MariaDB y PostgreSQL;
- Mailpit compartido;
- MinIO opcional;
- Adminer con acceso directo a DB;
- recuperación de nginx tras apagón sucio.

### Datos y migración

- datadir DB durable;
- dump manual;
- dump final al detener;
- auto-dump al detectar cambios;
- rotación de dumps y log JSONL;
- migración entre máquinas desde dump;
- importación desde LocalWP;
- reimportación de carpeta desconectada.

### Código y aislamiento de pruebas

- clonar, registrar, escanear, pull y eliminar repos Git;
- estado ahead/behind/dirty;
- workspace VS Code multi-root;
- deploy directo local: checkout + pull fast-forward + build;
- puntos de guardado con código comprimido + DB;
- clones temporales desde snapshot;
- worktree-projects que sobreponen solo repo objetivo y wp-config sobre WordPress padre.

### Superficies de control

- UI Tauri/Svelte;
- comandos Tauri IPC;
- D-Bus;
- `wordpress-panel-cli`;
- wrapper host `wp`;
- servidor MCP sin dependencias;
- plasmoid KDE.

Paridad no es completa: catálogos de `referencia/` detallan qué función aparece en cada superficie.

## Funciones complejas que conviene preservar

### Protección de DB en capas

1. datadir durable evita pérdida al recrear container;
2. auto-dump protege frente a corrupción o fallo operativo;
3. dump al detener deja punto reciente portable;
4. snapshot combina DB y código para crear entorno derivado.

Capas se complementan. Ninguna sustituye todas las demás.

### Migración con dump grande

Import usa `docker exec -i` en vez de bollard con stdin por deadlock observado. Añade pragmas, indicador de progreso, watchdog y reset de DB si import queda parcial.

### Worktree-projects

No duplican instalación WordPress. Montan `public` del padre y sobreponen repo Git + wp-config. DB puede compartirse sin mutar URLs del padre o copiarse. Optimiza disco y tiempo, pero exige composición cuidadosa en Docker y nginx.

### Ciclo de recursos

`start_site`, `stop_site` y `teardown_unused_shared` coordinan servicios por demanda. Esta política es valor central, aunque hoy está acoplada a detalles bollard.

## Estado del producto

Fases 1–4 están funcionalmente construidas, aunque encabezado histórico de Fase 1 siga diciendo “en curso”. Fase 5 IA no está implementada.

Diferidos o parciales:

- headless solo guarda flags;
- MinIO no instala integración S3 para WordPress;
- Cloudflare/deploy/package tienen stubs históricos;
- import LocalWP depende de dump en disco y no reemplaza URLs embebidas;
- barra de título KDE/Wayland sigue pendiente;
- deploy VPS/cPanel/Bitnami es idea futura, no función actual.

## Por qué cuesta cambiarlo

Problema no es falta de funciones. Problema es crecimiento transversal:

- `lib.rs` concentra adaptadores IPC y coordinación;
- `docker.rs` mezcla runtime, política, red, storage y recuperación;
- `ProjectDetail.svelte` concentra gran parte de experiencia de proyecto;
- contratos Rust/TypeScript/D-Bus/CLI/MCP se mantienen manualmente;
- operaciones multi-paso dependen de compensaciones ad hoc;
- estado se reparte entre proyecto, config global y Docker;
- progreso usa canal global sin identidad de operación;
- comandos host y paths amplían superficie de seguridad;
- tests mock UI no validan ACL/eventos Tauri reales.

Cambiar una función obliga a tocar varias superficies y mantener paridad manual.

## Dirección recomendada

Rebuild debe separar:

```text
UI
  → contrato generado
Application API
  → casos de uso + coordinador de operaciones
Dominio y políticas
  → puertos abstractos
Adaptadores
  → Bollard, filesystem, procesos, Tauri, D-Bus, CLI, MCP
```

Piezas clave:

- schemas versionados y migraciones explícitas;
- escritura atómica y locks por proyecto;
- reconciliador desired/actual para recursos;
- operaciones con `operationId`, progreso tipado, cancelación y journal;
- contratos generados Rust→TypeScript y manifiesto compartido;
- validación fuerte de rutas y comandos build;
- secretos locales mejor gestionados;
- dumps en streaming;
- adaptadores de plataforma;
- tests de dominio, contratos, adapters, Docker, Tauri runtime y e2e.

## Estrategia de reconstrucción

No hacer big-bang sin compatibilidad. Orden recomendado:

1. especificar schemas y contratos;
2. crear repository portable compatible con `config.json` actual;
3. construir runtime Docker simulable y reconciliador;
4. portar lifecycle mínimo;
5. portar DB durable y provisioning WordPress;
6. portar dominios/SSL;
7. portar backup, restore y recuperación;
8. portar Git, snapshots, clones y worktrees;
9. construir UI sobre API estable;
10. añadir CLI/D-Bus/MCP como adaptadores;
11. portar importadores;
12. endurecer, migrar instalaciones y retirar sistema anterior.

IA y deploy remoto deben entrar después como dominios separados, no aumentar núcleo local antes de estabilizarlo.

## Cómo usar este conjunto

- **Entender producto:** `02-estado-y-matriz-funcional.md`.
- **Entender runtime:** `actual/01-arquitectura-general.md`.
- **Operar:** `operacion/`.
- **Consultar contratos:** `referencia/`.
- **Entender decisiones y bugs:** `historia/`.
- **Empezar rebuild:** `reconstruccion/01-objetivos-principios-y-no-objetivos.md`.
- **Comprobar cobertura:** `verificacion/01-matriz-de-trazabilidad.md`.

## Fuentes primarias

- `CLAUDE.md`
- `src-tauri/src/config.rs::SiteConfig`
- `src-tauri/src/docker.rs::DockerManager`
- `src-tauri/src/lib.rs::run`
- `src-tauri/src/wordpress.rs`
- `src-tauri/src/migrate.rs`
- `src-tauri/src/snapshot.rs`
- `src-tauri/src/worktree.rs`
- `src/lib/components/ProjectDetail.svelte`
- `scripts/wordpress-panel-cli.sh`
- `mcp/server.mjs`
- `docs/CHANGELOG.md`
- `docs/KNOWN_ISSUES.md`
