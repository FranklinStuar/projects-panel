<script lang="ts">
  // Wrapper fino para deep-links a un proyecto. El flujo normal es el
  // master-detail embebido de `/`; aquí solo se renderiza el detalle de un id
  // concreto (p. ej. al abrir una URL directa).
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import ProjectDetail from '$lib/components/ProjectDetail.svelte';

  let id = $derived(page.params.id as string);
</script>

{#key id}
  <ProjectDetail
    {id}
    onDeleted={() => goto('/')}
    onSelect={(next) => goto(`/site/${next}`)}
  />
{/key}
