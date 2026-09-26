# Plan: Cloudflare Quick Tunnel por proyecto

Estado: **implementado**. Sustituyó al stub `feature_stub("cloudflare")`.
Cambio respecto al diseño original de abajo: no hay `write_config`/YAML ni
credentials-file — el comando `tunnel --url ... --http-host-header=...` no
necesita ninguno de los dos, así que `cloudflare.rs` quedó reducido a
`extract_url` (parseo del log). Todo lo demás (arquitectura, checklist)
sigue vigente.

## Objetivo

Botón "Exponer a internet" por proyecto: genera una URL pública temporal
(`https://algo-random.trycloudflare.com`) con HTTPS válido, sin que el
usuario tenga que dar de alta un dominio, crear cuenta Cloudflare, ni hacer
login. La URL cambia cada vez que se reinicia el túnel — eso es aceptado a
propósito (dominios temporales).

## Por qué Quick Tunnel y no Named Tunnel

Cloudflare tiene dos modos:
- **Named tunnel** (plan anterior, descartado): requiere zona propia en
  Cloudflare, login de cuenta, `tunnel create` + `route dns`. Da un dominio
  estable pero pide configuración de tu lado.
- **Quick tunnel**: `cloudflared tunnel --url http://...` — cero cuenta, cero
  dominio, cero login. Genera el hostname al vuelo y lo imprime en el log del
  proceso. Es justo lo que pediste: no necesitas proveer un dominio.

Se pierde la persistencia del hostname (cambia en cada `run`); si más
adelante se quiere un dominio fijo, hay que migrar al modo *named tunnel*
(login + zona), pero eso es un plan aparte, no este.

## Cómo funciona (arquitectura)

```
Internet (HTTPS) → *.trycloudflare.com (cert automático de Cloudflare)
    → container cf-{site-id} (cloudflared, en panel-net, sin puertos publicados)
      cmd: tunnel --url http://panel-nginx:80 --http-host-header={site.domain}
    → panel-nginx:80 (matchea server_name={site.domain}, el vhost *.test normal)
    → wp-{site-id} (php-fpm)
```

Truco clave: `--http-host-header` reescribe el `Host:` que le llega a
`panel-nginx` al dominio local del proyecto (`{id}.test`), aunque el
visitante haya entrado por la URL pública de Cloudflare. Así **no hace
falta tocar `nginx.rs`** ni crear un vhost nuevo — el vhost existente ya
matchea por `server_name`.

- Sin binario en el host: la imagen `cloudflare/cloudflared` corre solo en
  Docker, igual que cualquier otro container del panel.
- Sin cuenta ni secretos: no hay login, no hay `cert.pem`, no hay
  credentials-file. `cloudflare.rs` no necesita nada de manejo de auth.
- El SSL público lo entrega Cloudflare en su edge; `ssl.rs`/mkcert sigue
  siendo solo para `*.test` local, sin relación con esto.
- Un container por proyecto (`cf-{site-id}`), arranca/para bajo demanda por
  el usuario (no automático con `start_site`, para no violar "nada corre si
  no hace falta" — exponerse a internet es una decisión explícita, no el
  estado por defecto de un proyecto activo).

## Cambios por módulo

### `src-tauri/src/cloudflare.rs` (nuevo, mínimo)
- `extract_url(log: &str) -> Option<String>` → split por espacios/`|` (sin
  regex, no hace falta dependencia nueva) buscando un token
  `https://…trycloudflare.com` en el log del container (cloudflared lo
  imprime a los pocos segundos de arrancar). Único helper real; todo lo
  demás (crear/parar el container, leer el log) vive en `docker.rs`.

### `src-tauri/src/docker.rs`
Patrón por-proyecto (igual que `wp-{id}`, container dedicado, sin puertos
publicados):
- `ensure_cloudflared(site)`: container `cf-{site_id}`, imagen
  `cloudflare/cloudflared:latest`, red `panel-net`, sin `port_bindings`,
  sin binds (no hay credenciales que montar). Comando:
  `tunnel --url http://panel-nginx:80 --http-host-header={site.domain} --no-autoupdate`.
