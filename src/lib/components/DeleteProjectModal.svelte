<script lang="ts">
  // Borrado de un proyecto en dos fases:
  //  1. Confirmación: modal con el NOMBRE del proyecto + checkbox para borrar
  //     también la carpeta del disco (si no, solo se desconecta del panel).
  //  2. Ejecución: consola en vivo (OpConsole) con una ventana de gracia de 5 s
  //     —"Preparando proceso de eliminación"— y un botón «Cancelar borrado». Si
  //     no se cancela, llama a `delete_site` (que emite sus pasos) y al terminar
  //     habilita «Cerrar».
  //
  // Las líneas de la ventana de gracia se emiten por el mismo canal `op-log` que
  // usa el backend, así la consola las muestra igual que en migración/import.
  import { tick } from 'svelte';
  import { emit } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import type { SiteState } from '$lib/types';
  import OpConsole from './OpConsole.svelte';

  let {
    site = $bindable(),
    onClose
  }: {
    // Proyecto a borrar; `null` = modal cerrado. El componente lo pone a `null`
    // al cerrar.
    site: SiteState | null;
    // Se llama al cerrar la consola; `deleted` indica si llegó a borrarse.
    onClose?: (deleted: boolean) => void;
  } = $props();

  // Espejo de `PROGRESS_PREFIX` en OpConsole/progress.rs: línea "viva" que se
  // reescribe en sitio (el contador de la cuenta atrás).
  const PROGRESS_PREFIX = '';
  const GRACE_SECONDS = 5;
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

  let phase = $state<'confirm' | 'deleting'>('confirm');
  let deleteFolder = $state(false);
  let running = $state(false);
  let cancelable = $state(false);
  let deleted = $state(false);
  let cancelRequested = false;

  // Reinicia el estado al abrir para un proyecto nuevo (null → proyecto).
  let prevId: string | null = null;
  $effect(() => {
    const id = site?.config.id ?? null;
    if (id && id !== prevId) {
      phase = 'confirm';
      deleteFolder = false;
      deleted = false;
      cancelRequested = false;
    }
    prevId = id;
  });

  function dismiss() {
    const wasDeleted = deleted;
    site = null;
    phase = 'confirm';
    onClose?.(wasDeleted);
  }

  async function start() {
    if (!site) return;
    const target = site;
    phase = 'deleting';
    running = true;
    cancelable = true;
    cancelRequested = false;
    // Espera al flush para que OpConsole limpie su buffer (efecto al abrir)
    // antes de emitir la primera línea.
    await tick();

    await emit('op-log', `▶ Eliminando «${target.config.name}».`);
    await emit('op-log', 'Preparando proceso de eliminación…');

    for (let i = GRACE_SECONDS; i > 0; i--) {
      await emit(
        'op-log',
        `${PROGRESS_PREFIX}Eliminando en ${i} s…  (pulsa «Cancelar borrado» para abortar)`
      );
      await sleep(1000);
      if (cancelRequested) break;
    }

    if (cancelRequested) {
      await emit('op-log', '✗ Borrado cancelado. No se tocó nada.');
      cancelable = false;
      running = false;
      return;
    }

    // Punto de no retorno: ya no se puede cancelar.
    cancelable = false;
    try {
      await api.deleteSite(target.config.id, deleteFolder);
      await emit(
        'op-log',
        deleteFolder
          ? '✓ Proyecto eliminado: datos y carpeta borrados.'
          : '✓ Proyecto eliminado: datos borrados; carpeta conservada y desconectada del panel.'
      );
      deleted = true;
    } catch (e) {
      await emit('op-log', `✗ Error al eliminar: ${e}`);
    } finally {
      running = false;
    }
  }

  function requestCancel() {
    cancelRequested = true;
  }
</script>

{#if site && phase === 'confirm'}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Eliminar ${site.config.name}`}
      class="w-full max-w-md rounded-lg border border-zinc-300 bg-white p-5 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
    >
      <h2 class="mb-1 text-base font-semibold">Eliminar «{site.config.name}»</h2>
      <p class="mb-4 text-sm text-zinc-500">
        Se borrarán todos los datos del proyecto (base de datos y contenedores).
        Esta acción no se puede deshacer.
      </p>
      <label class="mb-5 flex items-start gap-2 text-sm">
        <input type="checkbox" class="mt-0.5" bind:checked={deleteFolder} />
        <span>
          Borrar también la carpeta del proyecto en disco
          <span class="mt-0.5 block break-all text-xs text-zinc-500">{site.config.path}</span>
          <span class="mt-0.5 block text-xs text-zinc-500">
            Si lo dejas sin marcar, la carpeta se conserva y solo se desconecta del
            panel (podrás reconfigurarla más tarde).
          </span>
        </span>
      </label>
      <div class="flex justify-end gap-2">
        <button
          class="rounded bg-zinc-200 px-3 py-1.5 text-sm dark:bg-zinc-800"
          onclick={dismiss}
        >
          Cancelar
        </button>
        <button
          class="rounded bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-500"
          onclick={start}
        >
          Eliminar
        </button>
      </div>
    </div>
  </div>
{/if}

<OpConsole
  open={!!site && phase === 'deleting'}
  title={site ? `Eliminar «${site.config.name}»` : 'Eliminar'}
  {running}
  {cancelable}
  onCancel={requestCancel}
  onClose={dismiss}
/>
