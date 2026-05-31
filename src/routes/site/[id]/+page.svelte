<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { api } from '$lib/api';
  import type { SiteState } from '$lib/types';

  let site = $state<SiteState | null>(null);
  let notFound = $state(false);

  onMount(async () => {
    const all = await api.getSites();
    site = all.find((s) => s.config.id === page.params.id) ?? null;
    notFound = site === null;
  });
</script>

{#if notFound}
  <p class="text-sm text-zinc-500">Proyecto no encontrado.</p>
{:else if site}
  <a href="/" class="mb-4 inline-block text-sm text-blue-500 underline">← Proyectos</a>
  <h1 class="mb-1 text-lg font-semibold">{site.config.name}</h1>
  <p class="mb-4 text-sm text-zinc-500">{site.config.domain}</p>

  <dl class="grid grid-cols-2 gap-2 text-sm">
    <dt class="text-zinc-500">Estado</dt>
    <dd>{site.status}</dd>
    <dt class="text-zinc-500">PHP</dt>
    <dd>{site.config.services.php.version}</dd>
    <dt class="text-zinc-500">Base de datos</dt>
    <dd>{site.config.services.db.type} {site.config.services.db.version}</dd>
    <dt class="text-zinc-500">Ruta</dt>
    <dd class="truncate">{site.config.path}</dd>
  </dl>

  <p class="mt-6 text-xs text-zinc-400">Tabs (Logs, Themes/Plugins, GitHub, Asistente IA): Fase 2+.</p>
{:else}
  <p class="text-sm text-zinc-500">Cargando…</p>
{/if}
