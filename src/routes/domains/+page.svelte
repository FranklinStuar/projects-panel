<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { SiteState } from '$lib/types';

  let sites = $state<SiteState[]>([]);
  onMount(async () => {
    sites = await api.getSites();
  });
</script>

<h1 class="mb-2 text-lg font-semibold">Dominios</h1>
<p class="mb-4 text-sm text-zinc-500">
  Todos los <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">.test</code> resuelven a
  <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">127.0.0.1</code> vía dnsmasq wildcard. No se edita
  <code>/etc/hosts</code> por proyecto.
</p>

<div class="overflow-hidden rounded border border-zinc-200 dark:border-zinc-800">
  {#each sites as s (s.config.id)}
    <div class="flex items-center justify-between border-b border-zinc-100 px-4 py-2 text-sm last:border-0 dark:border-zinc-800/60">
      <span>{s.config.domain}</span>
      <span class="text-xs text-zinc-500">{s.config.services.nginx.ssl ? 'HTTPS' : 'HTTP'}</span>
    </div>
  {/each}
</div>
