<script lang="ts">
  import { api } from '$lib/api';

  let installing = $state(false);
  let installMsg = $state<string | null>(null);
  let installErr = $state<string | null>(null);

  async function install() {
    installing = true;
    installMsg = null;
    installErr = null;
    try {
      installMsg = await api.installCliWrapper();
    } catch (e) {
      installErr = String(e);
    } finally {
      installing = false;
    }
  }

  // Grupos de comandos. `cmd` se muestra en monoespaciada; `desc` lo explica.
  const groups: { title: string; note?: string; items: { cmd: string; desc: string }[] }[] = [
    {
      title: 'Control del proyecto',
      note: 'start/stop admiten un proyecto por nombre o id; sin argumento usan el del directorio actual.',
      items: [
        { cmd: 'wordpress-panel-cli list', desc: 'Lista TODOS los proyectos con su estado (activo/parado), dominio y grupo.' },
        { cmd: 'wordpress-panel-cli start [proyecto]', desc: 'Enciende el proyecto (containers + nginx + auto-dump).' },
        { cmd: 'wordpress-panel-cli stop [proyecto]', desc: 'Apaga el proyecto y los compartidos que ya nadie use.' },
        { cmd: 'wordpress-panel-cli open admin', desc: 'Abre el wp-admin en el navegador con auto-login.' },
        { cmd: 'wordpress-panel-cli open site', desc: 'Abre la web pública (frontend) del proyecto.' },
        { cmd: 'wordpress-panel-cli open folder', desc: 'Abre la carpeta del proyecto en el explorador.' }
      ]
    },
    {
      title: 'Inspección',
      note: 'containers/resources requieren el panel abierto; logs de php funciona aun sin él.',
      items: [
        { cmd: 'wordpress-panel-cli containers', desc: 'Lista los contenedores del proyecto (php, db, nginx, mailpit, minio) con su estado.' },
        { cmd: 'wordpress-panel-cli resources', desc: 'docker stats (CPU/memoria) de los contenedores del proyecto.' },
        { cmd: 'wordpress-panel-cli logs [servicio] [-f] [-n N]', desc: 'Logs de un contenedor (servicio: php por defecto, o db/nginx/mailpit/minio). -f sigue en vivo, -n limita líneas.' }
      ]
    },
    {
      title: 'Puntos de guardado',
      note: 'Autodetecta el proyecto por el directorio actual.',
      items: [
        { cmd: 'wordpress-panel-cli snapshot list', desc: 'Lista los puntos de guardado (id, etiqueta, fecha, tamaño).' },
        { cmd: 'wordpress-panel-cli snapshot create "<etiqueta>"', desc: 'Crea un punto de guardado (código + dump SQL).' },
        { cmd: 'wordpress-panel-cli snapshot delete <snapshotId>', desc: 'Borra un punto de guardado del disco.' },
        { cmd: 'wordpress-panel-cli snapshot clone <snapshotId>', desc: 'Levanta un clone temporal desde ese punto de guardado.' }
      ]
    },
    {
      title: 'Git / deploy directo',
      note: 'El repo objetivo se infiere del directorio actual; override con --path <ruta rel. a public/>.',
      items: [
        { cmd: 'wordpress-panel-cli git scan', desc: 'Lista los repos git del proyecto (rama, remoto, registrado).' },
        { cmd: 'wordpress-panel-cli git status [--path <p>] [--branch <b>]', desc: 'Estado de la rama vs remoto: fetch + ahead/behind + árbol sucio.' },
        { cmd: 'wordpress-panel-cli git pull [--path <p>] [--branch <b>]', desc: 'git pull del repo.' },
        { cmd: 'wordpress-panel-cli git set-deploy --branch <b> [--build "<cmd>"] [--dirs a,b]', desc: 'Guarda rama, comando de build y carpetas de build para el deploy directo.' },
        { cmd: 'wordpress-panel-cli git deploy [--path <p>]', desc: 'Checkout + git pull --ff-only + build (según lo guardado).' }
      ]
    },
    {
      title: 'Worktrees',
      note: 'Probar una rama de un theme/plugin en aislamiento.',
      items: [
        { cmd: 'wordpress-panel-cli worktree list', desc: 'Lista los worktree-projects del proyecto.' },
        { cmd: 'wordpress-panel-cli worktree create <rama> [--target <ruta>] [--base <rama>] [--copy-db]', desc: 'Crea un worktree sobre una rama nueva (BD compartida salvo --copy-db).' },
        { cmd: 'wordpress-panel-cli worktree remove <id> [--delete-branch]', desc: 'Elimina el worktree (conserva la rama salvo --delete-branch).' }
      ]
    },
    {
      title: 'Utilidad',
      items: [
        { cmd: 'wordpress-panel-cli --help', desc: 'Ayuda completa con todos los grupos y ejemplos.' },
        { cmd: 'wordpress-panel-cli detect-project <ruta>', desc: 'Imprime el id del proyecto que contiene esa ruta.' }
      ]
    }
  ];
