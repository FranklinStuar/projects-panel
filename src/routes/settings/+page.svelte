<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SystemStatus } from '$lib/types';

  let status = $state<SystemStatus | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let msg = $state<string | null>(null);
  let err = $state<string | null>(null);

  async function load() {
    try {
      err = null;
      status = await api.systemStatus();
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }

  async function run(fn: () => Promise<unknown>, ok = 'Listo') {
    busy = true;
    err = null;
    msg = null;
    try {
      const r = await fn();
      msg = typeof r === 'string' && r ? r : ok;
      await load();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function resetEndpoint() {
    if (
      !confirm(
        'Reasignar el puerto del panel. Solo afecta a proyectos creados después; ' +
          'los ya instalados guardan su URL con el puerto actual. Se reasigna en el ' +
          'próximo arranque de panel-nginx (apaga todos los proyectos primero). ¿Continuar?'
      )
    )
      return;
    await run(() => api.resetEndpoint(), 'Endpoint reiniciado — reasignará puerto al próximo arranque.');
  }

  // Endpoint con URLs limpias = 127.0.0.1 en puertos estándar.
  let cleanUrls = $derived(
    !!status &&
      status.endpoint.loopbackIp === '127.0.0.1' &&
      status.endpoint.httpPort === 80 &&
      status.endpoint.httpsPort === 443
  );

  const FIRST_RUN = 'bash scripts/first-run.sh';

  onMount(load);
</script>

<h1 class="mb-1 text-lg font-semibold">Configuración</h1>
<p class="mb-4 text-sm text-zinc-500">
  Estado del sistema y primera configuración. Las acciones que requieren privilegios
  (dnsmasq, CA de mkcert, plasmoid) se ejecutan con
  <code class="rounded bg-zinc-800 px-1">{FIRST_RUN}</code>.
</p>

{#if err}
  <div class="mb-3 rounded border border-red-900 bg-red-950 px-3 py-2 text-sm text-red-300">{err}</div>
{/if}
{#if msg}
  <div class="mb-3 whitespace-pre-wrap rounded border border-blue-900 bg-blue-950 px-3 py-2 text-sm text-blue-300">{msg}</div>
{/if}

{#if loading}
  <p class="text-sm text-zinc-500">Cargando…</p>
{:else if status}
  <!-- Checklist de prerequisitos -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Sistema</h2>
    <div class="overflow-hidden rounded border border-zinc-800 text-sm">
      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.dockerOk} class:bg-red-500={!status.dockerOk}></span>
          <div><div class="font-medium">Docker</div><div class="text-xs text-zinc-500">Daemon de Docker accesible</div></div>
        </div>
      </div>

      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.networkOk} class:bg-red-500={!status.networkOk}></span>
          <div><div class="font-medium">Red panel-net</div><div class="text-xs text-zinc-500">Bridge Docker compartido</div></div>
        </div>
        {#if !status.networkOk}
          <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={busy}
            onclick={() => run(() => api.createPanelNetwork(), 'Red panel-net creada')}>Crear red</button>
        {/if}
      </div>

      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.dnsmasqOk} class:bg-red-500={!status.dnsmasqOk}></span>
          <div><div class="font-medium">dnsmasq *.test</div><div class="text-xs text-zinc-500">Resolución de *.test ({FIRST_RUN})</div></div>
        </div>
      </div>

      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.mkcertOk} class:bg-red-500={!status.mkcertOk}></span>
          <div><div class="font-medium">CA de mkcert</div><div class="text-xs text-zinc-500">Certificados SSL .test de confianza ({FIRST_RUN})</div></div>
        </div>
      </div>

      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.cliWrapperOk} class:bg-red-500={!status.cliWrapperOk}></span>
          <div><div class="font-medium">Wrappers WP-CLI</div><div class="text-xs text-zinc-500">wp / wordpress-panel-cli en ~/.local/bin</div></div>
        </div>
        <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={busy}
          onclick={() => run(() => api.installCliWrapper())}>{status.cliWrapperOk ? 'Reinstalar' : 'Instalar'}</button>
      </div>

      <div class="flex items-center justify-between px-4 py-3">
        <div class="flex items-center gap-3">
          <span class="inline-block h-2.5 w-2.5 rounded-full" class:bg-green-500={status.plasmoidOk} class:bg-red-500={!status.plasmoidOk}></span>
          <div><div class="font-medium">Plasmoid KDE</div><div class="text-xs text-zinc-500">Widget del panel ({FIRST_RUN})</div></div>
        </div>
      </div>
    </div>
  </section>

  <!-- Endpoint -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Punto de publicación</h2>
    <div class="flex items-center justify-between rounded border border-zinc-800 px-4 py-3 text-sm">
      <div>
        <div class="font-medium">
          {status.endpoint.loopbackIp}:{status.endpoint.httpPort}/{status.endpoint.httpsPort}
          {#if cleanUrls}
            <span class="ml-2 rounded bg-green-900/40 px-1.5 py-0.5 text-xs text-green-400">URLs limpias</span>
          {:else}
            <span class="ml-2 rounded bg-amber-900/40 px-1.5 py-0.5 text-xs text-amber-400">puerto alterno</span>
          {/if}
        </div>
        <div class="text-xs text-zinc-500">Dónde publica panel-nginx en el host (se elige una vez y se mantiene).</div>
      </div>
      <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={busy} onclick={resetEndpoint}>
        Reasignar puerto
      </button>
    </div>
  </section>

  <!-- Rutas -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Rutas</h2>
    <div class="rounded border border-zinc-800 text-sm">
      <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3">
        <span class="text-zinc-500">Proyectos</span>
        <code class="text-xs">{status.projectsRoot}</code>
      </div>
      <div class="flex items-center justify-between px-4 py-3">
        <span class="text-zinc-500">Configuración</span>
        <code class="text-xs">{status.configDir}</code>
      </div>
    </div>
    <p class="mt-2 text-xs text-zinc-500">Tema: navy oscuro (sin selector claro/oscuro).</p>
  </section>

  <!-- Herramientas de mantenimiento -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Mantenimiento</h2>
    <div class="overflow-hidden rounded border border-zinc-800 text-sm">
      <div class="flex items-center justify-between px-4 py-3">
        <div>
          <div class="font-medium">Regenerar php.ini en todos los proyectos</div>
          <div class="text-xs text-zinc-500">
            Aplica la configuración actual del panel (OPcache, límites, etc.) a todos los proyectos.
            Reinicia los proyectos encendidos para que surta efecto.
          </div>
        </div>
        <button
          class="shrink-0 rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50"
          disabled={busy}
          onclick={() => run(() => api.repairAllPhpIni(), 'php.ini regenerado')}
        >
          Aplicar a todos
        </button>
      </div>
      <div class="flex items-center justify-between border-t border-zinc-800 px-4 py-3">
        <div>
          <div class="font-medium">Reparar nginx</div>
          <div class="text-xs text-zinc-500">
            Si ningún sitio carga tras un apagón sucio: poda vhosts huérfanos (de proyectos cuyo
            container ya no corre) y reinicia el proxy nginx compartido.
          </div>
        </div>
        <button
          class="shrink-0 rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50"
          disabled={busy}
          onclick={() => run(() => api.repairNginx(), 'nginx reparado')}
        >
          Reparar nginx
        </button>
      </div>
    </div>
  </section>

{/if}