- `stop_cloudflared(site_id)`: para y borra el container (no hay estado que
  conservar entre corridas — el hostname es efímero por diseño).
- Reutilizar el mecanismo de logs ya existente (`logs.rs`) para leer las
  primeras líneas del container y aplicar `cloudflare::extract_url`, en vez
  de montar un watcher nuevo desde cero.

### Comandos IPC (`lib.rs` + `api.ts` + `types.ts`)
- `enable_tunnel(site_id) -> ()` — levanta `cf-{site_id}`.
- `disable_tunnel(site_id) -> ()` — lo para/borra.
- `tunnel_url(site_id) -> Option<String>` — lee logs recientes del
  container y devuelve la URL si ya apareció (polling simple desde el
  frontend cada 1-2s mientras el modal está abierto, como ya hace
  `OpConsole` con `op-log`; no hace falta evento nuevo si el polling basta).
- No hay campo persistente en `config.json` — es estado runtime, no
  configuración durable (como el estado de "running" de cualquier
  container, que ya se consulta on-demand vía Docker, no se guarda).

### Frontend
Reemplazar la rama `"cloudflare"` de `feature_stub` (`ProjectDetail.svelte`)
por un modal simple (`TunnelModal.svelte`): botón "Exponer a internet",
mientras arranca muestra "generando URL...", luego la URL con botón copiar,
y botón "Dejar de exponer". Sin campos de dominio ni de login.

### Docs
- `docs/ARCHITECTURE.md`: añadir `cf-{site-id}` a la tabla de containers
  (por-proyecto, on-demand, sin puertos publicados, sin datos durables).
- `docs/CHANGELOG.md`: entrada cuando se implemente.

## WordPress: assets funcionando por el túnel (verificado en vivo)

Probado end-to-end contra un proyecto real (`pgnyc`, SSL activado). Tres
problemas encadenados, cada uno con su fix:

1. **Nginx redirige `:80` a `https://$host`** cuando el proyecto tiene SSL
   (`nginx::render_vhost`). Con el Host reescrito, cloudflared caía en ese
   redirect hacia un dominio local inalcanzable. Fix: el túnel entra directo
   por `https://panel-nginx:443` con `--origin-server-name` (SNI) y
   `--no-tls-verify` (cert de mkcert, tráfico interno a `panel-net`).
2. **El `Host:` header debe igualar el `siteurl` EXACTO, puerto incluido**
   (`panel-nginx` publica en puertos altos, `Endpoint::site_url` →
   `pgnyc.test:8443`, no `:443`, para no chocar con LocalWP). Sin el puerto,
   WordPress detecta que no coincide con su `siteurl` y dispara su propio
   redirect canónico de vuelta a esa URL local. `ensure_cloudflared` arma el
   Host exacto con `Endpoint::site_url(domain, ssl)`.
