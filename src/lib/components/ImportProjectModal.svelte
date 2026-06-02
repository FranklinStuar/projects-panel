<script lang="ts">
  // Re-importar proyectos desconectados: lista las carpetas de ~/panel-wp/ que
  // ya no están en el panel pero siguen en disco (tras borrar conservando la
  // carpeta, o copiadas de otra PC) y permite re-conectarlas. Al importar, el
  // proyecto vuelve como «pendiente de migración»; se enciende luego con
  // «Migrar y encender». Espeja el patrón de "Importar desde LocalWP"
  // (settings) + la consola OpConsole de migración/import.
  import { api } from '$lib/api';
  import type { DisconnectedSite } from '$lib/types';
  import OpConsole from './OpConsole.svelte';

  let {
    open = $bindable(),
    onClose
  }: {
    open: boolean;
    // `imported` = si al menos una carpeta se re-importó (para recargar la lista).
    onClose?: (imported: boolean) => void;
  } = $props();

  let list = $state<DisconnectedSite[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let importing = $state<Record<string, boolean>>({});
  let imported = $state(false);

  let consoleOpen = $state(false);
  let consoleRunning = $state(false);

  // Carga la lista al abrir (transición cerrado→abierto).
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      imported = false;
      refresh();
    }
    wasOpen = open;
  });

  async function refresh() {
    loading = true;
    error = null;
    try {
      list = await api.listDisconnectedSites();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function importOne(s: DisconnectedSite) {
    importing = { ...importing, [s.folderName]: true };
    consoleOpen = true;
    consoleRunning = true;
    try {
      await api.importDisconnectedSite(s.folderName);
      imported = true;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      importing = { ...importing, [s.folderName]: false };
      consoleRunning = false;
    }
  }

  function dismiss() {
    open = false;
    onClose?.(imported);
  }
</script>

{#if open}
  <div class="fixed inset-0 z-40 flex items-center justify-center bg-black/60 p-4">
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Importar proyecto"
      class="flex max-h-[80vh] w-full max-w-lg flex-col rounded-lg border border-zinc-300 bg-white p-5 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
    >
      <div class="mb-1 flex items-center justify-between">
        <h2 class="text-base font-semibold">Importar proyecto</h2>
        <button class="rounded bg-zinc-200 px-2 py-1 text-sm dark:bg-zinc-800" onclick={dismiss}>
          Cerrar
        </button>
      </div>
      <p class="mb-4 text-sm text-zinc-500">
        Carpetas en <code class="text-xs">~/panel-wp/</code> que ya no están en el panel
        (proyectos desconectados, o copiados de otra PC). Al importar, el proyecto
        vuelve como «pendiente de migración»: enciéndelo con «Migrar y encender».
      </p>

      {#if error}
        <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
          {error}
        </div>
      {/if}

      {#if loading}
        <p class="text-sm text-zinc-500">Buscando carpetas…</p>
      {:else if list.length === 0}
        <p class="rounded border border-dashed border-zinc-300 px-4 py-6 text-center text-sm text-zinc-500 dark:border-zinc-700">
          No hay carpetas de proyectos desconectadas en <code class="text-xs">~/panel-wp/</code>.
        </p>
      {:else}
        <div class="overflow-y-auto rounded border border-zinc-200 text-sm dark:border-zinc-800">
          {#each list as s (s.folderName)}
            <div class="flex items-center justify-between border-b border-zinc-100 px-4 py-3 last:border-0 dark:border-zinc-800/60">
              <div class="min-w-0">
                <div class="font-medium">
                  {s.name} <span class="text-xs text-zinc-500">→ {s.domain}</span>
                </div>
                <div class="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs text-zinc-500">
                  <span
                    class="rounded px-1.5 py-0.5"
                    class:bg-green-100={s.kind === 'preserved'}
                    class:text-green-700={s.kind === 'preserved'}
                    class:bg-amber-100={s.kind === 'reconstructed'}
                    class:text-amber-700={s.kind === 'reconstructed'}
                    class:dark:bg-green-950={s.kind === 'preserved'}
                    class:dark:text-green-300={s.kind === 'preserved'}
                    class:dark:bg-amber-950={s.kind === 'reconstructed'}
                    class:dark:text-amber-300={s.kind === 'reconstructed'}
                  >
                    {s.kind === 'preserved' ? 'config conservada' : 'reconstruido'}
                  </span>
                  <span>PHP {s.phpVersion} · {s.dbType} {s.dbVersion}</span>
                  <span>· {s.hasDump ? 'con dump' : 'sin dump'}</span>
                </div>
              </div>
              <button
                class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
                disabled={importing[s.folderName]}
                onclick={() => importOne(s)}
              >
                {importing[s.folderName] ? '…' : 'Importar'}
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<OpConsole
  open={consoleOpen}
  running={consoleRunning}
  title="Importar proyecto"
  onClose={() => (consoleOpen = false)}
/>
