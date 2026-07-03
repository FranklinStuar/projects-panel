<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LocalSite } from '$lib/types';
  import OpConsole from '$lib/components/OpConsole.svelte';

  let localSites = $state<LocalSite[]>([]);
  let loading = $state(true);
  let localError = $state<string | null>(null);
  let msg = $state<string | null>(null);
  let err = $state<string | null>(null);
  let importing = $state<Record<string, boolean>>({});
  let consoleOpen = $state(false);
  let consoleRunning = $state(false);

  async function load() {
    // LocalWP es opcional: su ausencia no es un error de la página.
    try {
      localError = null;
      localSites = await api.listLocalwpSites();
    } catch (e) {
      localError = String(e);
      localSites = [];
    } finally {
      loading = false;
    }
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

  onMount(load);
</script>

<h1 class="mb-1 text-lg font-semibold">Importar desde LocalWP</h1>
<p class="mb-4 text-sm text-zinc-500">
  Copia los archivos y el dump; el sitio queda como "pendiente de migración".
  Enciéndelo con "Migrar y encender" en Proyectos (crea la DB e importa el dump).
</p>

{#if err}
  <div class="mb-3 rounded border border-red-900 bg-red-950 px-3 py-2 text-sm text-red-300">{err}</div>
{/if}
{#if msg}
  <div class="mb-3 whitespace-pre-wrap rounded border border-blue-900 bg-blue-950 px-3 py-2 text-sm text-blue-300">{msg}</div>
{/if}

{#if loading}
  <p class="text-sm text-zinc-500">Cargando…</p>
{:else if localError}
  <p class="rounded border border-zinc-800 px-4 py-3 text-sm text-zinc-500">{localError}</p>
{:else if localSites.length === 0}
  <p class="rounded border border-zinc-800 px-4 py-3 text-sm text-zinc-500">No hay sitios de LocalWP.</p>
{:else}
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

<OpConsole open={consoleOpen} running={consoleRunning} title="Importar desde LocalWP" onClose={() => (consoleOpen = false)} />