</script>

<h1 class="mb-2 text-lg font-semibold">CLI (terminal)</h1>
<p class="mb-4 max-w-2xl text-sm text-zinc-500">
  <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">wordpress-panel-cli</code> maneja
  puntos de guardado, git y worktrees desde la terminal. Habla con el panel
  <strong>en ejecución</strong> por D-Bus (reusa su lógica: containers, nginx, BD), así que
  <strong>el panel debe estar abierto</strong>. Se ejecuta desde la carpeta de un proyecto (o de su repo):
  autodetecta a qué proyecto perteneces.
</p>

<div class="mb-6 flex flex-wrap items-center gap-3 rounded border border-zinc-200 px-3 py-2 dark:border-zinc-800">
  <div class="min-w-0 flex-1 text-sm">
    <div class="font-medium">Instalar / actualizar la CLI</div>
    <div class="text-xs text-zinc-500">
      Copia <code>wordpress-panel-cli</code> y <code>wp</code> a
      <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">~/.local/bin</code>. También se instala sola al arrancar el panel.
    </div>
  </div>
  <button
    class="shrink-0 rounded bg-blue-600 px-3 py-1 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
    disabled={installing}
    onclick={install}
  >
    {installing ? 'Instalando…' : 'Instalar CLI'}
  </button>
</div>

{#if installMsg}
  <pre class="mb-4 overflow-x-auto rounded border border-emerald-300 bg-emerald-50 px-3 py-2 text-xs text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-300">{installMsg}</pre>
{/if}
{#if installErr}
  <div class="mb-4 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{installErr}</div>
{/if}

<p class="mb-4 max-w-2xl text-xs text-zinc-500">
  Si <code>wordpress-panel-cli</code> no se encuentra, añade
  <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">~/.local/bin</code> al PATH:
  <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">export PATH="$HOME/.local/bin:$PATH"</code>.
  El progreso de operaciones largas (snapshot, deploy) sale en la consola del panel; la CLI espera y muestra el resultado final.
</p>

<div class="flex flex-col gap-6">
  {#each groups as g (g.title)}
    <section>
      <h2 class="mb-1 text-sm font-semibold uppercase tracking-wide text-zinc-500">{g.title}</h2>
      {#if g.note}
        <p class="mb-2 text-xs text-zinc-400">{g.note}</p>
      {/if}
      <div class="flex flex-col gap-2">
        {#each g.items as it (it.cmd)}
          <div class="rounded border border-zinc-200 px-3 py-2 dark:border-zinc-800">
            <code class="block overflow-x-auto whitespace-pre text-xs text-zinc-800 dark:text-zinc-100">{it.cmd}</code>
            <div class="mt-1 text-xs text-zinc-500">{it.desc}</div>
          </div>
        {/each}
      </div>
    </section>
  {/each}
</div>