3. **Los assets (CSS/JS/imágenes) seguían apuntando al dominio local** —
   WordPress los genera desde `siteurl`/`home` en la base de datos, no desde
   el Host de la request. Resuelto con un mu-plugin nuevo,
   `docker/mu-plugins/panel-dynamic-url.php` (inyectado por
   `wordpress::sync_mu_plugins`, igual que mailpit/autologin):
   - Filtra `option_siteurl`/`option_home`/`content_url`/`plugins_url`/oru
     `theme_root_uri` para que usen el host real de la petición en vez del
     guardado en la DB. Respeta `X-Forwarded-Host` — **confirmado que
     `cloudflared` lo pone con el hostname público real del túnel**, aunque
     el Host: que le llega a nginx/WP sea el local (necesario para el punto 2).
     Sin efecto en WP-CLI/cron (sin request HTTP, usan el valor guardado).
   - `WP_CONTENT_URL`/`WP_PLUGIN_URL` se calculan UNA vez al arrancar WP,
     antes de que carguen los mu-plugins, así que no basta con filtrar
     `option_siteurl`: hacen falta también los filtros `content_url`/
     `plugins_url`/`theme_root_uri`, que sí se re-evalúan por request.
   - Media insertada en contenido (`post_content`, `guid` de adjuntos) guarda
     su URL completa **literal** en la base de datos — ningún filtro de
     función la alcanza. Para esos casos, un `ob_start` en `template_redirect`
     (nunca en `wp-admin`) reemplaza el `dominio:puerto` local por el del
     túnel en el HTML final ya renderizado. Gateado en `X-Forwarded-Host`:
     sin túnel de por medio, cero costo en el uso local normal. Reemplaza
     solo `dominio:puerto` (sin esquema) para que funcione igual si la URL
     viene con `/` normales o escapadas (`\/`, común en JSON-LD/atributos
     `data-*` de plugins de galería).

Con los tres fixes, `pgnyc` sirvió por el túnel con **cero** referencias al
dominio local en el HTML — HTML, CSS, JS de plugins/tema, e imágenes de
`wp-content/uploads` cargando en `200` desde la URL pública.

Proyectos creados antes de este mu-plugin no lo tienen instalado: `enable_tunnel`
llama `wordpress::sync_mu_plugins` (idempotente) antes de levantar el túnel.

## Worktree-projects: mismo botón, mismo mu-plugin (verificado en vivo)

El túnel funciona igual para un worktree (`worktree.rs`): la UI no distingue
(«Servicios» siempre muestra el botón), y `enable_tunnel`/`ensure_cloudflared`
no tienen ningún caso especial — usan `site.domain`/`site.services.nginx.ssl`
igual que cualquier sitio. Probado end-to-end contra
`pgnyc · feature/franklinp/sc-8445/blog` (worktree con DB compartida):

- **Bug real encontrado y corregido**: `sync_mu_plugins` escribía en
  `site.public_dir()` propio. Un worktree NO tiene su `public` montado en su
  container — este monta el del PADRE (`docker::create_php_container`) y
  solo sobrepone el `git worktree` del repo objetivo + su `wp-config.php`
  propio. Escribir el mu-plugin en el path del worktree lo dejaba en un
  directorio que el container nunca ve. Fix: `wordpress::mu_plugins_dir()`
  resuelve al `public_dir()` del padre cuando `site.worktree_of` está
  poblado (mismo lookup que ya usa `create_php_container`). Aplica a los
  tres mu-plugins (mailpit, autologin, dynamic-url), no solo al del túnel.
- **Caso adicional de contenido con dominio local propio del worktree**: con
  DB compartida (`shared_db: true`), el worktree fija su dominio por
  constantes `WP_HOME`/`WP_SITEURL` en su wp-config propio (no muta la DB
  del padre). Contenido guardado mientras se editaba con ESE dominio (no el
  del padre) queda con ese dominio literal en la base compartida. El
  reemplazo de media (`template_redirect`) ahora considera esas constantes
  además del valor crudo de `wp_options.home`, no solo uno.
- Resultado verificado: HTML, CSS, JS e imágenes de `wp-content/uploads`
  cargando en 200 por la URL pública, cero referencias a `*.test` en el HTML
  final (ni el dominio del worktree ni el del padre).

## Bug crítico adicional: `redirect_canonical()` rompía la portada por defecto

Verificado con un test de integración real (`integration_tests::tunel_cloudflare_e2e`,
crea un sitio WordPress **recién instalado**, sin personalizar — portada =
listado de posts por defecto, `is_front_page()`): la portada devolvía **301 a
una URL híbrida rota** (`https://{dominio-local}/`, sin puerto, ni el dominio
local completo ni el del túnel). `pgnyc`/el worktree no lo mostraron porque su
portada es una página estática, que no entra a esa rama de
`wp-includes/canonical.php`.

