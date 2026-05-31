<script lang="ts">
  // Consola de progreso para operaciones largas (migración, import). Escucha el
  // evento `op-log` del backend (ver src-tauri/src/progress.rs) y muestra las
  // líneas en vivo en un modal. Mientras `running`, el botón Cerrar se desactiva.
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';

  let {
    open = false,
    title = 'Operación',
    running = false,
    onClose
  }: { open?: boolean; title?: string; running?: boolean; onClose?: () => void } = $props();

  let lines = $state<string[]>([]);
  let un: UnlistenFn | null = null;
  let listening = false;
  let box = $state<HTMLPreElement | null>(null);

  $effect(() => {
    if (open && !listening) {
      listening = true;
      lines = [];
      listen<string>('op-log', (e) => {
        lines = [...lines.slice(-300), e.payload];
      }).then((u) => (un = u));
    } else if (!open && listening) {
      listening = false;
      un?.();
      un = null;
    }
  });

  // Autoscroll al final cuando llegan líneas.
  $effect(() => {
    lines.length;
    if (box) box.scrollTop = box.scrollHeight;
  });

  onDestroy(() => un?.());
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
    <div class="flex h-[70vh] w-full max-w-2xl flex-col rounded-lg border border-zinc-700 bg-zinc-950 shadow-xl">
      <div class="flex items-center justify-between border-b border-zinc-800 px-4 py-2">
        <span class="text-sm font-medium">
          {title}
          {#if running}<span class="ml-1 text-zinc-500">en curso…</span>{:else}<span class="ml-1 text-green-400">listo</span>{/if}
        </span>
        <button
          class="rounded bg-zinc-800 px-2 py-1 text-sm disabled:opacity-40"
          disabled={running}
          onclick={() => onClose?.()}
        >
          Cerrar
        </button>
      </div>
      <pre bind:this={box} class="m-0 flex-1 overflow-auto whitespace-pre-wrap px-4 py-2 text-xs leading-relaxed text-zinc-300">{lines.join('\n')}</pre>
    </div>
  </div>
{/if}
