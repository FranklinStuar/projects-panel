<script lang="ts">
  // Modal global de confirmación (ver `$lib/confirm.svelte.ts`). Se monta una
  // vez en `+layout.svelte`; Enter acepta, Escape cancela.
  import { tick } from 'svelte';
  import { confirmState, answerConfirm } from '$lib/confirm.svelte';

  let okBtn = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (confirmState.current) tick().then(() => okBtn?.focus());
  });

  function onKey(ev: KeyboardEvent) {
    if (!confirmState.current) return;
    if (ev.key === 'Escape') {
      ev.preventDefault();
      answerConfirm(false);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if confirmState.current}
  {@const req = confirmState.current}
  <div class="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 p-4">
    <div
      role="alertdialog"
      aria-modal="true"
      aria-label="Confirmar"
      class="w-full max-w-md rounded-lg border border-zinc-300 bg-white p-5 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
    >
      <p class="mb-5 whitespace-pre-line text-sm">{req.message}</p>
      <div class="flex justify-end gap-2">
        <button
          class="rounded bg-zinc-200 px-3 py-1.5 text-sm dark:bg-zinc-800"
          onclick={() => answerConfirm(false)}
        >
          Cancelar
        </button>
        <button
          bind:this={okBtn}
          class="rounded px-3 py-1.5 text-sm font-medium text-white {req.danger
            ? 'bg-red-600 hover:bg-red-500'
            : 'bg-blue-600 hover:bg-blue-500'}"
          onclick={() => answerConfirm(true)}
        >
          {req.confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}
