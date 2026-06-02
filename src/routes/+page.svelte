<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SiteState, Endpoint } from '$lib/types';
  import OpConsole from '$lib/components/OpConsole.svelte';

  let sites = $state<SiteState[]>([]);
  let endpoint = $state<Endpoint | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let stoppingAll = $state(false);

  async function stopAll() {
    stoppingAll = true;
    error = null;
    // Marca como ocupados todos los encendidos para feedback inmediato.
    busy = { ...busy, ...Object.fromEntries(sites.filter((s) => s.status === 'running').map((s) => [s.config.id, true])) };
    try {
      await api.stopAllSites();
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = {};
      stoppingAll = false;
    }
  }

  // Consola de progreso de migración.
  let consoleOpen = $state(false);
  let migrating = $state(false);

  // Etiqueta de host con puerto solo si el panel publica en uno alterno.
  function hostLabel(s: SiteState): string {
    if (!endpoint) return s.config.domain;
    const ssl = s.config.services.nginx.ssl;
    const port = ssl ? endpoint.httpsPort : endpoint.httpPort;
    const std = ssl ? 443 : 80;
    return port === std ? s.config.domain : `${s.config.domain}:${port}`;
  }

  async function load() {
    try {
      error = null;
      [sites, endpoint] = await Promise.all([api.getSites(), api.panelEndpoint()]);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function toggle(s: SiteState) {
    busy = { ...busy, [s.config.id]: true };
    try {
      if (s.status === 'running') await api.stopSite(s.config.id);
      else await api.startSite(s.config.id);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = { ...busy, [s.config.id]: false };
    }
  }

  async function cancelImport(s: SiteState) {
    if (!confirm(`Cancelar la importación de "${s.config.name}"? Se borrará su carpeta:\n${s.config.path}`))
      return;
    busy = { ...busy, [s.config.id]: true };
    error = null;
    try {
      await api.deleteSite(s.config.id);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = { ...busy, [s.config.id]: false };
    }
  }

  async function migrate(s: SiteState) {
    if (!confirm(`Migrar "${s.config.name}" a este sistema (crear DB, importar dump, regenerar SSL) y encender?`))
      return;
    busy = { ...busy, [s.config.id]: true };
    error = null;
    consoleOpen = true;
    migrating = true;
    try {
      const r = await api.migrateSite(s.config.id);
      if (r.note) error = r.note; // aviso informativo (p. ej. sin dump)
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = { ...busy, [s.config.id]: false };
      migrating = false;
    }
  }

  // Cuántos proyectos están encendidos (señal en "Apagar todo").
  let runningCount = $derived(sites.filter((s) => s.status === 'running').length);

  // Agrupar por grupo (null = "Sin grupo")
  let groups = $derived.by(() => {
    const map = new Map<string, SiteState[]>();
    for (const s of sites) {
      const key = s.config.group ?? 'Sin grupo';
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(s);
    }
    return [...map.entries()];
  });

  onMount(load);
</script>

<div class="mb-4 flex items-center justify-between">
  <h1 class="text-lg font-semibold">Proyectos</h1>
  <div class="flex gap-2">
    <button
      class="flex items-center gap-1.5 rounded bg-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-300 disabled:opacity-40 disabled:hover:bg-zinc-200 dark:bg-zinc-800 dark:hover:bg-zinc-700 dark:disabled:hover:bg-zinc-800"
      disabled={runningCount === 0 || stoppingAll}
      title={runningCount === 0 ? 'No hay proyectos encendidos' : `${runningCount} encendido${runningCount === 1 ? '' : 's'}`}
      onclick={stopAll}
    >
      {stoppingAll ? 'Apagando…' : 'Apagar todo'}
      {#if runningCount > 0 && !stoppingAll}
        <span class="inline-flex h-5 min-w-[1.25rem] items-center justify-center rounded-full bg-green-500 px-1.5 text-xs font-semibold text-white">
          {runningCount}
        </span>
      {/if}
    </button>
    <a
      href="/site/new"
      class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500"
    >
      Nuevo proyecto
    </a>
  </div>
</div>

{#if error}
  <div class="mb-4 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
    {error}
  </div>
{/if}

{#if loading}
  <p class="text-sm text-zinc-500">Cargando…</p>
{:else if sites.length === 0}
  <div class="rounded border border-dashed border-zinc-300 p-8 text-center text-sm text-zinc-500 dark:border-zinc-700">
    No hay proyectos todavía. Crea uno con <a href="/site/new" class="text-blue-500 underline">Nuevo proyecto</a>.
  </div>
{:else}
  {#each groups as [name, items] (name)}
    <section class="mb-6">
      <h2 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">{name}</h2>
      <div class="overflow-hidden rounded border border-zinc-200 dark:border-zinc-800">
        {#each items as s (s.config.id)}
          <div class="flex items-center justify-between border-b border-zinc-100 px-4 py-3 last:border-0 dark:border-zinc-800/60">
            <div class="flex items-center gap-3">
              <span
                class="inline-block h-2.5 w-2.5 rounded-full"
                class:bg-green-500={s.status === 'running'}
                class:bg-zinc-400={s.status === 'stopped'}
                class:bg-amber-500={s.status === 'migrationPending'}
              ></span>
              <a href={`/site/${s.config.id}`} class="font-medium hover:underline">{s.config.name}</a>
              <span class="text-xs text-zinc-500">{hostLabel(s)}</span>
              <span class="text-xs text-zinc-400">
                PHP {s.config.services.php.version} · {s.config.services.db.type} {s.config.services.db.version}
              </span>
            </div>
            {#if s.status === 'migrationPending'}
              <div class="flex items-center gap-2">
                <button
                  class="rounded px-3 py-1.5 text-sm text-zinc-400 hover:text-red-400 disabled:opacity-50"
                  disabled={busy[s.config.id]}
                  onclick={() => cancelImport(s)}
                >
                  Cancelar
                </button>
                <button
                  class="rounded bg-amber-600 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
                  disabled={busy[s.config.id]}
                  onclick={() => migrate(s)}
                >
                  {busy[s.config.id] ? '…' : 'Migrar y encender'}
                </button>
              </div>
            {:else}
              <button
                class="rounded px-3 py-1.5 text-sm font-medium"
                class:bg-green-600={s.status !== 'running'}
                class:text-white={s.status !== 'running'}
                class:bg-zinc-200={s.status === 'running'}
                class:dark:bg-zinc-800={s.status === 'running'}
                disabled={busy[s.config.id]}
                onclick={() => toggle(s)}
              >
                {busy[s.config.id] ? '…' : s.status === 'running' ? 'Detener' : 'Encender'}
              </button>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/each}
{/if}

<OpConsole open={consoleOpen} running={migrating} title="Migración" onClose={() => (consoleOpen = false)} />
