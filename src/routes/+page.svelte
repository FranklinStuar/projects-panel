<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SiteState, Endpoint } from '$lib/types';
  import ProjectDetail from '$lib/components/ProjectDetail.svelte';
  import ImportProjectModal from '$lib/components/ImportProjectModal.svelte';

  let sites = $state<SiteState[]>([]);
  let endpoint = $state<Endpoint | null>(null);
  let persistedGroups = $state<string[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let stoppingAll = $state(false);

  // Proyecto seleccionado → su detalle se muestra embebido en el panel grande.
  let selectedId = $state<string | null>(null);
  // Modal "Importar proyecto" (carpetas desconectadas en ~/panel-wp/).
  let importOpen = $state(false);
  // Alta de grupo inline.
  let addingGroup = $state(false);
  let newGroupName = $state('');
  // Drag & drop de proyectos entre grupos.
  let dragId = $state<string | null>(null);
  let dragOverGroup = $state<string | null>(null);

  const UNGROUPED = 'Sin grupo';
  const COLLAPSE_KEY = 'wp-panel:collapsed-groups';

  // Grupos contraídos (persistido en localStorage para que el usuario muestre
  // solo los que le interesan).
  let collapsed = $state<Record<string, boolean>>({});
  try {
    const raw = localStorage.getItem(COLLAPSE_KEY);
    if (raw) collapsed = JSON.parse(raw);
  } catch {}

  function toggleCollapse(name: string) {
    collapsed = { ...collapsed, [name]: !collapsed[name] };
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed));
    } catch {}
  }

  async function load() {
    try {
      error = null;
      [sites, endpoint, persistedGroups] = await Promise.all([
        api.getSites(),
        api.panelEndpoint(),
        api.listGroups()
      ]);
      // Si el proyecto seleccionado ya no existe, deselecciona.
      if (selectedId && !sites.some((s) => s.config.id === selectedId)) selectedId = null;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Etiqueta de host con puerto solo si el panel publica en uno alterno.
  function hostLabel(s: SiteState): string {
    if (!endpoint) return s.config.domain;
    const ssl = s.config.services.nginx.ssl;
    const port = ssl ? endpoint.httpsPort : endpoint.httpPort;
    const std = ssl ? 443 : 80;
    return port === std ? s.config.domain : `${s.config.domain}:${port}`;
  }

  let runningCount = $derived(sites.filter((s) => s.status === 'running').length);

  async function toggle(s: SiteState) {
    busy = { ...busy, [s.config.id]: true };
    error = null;
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

  async function stopAll() {
    stoppingAll = true;
    error = null;
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

  async function addGroup() {
    const name = newGroupName.trim();
    if (!name) {
      addingGroup = false;
      return;
    }
    try {
      await api.createGroup(name);
      newGroupName = '';
      addingGroup = false;
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  // Suelta el proyecto arrastrado en `group` (cadena vacía = sin grupo).
  async function dropOnGroup(group: string) {
    const id = dragId;
    dragId = null;
    dragOverGroup = null;
    if (!id) return;
    const target = group === UNGROUPED ? '' : group;
    const site = sites.find((s) => s.config.id === id);
    if (site && (site.config.group ?? '') === target) return; // sin cambio
    try {
      await api.setSiteGroup(id, target || null);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  // Proyectos encendidos: se fijan al inicio de la lista (sección "En ejecución")
  // para no buscarlos entre todos. No se duplican: salen de su grupo mientras
  // están encendidos y vuelven al pararse.
  let runningSites = $derived(sites.filter((s) => s.status === 'running'));

  // Agrupar los proyectos NO encendidos por grupo, con clones anidados bajo su
  // padre. El orden de los grupos lo da groups.json; los grupos persistidos sin
  // proyectos aparecen vacíos (drop target). "Sin grupo" siempre al final.
  type Node = { site: SiteState; clones: SiteState[] };
  let groups = $derived.by<[string, Node[]][]>(() => {
    const stopped = sites.filter((s) => s.status !== 'running');
    const stoppedIds = new Set(stopped.map((s) => s.config.id));

    const clonesByParent = new Map<string, SiteState[]>();
    for (const s of stopped) {
      const pid = s.config.cloneOf?.parentId;
      if (!pid) continue;
      if (!clonesByParent.has(pid)) clonesByParent.set(pid, []);
      clonesByParent.get(pid)!.push(s);
    }

    const map = new Map<string, Node[]>();
    for (const s of stopped) {
      const pid = s.config.cloneOf?.parentId;
      // Clone anidado solo si su padre también está parado (si no, se muestra
      // suelto en su grupo, porque el padre está en "En ejecución").
      if (pid && stoppedIds.has(pid)) continue;
      const key = s.config.group ?? UNGROUPED;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push({ site: s, clones: clonesByParent.get(s.config.id) ?? [] });
    }

    // Orden: grupos persistidos (incl. vacíos), luego grupos sueltos detectados.
    const names: string[] = [];
    for (const g of persistedGroups) if (!names.includes(g)) names.push(g);
    for (const k of map.keys()) if (k !== UNGROUPED && !names.includes(k)) names.push(k);

    const out: [string, Node[]][] = names.map((n) => [n, map.get(n) ?? []]);
    if (map.has(UNGROUPED)) out.push([UNGROUPED, map.get(UNGROUPED)!]);
    return out;
  });

  onMount(load);
</script>

<div class="flex h-full">
  <!-- Columna lista de proyectos -->
  <div class="flex w-64 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
    <div class="flex items-center justify-between gap-2 px-3 py-3">
      <h1 class="text-sm font-semibold tracking-wide">Proyectos</h1>
      <div class="flex items-center gap-1">
        <button
          class="flex h-7 w-7 items-center justify-center rounded text-zinc-500 hover:bg-zinc-200 hover:text-zinc-800 disabled:opacity-30 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
          title="Agregar grupo"
          aria-label="Agregar grupo"
          onclick={() => { addingGroup = true; newGroupName = ''; }}
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path d="M3.5 4A1.5 1.5 0 0 0 2 5.5v9A1.5 1.5 0 0 0 3.5 16H10a1 1 0 0 0 0-2H4V8h12v2a1 1 0 1 0 2 0V7a1.5 1.5 0 0 0-1.5-1.5H9L7.5 4h-4Zm12 7a1 1 0 0 1 1 1v2h2a1 1 0 1 1 0 2h-2v2a1 1 0 1 1-2 0v-2h-2a1 1 0 1 1 0-2h2v-2a1 1 0 0 1 1-1Z" />
          </svg>
        </button>
        <button
          class="relative flex h-7 w-7 items-center justify-center rounded text-zinc-500 hover:bg-zinc-200 hover:text-zinc-800 disabled:opacity-30 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
          title={runningCount === 0 ? 'No hay proyectos encendidos' : `Apagar todo (${runningCount})`}
          aria-label="Apagar todo"
          disabled={runningCount === 0 || stoppingAll}
          onclick={stopAll}
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path d="M10 2a1 1 0 0 1 1 1v7a1 1 0 1 1-2 0V3a1 1 0 0 1 1-1Zm-4 3.3a1 1 0 0 1 .1 1.4 5 5 0 1 0 7.8 0 1 1 0 1 1 1.5-1.3 7 7 0 1 1-10.9 0 1 1 0 0 1 1.4-.1Z" />
          </svg>
          {#if runningCount > 0 && !stoppingAll}
            <span class="absolute -right-0.5 -top-0.5 inline-flex h-4 min-w-[1rem] items-center justify-center rounded-full bg-green-500 px-1 text-[10px] font-semibold text-white">{runningCount}</span>
          {/if}
        </button>
      </div>
    </div>

    {#if addingGroup}
      <div class="px-3 pb-2">
        <input
          class="w-full rounded border border-zinc-300 px-2 py-1 text-sm dark:border-zinc-700 dark:bg-zinc-900"
          placeholder="Nombre del grupo…"
          bind:value={newGroupName}
          onkeydown={(e) => { if (e.key === 'Enter') addGroup(); if (e.key === 'Escape') addingGroup = false; }}
          onblur={addGroup}
        />
      </div>
    {/if}

    {#if error}
      <div class="mx-3 mb-2 rounded border border-red-300 bg-red-50 px-2 py-1.5 text-xs text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{error}</div>
    {/if}

    <div class="flex-1 overflow-auto px-2 pb-2" data-testid="project-list">
      {#if loading}
        <p class="px-2 text-sm text-zinc-500">Cargando…</p>
      {:else if sites.length === 0 && groups.length === 0}
        <p class="px-2 text-sm text-zinc-500">No hay proyectos. Crea uno con el botón <strong>+</strong> del riel.</p>
      {:else}
        <!-- Encendidos al inicio, fijos (no se duplican: salen de su grupo). -->
        {#if runningSites.length > 0}
          <section class="mb-2">
            <h2 class="flex items-center gap-1.5 px-2 pb-1 pt-2 text-xs font-semibold uppercase tracking-wide text-green-600 dark:text-green-500">
              <span class="inline-block h-2 w-2 rounded-full bg-green-500"></span>
              En ejecución
            </h2>
            {#each runningSites as s (s.config.id)}
              {@render siteRow(s, false)}
            {/each}
          </section>
        {/if}

        {#each groups as [name, nodes] (name)}
          <section
            class="mb-2 rounded {dragOverGroup === name ? 'bg-blue-500/10 ring-1 ring-blue-500/40' : ''}"
            role="group"
            ondragover={(e) => { if (dragId) { e.preventDefault(); dragOverGroup = name; } }}
            ondragleave={() => { if (dragOverGroup === name) dragOverGroup = null; }}
            ondrop={(e) => { e.preventDefault(); dropOnGroup(name); }}
          >
            <h2 class="px-1 pb-1 pt-2">
              <button
                class="flex w-full items-center gap-1 text-xs font-semibold uppercase tracking-wide text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300"
                aria-expanded={!collapsed[name]}
                onclick={() => toggleCollapse(name)}
              >
                <svg class="h-3 w-3 shrink-0 transition-transform {collapsed[name] ? '-rotate-90' : ''}" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                  <path d="M5.5 7.5 10 12l4.5-4.5H5.5Z" />
                </svg>
                <span class="truncate">{name}</span>
                {#if nodes.length > 0}<span class="ml-auto text-[10px] font-normal text-zinc-400">{nodes.length}</span>{/if}
              </button>
            </h2>
            {#if !collapsed[name]}
              {#if nodes.length === 0}
                <p class="px-2 pb-1 text-xs italic text-zinc-400">Arrastra proyectos aquí</p>
              {/if}
              {#each nodes as node (node.site.config.id)}
                {@render siteRow(node.site, false)}
                {#each node.clones as c (c.config.id)}
                  {@render siteRow(c, true)}
                {/each}
              {/each}
            {/if}
          </section>
        {/each}
      {/if}
    </div>

    <div class="flex items-center justify-between border-t border-zinc-200 px-3 py-2 text-xs text-zinc-500 dark:border-zinc-800">
      <span class="flex items-center gap-1.5">
        <span class="inline-block h-2 w-2 rounded-full" class:bg-green-500={runningCount > 0} class:bg-zinc-400={runningCount === 0}></span>
        {runningCount} encendido{runningCount === 1 ? '' : 's'}
      </span>
      <button class="hover:text-zinc-700 hover:underline dark:hover:text-zinc-300" onclick={() => (importOpen = true)}>Importar proyecto</button>
    </div>
  </div>

  <!-- Panel detalle -->
  <div class="flex-1 overflow-auto p-6">
    {#if selectedId}
      {#key selectedId}
        <ProjectDetail
          id={selectedId}
          onChanged={load}
          onDeleted={() => { selectedId = null; load(); }}
          onSelect={(id) => { selectedId = id; }}
        />
      {/key}
    {:else}
      <div class="flex h-full items-center justify-center text-center text-sm text-zinc-500">
        <div>
          <p class="mb-1">Selecciona un proyecto de la lista.</p>
          <p class="text-xs">O crea uno nuevo con el botón <strong>+</strong> del riel.</p>
        </div>
      </div>
    {/if}
  </div>
</div>

<ImportProjectModal bind:open={importOpen} onClose={(imported) => imported && load()} />

{#snippet siteRow(s: SiteState, isClone: boolean)}
  <div
    class="group flex items-center gap-2 rounded px-2 py-1.5 {selectedId === s.config.id ? 'bg-blue-600/15 dark:bg-blue-500/20' : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/60'}"
    class:ml-4={isClone}
    role="button"
    tabindex="0"
    draggable={!s.config.cloneOf}
    ondragstart={(e) => { if (!s.config.cloneOf) { dragId = s.config.id; e.dataTransfer?.setData('text/plain', s.config.id); } }}
    ondragend={() => { dragId = null; dragOverGroup = null; }}
    onclick={() => (selectedId = s.config.id)}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectedId = s.config.id; } }}
  >
    {#if isClone}
      <span class="text-zinc-300 dark:text-zinc-600" aria-hidden="true">└</span>
    {/if}
    <span
      class="inline-block h-2 w-2 shrink-0 rounded-full"
      class:bg-green-500={s.status === 'running'}
      class:bg-zinc-400={s.status === 'stopped'}
      class:bg-amber-500={s.status === 'migrationPending'}
    ></span>
    <span class="min-w-0 flex-1 truncate text-sm" class:font-medium={selectedId === s.config.id}>{s.config.name}</span>
    {#if s.config.cloneOf}
      <span class="shrink-0 rounded border border-amber-400 px-1 text-[10px] font-medium text-amber-500">C</span>
    {:else if s.config.worktreeOf}
      <span class="shrink-0 rounded border border-violet-400 px-1 text-[10px] font-medium text-violet-500">W</span>
    {/if}

    {#if s.status !== 'migrationPending'}
      <button
        class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-zinc-400 opacity-0 hover:bg-zinc-200 hover:text-zinc-700 focus:opacity-100 group-hover:opacity-100 disabled:opacity-40 dark:hover:bg-zinc-700 dark:hover:text-zinc-100 {s.status === 'running' ? 'opacity-100 text-green-500' : ''}"
        title={s.status === 'running' ? 'Detener' : 'Encender'}
        aria-label={s.status === 'running' ? 'Detener' : 'Encender'}
        disabled={busy[s.config.id]}
        onclick={(e) => { e.stopPropagation(); toggle(s); }}
      >
        {#if busy[s.config.id]}
          <span class="text-xs">…</span>
        {:else if s.status === 'running'}
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><rect x="5" y="5" width="10" height="10" rx="1.5" /></svg>
        {:else}
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path d="M6 4.5v11a1 1 0 0 0 1.5.87l9-5.5a1 1 0 0 0 0-1.74l-9-5.5A1 1 0 0 0 6 4.5Z" /></svg>
        {/if}
      </button>
    {/if}
  </div>
{/snippet}
