<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { page } from '$app/state';
  import { api } from '$lib/api';
  import type { SiteState } from '$lib/types';

  let site = $state<SiteState | null>(null);
  let notFound = $state(false);
  let tab = $state<'info' | 'logs' | 'ext'>('info');
  let error = $state<string | null>(null);
  let busy = $state(false);

  const id = page.params.id;

  async function load() {
    const all = await api.getSites();
    site = all.find((s) => s.config.id === id) ?? null;
    notFound = site === null;
  }

  async function toggle() {
    if (!site) return;
    busy = true;
    error = null;
    try {
      if (site.status === 'running') await api.stopSite(id);
      else await api.startSite(id);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function openAdmin() {
    error = null;
    try {
      await api.openAdmin(id);
    } catch (e) {
      error = String(e);
    }
  }

  // --- Logs en vivo ---------------------------------------------------------
  let logLines = $state<string[]>([]);
  let unlisten: UnlistenFn | null = null;
  let streaming = false;

  async function startLogs() {
    if (streaming) return;
    streaming = true;
    logLines = [];
    unlisten = await listen<string>(`log:${id}`, (ev) => {
      logLines = [...logLines.slice(-500), ev.payload];
    });
    await api.streamLogs(id);
  }
  async function stopLogs() {
    if (!streaming) return;
    streaming = false;
    await api.stopLogs(id).catch(() => {});
    unlisten?.();
    unlisten = null;
  }

  // arranca/para el stream al entrar/salir del tab Logs
  $effect(() => {
    if (tab === 'logs' && site?.status === 'running') startLogs();
    else stopLogs();
  });

  // --- Plugins / Themes -----------------------------------------------------
  let plugins = $state<any[]>([]);
  let themes = $state<any[]>([]);
  let extError = $state<string | null>(null);
  let extLoading = $state(false);

  async function loadExt() {
    if (!site || site.status !== 'running') return;
    extLoading = true;
    extError = null;
    try {
      plugins = JSON.parse((await api.listPlugins(id)) || '[]');
      themes = JSON.parse((await api.listThemes(id)) || '[]');
    } catch (e) {
      extError = String(e);
    } finally {
      extLoading = false;
    }
  }

  $effect(() => {
    if (tab === 'ext') loadExt();
  });

  onMount(load);
  onDestroy(stopLogs);

  const tabs = [
    { id: 'info', label: 'Info' },
    { id: 'logs', label: 'Logs' },
    { id: 'ext', label: 'Plugins / Themes' }
  ] as const;
</script>

{#if notFound}
  <p class="text-sm text-zinc-500">Proyecto no encontrado.</p>
{:else if site}
  <a href="/" class="mb-3 inline-block text-sm text-blue-500 underline">← Proyectos</a>

  <div class="mb-4 flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">{site.config.name}</h1>
      <p class="text-sm text-zinc-500">{site.config.domain}</p>
    </div>
    <div class="flex items-center gap-2">
      {#if site.status === 'running'}
        <button class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500" onclick={openAdmin}>
          Abrir admin
        </button>
      {/if}
      <button
        class="rounded px-3 py-1.5 text-sm font-medium"
        class:bg-green-600={site.status !== 'running'}
        class:text-white={site.status !== 'running'}
        class:bg-zinc-200={site.status === 'running'}
        class:dark:bg-zinc-800={site.status === 'running'}
        disabled={busy}
        onclick={toggle}
      >
        {busy ? '…' : site.status === 'running' ? 'Detener' : 'Encender'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
      {error}
    </div>
  {/if}

  <div class="mb-4 flex gap-1 border-b border-zinc-200 dark:border-zinc-800">
    {#each tabs as t (t.id)}
      <button
        class="border-b-2 px-3 py-2 text-sm"
        class:border-blue-500={tab === t.id}
        class:font-medium={tab === t.id}
        class:border-transparent={tab !== t.id}
        class:text-zinc-500={tab !== t.id}
        onclick={() => (tab = t.id)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  {#if tab === 'info'}
    <dl class="grid grid-cols-2 gap-2 text-sm">
      <dt class="text-zinc-500">Estado</dt>
      <dd>{site.status}</dd>
      <dt class="text-zinc-500">PHP</dt>
      <dd>{site.config.services.php.version}</dd>
      <dt class="text-zinc-500">Base de datos</dt>
      <dd>{site.config.services.db.type} {site.config.services.db.version} · {site.config.services.db.dbName}</dd>
      <dt class="text-zinc-500">SSL</dt>
      <dd>{site.config.services.nginx.ssl ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Auto-login</dt>
      <dd>{site.config.oneClickAdmin ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Ruta</dt>
      <dd class="truncate">{site.config.path}</dd>
    </dl>
  {:else if tab === 'logs'}
    {#if site.status !== 'running'}
      <p class="text-sm text-zinc-500">Enciende el proyecto para ver logs en vivo.</p>
    {:else}
      <pre class="h-96 overflow-auto rounded bg-zinc-900 p-3 text-xs leading-relaxed text-zinc-100">{logLines.join('') || 'Esperando logs…'}</pre>
    {/if}
  {:else if tab === 'ext'}
    {#if site.status !== 'running'}
      <p class="text-sm text-zinc-500">Enciende el proyecto para listar plugins y themes.</p>
    {:else if extLoading}
      <p class="text-sm text-zinc-500">Cargando…</p>
    {:else if extError}
      <p class="text-sm text-red-500">{extError}</p>
    {:else}
      <div class="grid grid-cols-2 gap-6">
        <div>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Plugins</h3>
          <div class="overflow-hidden rounded border border-zinc-200 text-sm dark:border-zinc-800">
            {#each plugins as p (p.name)}
              <div class="flex items-center justify-between border-b border-zinc-100 px-3 py-1.5 last:border-0 dark:border-zinc-800/60">
                <span>{p.name}</span>
                <span class="text-xs" class:text-green-500={p.status === 'active'} class:text-zinc-400={p.status !== 'active'}>{p.status}</span>
              </div>
            {:else}
              <div class="px-3 py-2 text-xs text-zinc-500">Sin plugins</div>
            {/each}
          </div>
        </div>
        <div>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Themes</h3>
          <div class="overflow-hidden rounded border border-zinc-200 text-sm dark:border-zinc-800">
            {#each themes as t (t.name)}
              <div class="flex items-center justify-between border-b border-zinc-100 px-3 py-1.5 last:border-0 dark:border-zinc-800/60">
                <span>{t.name}</span>
                <span class="text-xs" class:text-green-500={t.status === 'active'} class:text-zinc-400={t.status !== 'active'}>{t.status}</span>
              </div>
            {:else}
              <div class="px-3 py-2 text-xs text-zinc-500">Sin themes</div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  {/if}
{:else}
  <p class="text-sm text-zinc-500">Cargando…</p>
{/if}
