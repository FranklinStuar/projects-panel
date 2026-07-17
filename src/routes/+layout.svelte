<script lang="ts">
  import '../app.css';
  import { page } from '$app/state';

  let { children } = $props();

  // Riel de íconos: alterna entre las secciones del panel. Proyectos es el
  // master-detail (lista + detalle); el resto son páginas sueltas.
  const nav = [
    {
      href: '/',
      label: 'Proyectos',
      // carpeta / sitios
      icon: 'M2 5.5A1.5 1.5 0 0 1 3.5 4h4l1.5 1.5h7.5A1.5 1.5 0 0 1 18 7v9.5A1.5 1.5 0 0 1 16.5 18h-13A1.5 1.5 0 0 1 2 16.5v-11Z'
    },
    {
      href: '/domains',
      label: 'Dominios',
      // globo
      icon: 'M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16Zm5.3 5h-2.2a12.6 12.6 0 0 0-1-2.5A6 6 0 0 1 15.3 7ZM10 4c.6.8 1.1 1.8 1.4 3H8.6c.3-1.2.8-2.2 1.4-3ZM4.3 12a6 6 0 0 1 0-4h2.5a14 14 0 0 0 0 4H4.3Zm.4 1h2.2c.3.9.6 1.7 1 2.5A6 6 0 0 1 4.7 13Zm2.2-6H4.7a6 6 0 0 1 3.2-2.5c-.4.8-.7 1.6-1 2.5ZM10 16c-.6-.8-1.1-1.8-1.4-3h2.8c-.3 1.2-.8 2.2-1.4 3Zm1.7-4H8.3a12.4 12.4 0 0 1 0-4h3.4a12.4 12.4 0 0 1 0 4Zm.4 3.5c.4-.8.7-1.6 1-2.5h2.2a6 6 0 0 1-3.2 2.5ZM13.2 12a14 14 0 0 0 0-4h2.5a6 6 0 0 1 0 4h-2.5Z'
    },
    {
      href: '/services',
      label: 'Servicios',
      // cubos / servicios
      icon: 'M9.6 2.3a1 1 0 0 1 .8 0l5.5 2.4a1 1 0 0 1 0 1.83l-5.5 2.4a1 1 0 0 1-.8 0L4.1 6.53a1 1 0 0 1 0-1.83l5.5-2.4ZM3.5 9.1l5.5 2.4v5.2l-5.5-2.4a1 1 0 0 1-.6-.92V9.1Zm13 0v4.28a1 1 0 0 1-.6.92L11 16.7v-5.2l5.5-2.4Z'
    },
    {
      href: '/import-localwp',
      label: 'Importar desde LocalWP',
      // bandeja con flecha de descarga
      icon: 'M10 2a1 1 0 0 1 1 1v6.6l1.8-1.8a1 1 0 1 1 1.4 1.4l-3.5 3.5a1 1 0 0 1-1.4 0L5.8 9.2a1 1 0 1 1 1.4-1.4L9 9.6V3a1 1 0 0 1 1-1ZM4 13a1 1 0 0 1 1 1v1.5a.5.5 0 0 0 .5.5h9a.5.5 0 0 0 .5-.5V14a1 1 0 1 1 2 0v1.5A2.5 2.5 0 0 1 14.5 18h-9A2.5 2.5 0 0 1 3 15.5V14a1 1 0 0 1 1-1Z'
    },
    {
      href: '/dumps',
      label: 'Log de volcados de DB',
      // cilindro de base de datos
      icon: 'M10 2c3.3 0 6 1.1 6 2.5S13.3 7 10 7 4 5.9 4 4.5 6.7 2 10 2Zm6 4.7v3C16 11.1 13.3 12 10 12s-6-.9-6-2.3v-3C5.3 7.8 7.5 8.5 10 8.5s4.7-.7 6-1.8Zm0 5v3.8C16 16.9 13.3 18 10 18s-6-1.1-6-2.5v-3.8c1.3 1.1 3.5 1.8 6 1.8s4.7-.7 6-1.8Z'
    },
    {
      href: '/cli',
      label: 'CLI (terminal)',
      // prompt de terminal ">_"
      icon: 'M4.9 6.2a1.1 1.1 0 0 0-1.5 1.6L6 10l-2.6 2.2a1.1 1.1 0 0 0 1.5 1.6l3.5-3a1.1 1.1 0 0 0 0-1.6l-3.5-3ZM10 12.4a1 1 0 0 0 0 2h5a1 1 0 1 0 0-2h-5Z'
    },
    {
      href: '/settings',
      label: 'Configuración',
      // engranaje
      icon: 'M8.3 2.6a1 1 0 0 1 1-.6h1.4a1 1 0 0 1 1 .6l.4 1.3 1.3.7 1.3-.4a1 1 0 0 1 1.1.4l.7 1.2a1 1 0 0 1-.1 1.2l-.9 1v1.5l.9 1a1 1 0 0 1 .1 1.2l-.7 1.2a1 1 0 0 1-1.1.4l-1.3-.4-1.3.7-.4 1.3a1 1 0 0 1-1 .6H9.3a1 1 0 0 1-1-.6l-.4-1.3-1.3-.7-1.3.4a1 1 0 0 1-1.1-.4l-.7-1.2a1 1 0 0 1 .1-1.2l.9-1V9l-.9-1a1 1 0 0 1-.1-1.2l.7-1.2a1 1 0 0 1 1.1-.4l1.3.4 1.3-.7.4-1.3ZM10 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z'
    }
  ];

  function isActive(href: string): boolean {
    const path = page.url.pathname;
    if (href === '/') return path === '/' || path.startsWith('/site');
    return path === href || path.startsWith(href + '/');
  }

  // El master-detail de Proyectos (/) gestiona su propio layout a pantalla
  // completa; el resto de páginas usan el padding estándar.
  let padded = $derived(page.url.pathname !== '/');
</script>

<div class="flex h-screen overflow-hidden">
  <aside
    class="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-zinc-200 bg-zinc-100 py-3 dark:border-zinc-800 dark:bg-zinc-900"
  >
    <nav class="flex flex-col items-center gap-1" aria-label="Secciones">
      {#each nav as item (item.href)}
        <a
          href={item.href}
          title={item.label}
          aria-label={item.label}
          aria-current={isActive(item.href) ? 'page' : undefined}
          class="flex h-10 w-10 items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-200 hover:text-zinc-800 dark:hover:bg-zinc-800 dark:hover:text-zinc-100
                 aria-[current=page]:bg-blue-600 aria-[current=page]:text-white aria-[current=page]:hover:bg-blue-500 aria-[current=page]:hover:text-white"
        >
          <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path d={item.icon} />
          </svg>
        </a>
      {/each}
    </nav>

    <div class="mt-auto">
      <a
        href="/site/new"
        title="Nuevo proyecto"
        aria-label="Nuevo proyecto"
        class="flex h-10 w-10 items-center justify-center rounded-full bg-blue-600 text-white hover:bg-blue-500"
      >
        <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
          <path d="M10 4a1 1 0 0 1 1 1v4h4a1 1 0 1 1 0 2h-4v4a1 1 0 1 1-2 0v-4H5a1 1 0 1 1 0-2h4V5a1 1 0 0 1 1-1Z" />
        </svg>
      </a>
    </div>
  </aside>

  <main class="flex-1 overflow-auto" class:p-6={padded}>
    {@render children()}
  </main>
</div>
