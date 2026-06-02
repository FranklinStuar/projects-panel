<script lang="ts">
  // Consola de progreso para operaciones largas (migración, import). Escucha el
  // evento `op-log` del backend (ver src-tauri/src/progress.rs) y muestra las
  // líneas en vivo en un modal. Mientras `running`, el botón Cerrar se desactiva.
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';

  let {
    open = false,
    title = 'Operación',
    running = false,
    cancelable = false,
    onCancel,
    onClose
  }: {
    open?: boolean;
    title?: string;
    running?: boolean;
    // Si `cancelable`, se muestra un botón para abortar la operación en curso
    // (p. ej. la ventana de gracia antes de borrar un proyecto).
    cancelable?: boolean;
    onCancel?: () => void;
    onClose?: () => void;
  } = $props();

  // Carácter (SOH) con que el backend marca una línea de progreso "viva": la
  // reescribimos en sitio en vez de apilarla, así un contador que tickea cada 2s
  // (import) no inunda la consola. Espejo de `PROGRESS_PREFIX` en progress.rs.
  const PROGRESS_PREFIX = '\u0001';

  let lines = $state<string[]>([]);
  let lastIsProgress = false; // ¿la última línea es una línea viva reemplazable?
  let un: UnlistenFn | null = null;
  let wasOpen = false;

  let box = $state<HTMLPreElement | null>(null);

  // Engancha el listener al montar (no al abrir): `listen` es async y la
  // operación —migración/import— arranca su `invoke` en el mismo tick que se abre
  // la consola; si esperáramos a `open` se perderían las primeras líneas.
  onMount(() => {
    listen<string>('op-log', (e) => {
      const payload = e.payload;
      if (payload.startsWith(PROGRESS_PREFIX)) {
        // Línea viva: reemplaza la anterior viva en sitio; si no, apila una nueva.
        const text = payload.slice(PROGRESS_PREFIX.length);
        lines = lastIsProgress
          ? [...lines.slice(0, -1), text]
          : [...lines.slice(-300), text];
        lastIsProgress = true;
      } else {
        lines = [...lines.slice(-300), payload];
        lastIsProgress = false;
      }
    }).then((u) => (un = u));
  });

  // Limpia el buffer al inicio de cada operación (transición cerrado→abierto).
  $effect(() => {
    if (open && !wasOpen) {
      lines = [];
      lastIsProgress = false;
    }
    wasOpen = open;
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
        <div class="flex items-center gap-2">
          {#if cancelable}
            <button
              class="rounded bg-red-600 px-2 py-1 text-sm font-medium text-white hover:bg-red-500"
              onclick={() => onCancel?.()}
            >
              Cancelar borrado
            </button>
          {/if}
          <button
            class="rounded bg-zinc-800 px-2 py-1 text-sm disabled:opacity-40"
            disabled={running}
            onclick={() => onClose?.()}
          >
            Cerrar
          </button>
        </div>
      </div>
      <pre bind:this={box} class="m-0 flex-1 overflow-auto whitespace-pre-wrap px-4 py-2 text-xs leading-relaxed text-zinc-300">{lines.join('\n')}</pre>
    </div>
  </div>
{/if}
