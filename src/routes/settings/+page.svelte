<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SystemStatus, LocalSite, DumpLogEntry } from '$lib/types';
  import OpConsole from '$lib/components/OpConsole.svelte';

  let status = $state<SystemStatus | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let msg = $state<string | null>(null);
  let err = $state<string | null>(null);

  // Import LocalWP.
  let localSites = $state<LocalSite[]>([]);
  let localError = $state<string | null>(null);
  let importing = $state<Record<string, boolean>>({});
  let consoleOpen = $state(false);
  let consoleRunning = $state(false);

  // Log de volcados de DB.
  let dumpEntries = $state<DumpLogEntry[]>([]);
  let cleanBefore = $state('');
  let cleanDb = $state('');
  let dbNames = $derived([...new Set(dumpEntries.map((e) => e.dbName))]);

  async function load() {
    try {
      err = null;
      status = await api.systemStatus();
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
    // LocalWP es opcional: su ausencia no es un error de la página.
    try {
      localError = null;
      localSites = await api.listLocalwpSites();
    } catch (e) {
      localError = String(e);
      localSites = [];
    }
    try {
      dumpEntries = await api.dumpLog();
    } catch {
      dumpEntries = [];
    }
  }

  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function fmtDate(iso: string): string {
    return iso.replace('T', ' ').replace('Z', ' UTC');
  }

  const SOURCE_LABEL: Record<string, string> = {
    auto: 'automático',
    stop: 'al detener',
    manual: 'manual',
  };

  async function cleanByDate() {
    if (!cleanBefore) return;
    if (!confirm(`Borrar del log las entradas anteriores a ${cleanBefore}? (no borra los .sql)`)) return;
    await run(async () => {
      const n = await api.cleanDumpLog(cleanBefore, null);
      return `${n} entradas borradas del log.`;
    });
  }

  async function cleanByDb() {
    if (!cleanDb) return;
    if (!confirm(`Borrar del log todas las entradas de "${cleanDb}"? (no borra los .sql)`)) return;
    await run(async () => {
      const n = await api.cleanDumpLog(null, cleanDb);
      return `${n} entradas borradas del log.`;
    });
  }

  async function cleanAll() {
    if (!confirm('Borrar TODO el log de volcados? (no borra los .sql)')) return;
    await run(async () => {
      const n = await api.cleanDumpLog(null, null);
      return `${n} entradas borradas del log.`;
    });
  }

  async function importLocal(s: LocalSite) {
    importing = { ...importing, [s.id]: true };
    err = null;
    msg = null;
    consoleOpen = true;
    consoleRunning = true;
    try {
      const r = await api.importLocalwpSite(s.id);
      msg = `Importado "${r.site.name}" → ${r.site.domain}. ${r.note ?? ''} Usa "Migrar y encender" en Proyectos.`;
      await load();
    } catch (e) {
      err = String(e);
    } finally {
      importing = { ...importing, [s.id]: false };
      consoleRunning = false;
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
    </div>
  </section>

  <!-- Log de volcados de DB -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Log de volcados de DB</h2>
    <p class="mb-2 text-xs text-zinc-500">
      Cada vez que se escribe un dump (automático al cambiar la DB, al detener o manual) queda
      registrado aquí para revisarlo y comparar. La limpieza solo borra el registro, no los
      archivos <code class="rounded bg-zinc-800 px-1">.sql</code>.
    </p>

    <!-- Controles de limpieza -->
    <div class="mb-2 flex flex-wrap items-end gap-2 rounded border border-zinc-800 px-4 py-3 text-sm">
      <label class="flex flex-col gap-1">
        <span class="text-xs text-zinc-500">Anteriores a</span>
        <input type="date" bind:value={cleanBefore} class="rounded bg-zinc-900 px-2 py-1 text-sm" />
      </label>
      <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={busy || !cleanBefore} onclick={cleanByDate}>
        Borrar por fecha
      </button>

      <label class="ml-2 flex flex-col gap-1">
        <span class="text-xs text-zinc-500">Base de datos</span>
        <select bind:value={cleanDb} class="rounded bg-zinc-900 px-2 py-1 text-sm">
          <option value="">— elige —</option>
          {#each dbNames as db (db)}
            <option value={db}>{db}</option>
          {/each}
        </select>
      </label>
      <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={busy || !cleanDb} onclick={cleanByDb}>
        Borrar por base
      </button>

      <button class="ml-auto rounded bg-red-900/60 px-3 py-1.5 text-red-200 disabled:opacity-50" disabled={busy || dumpEntries.length === 0} onclick={cleanAll}>
        Borrar todo
      </button>
    </div>

    {#if dumpEntries.length === 0}
      <p class="rounded border border-zinc-800 px-4 py-3 text-sm text-zinc-500">Aún no hay volcados registrados.</p>
    {:else}
      <div class="overflow-hidden rounded border border-zinc-800 text-sm">
        {#each dumpEntries as d (d.timestamp + d.file)}
          <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-2 last:border-0">
            <div class="min-w-0">
              <div class="font-medium">
                {d.siteName} <span class="text-xs text-zinc-500">· {d.dbName}</span>
              </div>
              <div class="truncate text-xs text-zinc-500" title={d.file}>{d.file}</div>
            </div>
            <div class="ml-3 shrink-0 text-right text-xs text-zinc-500">
              <div>{fmtDate(d.timestamp)}</div>
              <div>{fmtBytes(d.bytes)} · {SOURCE_LABEL[d.source] ?? d.source}</div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <!-- Import LocalWP -->
  <section class="mb-6">
    <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Importar desde LocalWP</h2>
    {#if localError}
      <p class="rounded border border-zinc-800 px-4 py-3 text-sm text-zinc-500">{localError}</p>
    {:else if localSites.length === 0}
      <p class="rounded border border-zinc-800 px-4 py-3 text-sm text-zinc-500">No hay sitios de LocalWP.</p>
    {:else}
      <p class="mb-2 text-xs text-zinc-500">
        Copia los archivos y el dump; el sitio queda como "pendiente de migración".
        Enciéndelo con "Migrar y encender" en Proyectos (crea la DB e importa el dump).
      </p>
      <div class="overflow-hidden rounded border border-zinc-800 text-sm">
        {#each localSites as s (s.id)}
          <div class="flex items-center justify-between border-b border-zinc-800/60 px-4 py-3 last:border-0">
            <div>
              <div class="font-medium">{s.name} <span class="text-xs text-zinc-500">→ {s.domain}</span></div>
              <div class="text-xs text-zinc-500">
                PHP {s.phpVersion} · MySQL {s.dbVersion}{s.multisite ? ' · multisite' : ''}{s.xdebug ? ' · xdebug' : ''}
              </div>
            </div>
            {#if s.alreadyImported}
              <span class="text-xs text-zinc-500">Ya importado</span>
            {:else}
              <button class="rounded bg-zinc-800 px-3 py-1.5 disabled:opacity-50" disabled={importing[s.id]}
                onclick={() => importLocal(s)}>{importing[s.id] ? '…' : 'Importar'}</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<OpConsole open={consoleOpen} running={consoleRunning} title="Importar desde LocalWP" onClose={() => (consoleOpen = false)} />