Causa exacta (`redirect_canonical()`, líneas ~612-616): arma el host del
redirect desde `$_SERVER['HTTP_HOST']` (el LOCAL, necesario para que nginx
enrute) pero el PUERTO desde `wp_parse_url(home_url())` (que el mu-plugin deja
sin puerto, porque el túnel es `:443` estándar) — la mezcla de fuentes
produce una URL que no es ninguna de las dos válidas.

Fix: el mu-plugin ahora desactiva `redirect_canonical` por completo mientras
hay `X-Forwarded-Host` (patrón estándar detrás de cualquier reverse proxy que
cambia el dominio visible — la noción de "URL canónica" no aplica cuando el
sitio es accesible por dos dominios a la vez a propósito). Cero efecto local
(sin `X-Forwarded-Host`, el filtro no hace nada).

## Test de integración permanente

`src-tauri/src/integration_tests.rs::tunel_cloudflare_e2e` (`#[ignore]`, correr
con `cargo test -- --ignored --test-threads=1 --exact
integration_tests::tunel_cloudflare_e2e`): crea un sitio real con SSL,
arranca el túnel con el código de producción (`docker::ensure_cloudflared`),
extrae la URL con `cloudflare::extract_url`, resuelve por DoH (el resolver
local del sistema puede tardar/fallar en resolver un subdominio
`*.trycloudflare.com` recién creado — no es un bug del túnel) y verifica 200 +
cero menciones al dominio local en el HTML. Deja este bug (portada por
defecto) cubierto para siempre.

## CLI y MCP

`enable_tunnel`/`disable_tunnel`/`tunnel_status` también se exponen fuera de
la GUI, mismo código detrás (D-Bus → `docker.rs`/`wordpress.rs`, igual que el
comando Tauri):
- **CLI** (`wordpress-panel-cli`, habla por D-Bus con el panel en ejecución):
  `wordpress-panel-cli tunnel {enable|disable|status} [proyecto]`.
- **MCP** (`mcp/server.mjs`, envuelve el CLI): tools `enable_tunnel`,
  `disable_tunnel`, `tunnel_status`.

Verificado en vivo contra el panel real corriendo (AppImage reconstruido):
`tunnel enable` por CLI encendió `cf-{id}`, `tunnel status` devolvió la URL
pública, y Playwright confirmó **el mismo diseño exacto** (mismas dimensiones
de página, mismo layout/colores/imágenes, cero errores de consola) navegando
`https://pgnyc.test:8443/` vs la URL del túnel.

**Bug de aislamiento en los tests, encontrado y corregido en el camino**:
`integration_tests.rs::teardown()` llamaba `docker.stop_site(site, &[])` — el
`&[]` (lista vacía de "otros sitios activos") hacía que
`teardown_unused_shared` apagara `panel-nginx`/DB/mailpit compartidos SIN
chequear si había otros proyectos reales corriendo en la misma máquina.
Preexistente (no introducido por el túnel), afectaba a CUALQUIER test de
este archivo corrido en una máquina con el panel real activo — causó una
interrupción real durante esta verificación (recuperada con
`wordpress-panel-cli start <id>`). Fix: `teardown()` ahora pasa
`config::load_all_sites()` en vez de `&[]`.

---

## Checklist: qué tienes que hacer tú

Nada de cuenta ni configuración en Cloudflare. Solo:

1. Que Docker pueda hacer pull de la imagen `cloudflare/cloudflared` (pull
   normal, público, sin credenciales).
2. Nada más — no hay dominio que dar de alta, no hay login, no hay DNS.

Si en el futuro quieres un dominio fijo en vez de uno temporal por sesión,
eso es el modo *named tunnel* (requiere tu dominio en Cloudflare + login) —
un plan distinto, no incluido aquí a propósito.
