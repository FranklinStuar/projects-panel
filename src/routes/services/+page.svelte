<script lang="ts">
  import { api } from '$lib/api';

  let msg = $state<string | null>(null);
  let err = $state<string | null>(null);
  let busy = $state(false);

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    err = null;
    msg = null;
    try {
      const r = await fn();
      if (typeof r === 'string') msg = r;
      else msg = 'OK';
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h1 class="mb-1 text-lg font-semibold">Servicios compartidos</h1>
<p class="mb-4 text-sm text-zinc-500">
  Servicios on-demand para todos los proyectos. Solo corren mientras hay un proyecto activo
  que los necesite (0 recursos en reposo).
</p>

{#if err}
  <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{err}</div>
{/if}
{#if msg}
  <div class="mb-3 whitespace-pre-wrap rounded border border-blue-300 bg-blue-50 px-3 py-2 text-sm text-blue-700 dark:border-blue-900 dark:bg-blue-950 dark:text-blue-300">{msg}</div>
{/if}

<div class="flex flex-col gap-3 text-sm">
  <div class="flex items-center justify-between rounded border border-zinc-200 px-4 py-3 dark:border-zinc-800">
    <div>
      <div class="font-medium">Mailpit</div>
      <div class="text-xs text-zinc-500">Captura el correo saliente de todos los proyectos · 127.0.0.1:8025</div>
    </div>
    <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
      disabled={busy} onclick={() => run(() => api.openMailpit())}>Abrir</button>
  </div>

  <div class="flex items-center justify-between rounded border border-zinc-200 px-4 py-3 dark:border-zinc-800">
    <div>
      <div class="font-medium">MinIO (S3 local)</div>
      <div class="text-xs text-zinc-500">Consola 127.0.0.1:9101 · API 127.0.0.1:9100 · usuario/clave: panel / panel-secret</div>
    </div>
    <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
      disabled={busy} onclick={() => run(() => api.openMinio())}>Abrir consola</button>
  </div>

  <div class="flex items-center justify-between rounded border border-zinc-200 px-4 py-3 dark:border-zinc-800">
    <div>
      <div class="font-medium">Terminal WP-CLI</div>
      <div class="text-xs text-zinc-500">Instala <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">wp</code> en ~/.local/bin para usarlo dentro de cada proyecto</div>
    </div>
    <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
      disabled={busy} onclick={() => run(() => api.installCliWrapper())}>Instalar</button>
  </div>
</div>
