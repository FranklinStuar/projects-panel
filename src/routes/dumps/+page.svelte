<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DumpLogEntry } from '$lib/types';

  let dumpEntries = $state<DumpLogEntry[]>([]);
  let loading = $state(true);
  let busy = $state(false);
  let msg = $state<string | null>(null);
  let err = $state<string | null>(null);

  let cleanBefore = $state('');
  let cleanDb = $state('');
  let dbNames = $derived([...new Set(dumpEntries.map((e) => e.dbName))]);

  async function load() {
    try {
      err = null;
      dumpEntries = await api.dumpLog();
    } catch (e) {
      err = String(e);
      dumpEntries = [];
    } finally {
      loading = false;
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

  onMount(load);
</script>

<h1 class="mb-1 text-lg font-semibold">Log de volcados de DB</h1>
<p class="mb-4 text-sm text-zinc-500">
  Cada vez que se escribe un dump (automático al cambiar la DB, al detener o manual) queda
  registrado aquí para revisarlo y comparar. La limpieza solo borra el registro, no los
  archivos <code class="rounded bg-zinc-800 px-1">.sql</code>.
</p>

{#if err}
  <div class="mb-3 rounded border border-red-900 bg-red-950 px-3 py-2 text-sm text-red-300">{err}</div>
{/if}
{#if msg}
  <div class="mb-3 whitespace-pre-wrap rounded border border-blue-900 bg-blue-950 px-3 py-2 text-sm text-blue-300">{msg}</div>
{/if}

{#if loading}
  <p class="text-sm text-zinc-500">Cargando…</p>
{:else}
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
{/if}
