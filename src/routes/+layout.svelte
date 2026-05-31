<script lang="ts">
  import '../app.css';

  let { children } = $props();

  const nav = [
    { href: '/', label: 'Proyectos' },
    { href: '/domains', label: 'Dominios' },
    { href: '/settings', label: 'Configuración' }
  ];
</script>

<div class="flex h-screen overflow-hidden">
  <aside
    class="flex w-52 shrink-0 flex-col border-r border-zinc-200 bg-zinc-100 dark:border-zinc-800 dark:bg-zinc-900"
  >
    <div class="px-4 py-4 text-sm font-semibold tracking-wide">Panel WP</div>
    <nav class="flex flex-col gap-1 px-2">
      {#each nav as item (item.href)}
        <a
          href={item.href}
          class="rounded px-3 py-2 text-sm hover:bg-zinc-200 dark:hover:bg-zinc-800"
        >
          {item.label}
        </a>
      {/each}
    </nav>
  </aside>

  <main class="flex-1 overflow-auto p-6">
    {@render children()}
  </main>
</div>
