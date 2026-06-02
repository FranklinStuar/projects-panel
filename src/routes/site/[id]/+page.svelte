<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import type { SiteState, GhStatus, DetectedRepo, Endpoint } from '$lib/types';
  import OpConsole from '$lib/components/OpConsole.svelte';
  import DeleteProjectModal from '$lib/components/DeleteProjectModal.svelte';

  let site = $state<SiteState | null>(null);
  let endpoint = $state<Endpoint | null>(null);
  let notFound = $state(false);
  let tab = $state<'info' | 'logs' | 'ext' | 'github' | 'svc'>('info');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let consoleOpen = $state(false);
  let migrating = $state(false);

  const id = page.params.id as string;

  // Etiqueta de host con puerto solo si el panel publica en uno alterno.
  function hostLabel(s: SiteState): string {
    if (!endpoint) return s.config.domain;
    const ssl = s.config.services.nginx.ssl;
    const port = ssl ? endpoint.httpsPort : endpoint.httpPort;
    const std = ssl ? 443 : 80;
    return port === std ? s.config.domain : `${s.config.domain}:${port}`;
  }

  async function load() {
    const [all, ep] = await Promise.all([api.getSites(), api.panelEndpoint()]);
    endpoint = ep;
    site = all.find((s) => s.config.id === id) ?? null;
    notFound = site === null;
  }

  async function toggle() {
    if (!site) return;
    busy = true;
    error = null;
    try {
      if (site.status === 'running') await api.stopSite(id);
      else await api.startSite(id);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function migrate() {
    if (!site) return;
    if (!confirm(`Migrar "${site.config.name}" a este sistema (crear DB, importar dump, regenerar SSL) y encender?`))
      return;
    busy = true;
    error = null;
    consoleOpen = true;
    migrating = true;
    try {
      const r = await api.migrateSite(id);
      if (r.note) error = r.note;
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
      migrating = false;
    }
  }

  async function cancelImport() {
    if (!site) return;
    if (!confirm(`Cancelar la importación de "${site.config.name}"? Se borrará su carpeta:\n${site.config.path}`))
      return;
    busy = true;
    error = null;
    try {
      await api.deleteSite(id, true);
      await goto('/');
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  // Proyecto en proceso de borrado (abre el modal de confirmación + consola).
  let deleteTarget = $state<SiteState | null>(null);

  async function act(fn: () => Promise<unknown>) {
    error = null;
    try {
      await fn();
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function openAdmin() {
    error = null;
    try {
      await api.openAdmin(id);
    } catch (e) {
      error = String(e);
    }
  }

  async function openSite() {
    error = null;
    try {
      await api.openSite(id);
    } catch (e) {
      error = String(e);
    }
  }

  async function openFolder() {
    error = null;
    try {
      await api.openFolder(id);
    } catch (e) {
      error = String(e);
    }
  }

  // --- Logs en vivo ---------------------------------------------------------
  let logLines = $state<string[]>([]);
  let unlisten: UnlistenFn | null = null;
  let streaming = false;

  async function startLogs() {
    if (streaming) return;
    streaming = true;
    logLines = [];
    unlisten = await listen<string>(`log:${id}`, (ev) => {
      logLines = [...logLines.slice(-500), ev.payload];
    });
    await api.streamLogs(id);
  }
  async function stopLogs() {
    if (!streaming) return;
    streaming = false;
    await api.stopLogs(id).catch(() => {});
    unlisten?.();
    unlisten = null;
  }

  // arranca/para el stream al entrar/salir del tab Logs
  $effect(() => {
    if (tab === 'logs' && site?.status === 'running') startLogs();
    else stopLogs();
  });

  // --- Plugins / Themes -----------------------------------------------------
  let plugins = $state<any[]>([]);
  let themes = $state<any[]>([]);
  let extError = $state<string | null>(null);
  let extLoading = $state(false);

  async function loadExt() {
    if (!site || site.status !== 'running') return;
    extLoading = true;
    extError = null;
    try {
      plugins = JSON.parse((await api.listPlugins(id)) || '[]');
      themes = JSON.parse((await api.listThemes(id)) || '[]');
    } catch (e) {
      extError = String(e);
    } finally {
      extLoading = false;
    }
  }

  $effect(() => {
    if (tab === 'ext') loadExt();
  });

  // Reinyecta el mu-plugin de auto-login (proyectos importados de LocalWP no lo
  // traen, así que su admin no auto-loguea como los creados en el panel).
  let repairing = $state(false);
  async function repairAutologin() {
    if (!site) return;
    repairing = true;
    error = null;
    try {
      await api.repairAutologin(id);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      repairing = false;
    }
  }

  // --- GitHub / Git ---------------------------------------------------------
  let gh = $state<GhStatus | null>(null);
  let ghError = $state<string | null>(null);
  let ghBusy = $state(false);
  // Clonar repo nuevo
  let cloneRepo = $state('');
  let cloneBranch = $state('main');
  let cloneKind = $state<'theme' | 'plugin' | 'muplugin'>('plugin');
  let clonePath = $state(''); // ruta custom opcional (relativa a public/)
  // Repos git detectados en disco (registrados + huérfanos)
  let detected = $state<DetectedRepo[]>([]);

  async function loadGh() {
    try {
      gh = await api.ghStatus();
    } catch (e) {
      ghError = String(e);
    }
    await scanRepos();
  }

  async function scanRepos() {
    try {
      detected = await api.ghScan(id);
    } catch (e) {
      ghError = String(e);
    }
  }

  async function ghAction(fn: () => Promise<unknown>) {
    ghBusy = true;
    ghError = null;
    try {
      await fn();
      await load();
      await scanRepos();
    } catch (e) {
      ghError = String(e);
    } finally {
      ghBusy = false;
    }
  }

  let orphans = $derived(detected.filter((r) => !r.registered));

  $effect(() => {
    if (tab === 'github' && gh === null) loadGh();
  });

  onMount(load);
  onDestroy(stopLogs);

  // --- Servicios (Fase 3) ---------------------------------------------------
  let svcMsg = $state<string | null>(null);
  let svcErr = $state<string | null>(null);
  let svcBusy = $state(false);

  async function svcAction(fn: () => Promise<unknown>) {
    svcBusy = true;
    svcErr = null;
    svcMsg = null;
    try {
      const r = await fn();
      if (typeof r === 'string') svcMsg = r;
      await load();
    } catch (e) {
      svcErr = String(e);
    } finally {
      svcBusy = false;
    }
  }

  const tabs = [
    { id: 'info', label: 'Info' },
    { id: 'logs', label: 'Logs' },
    { id: 'ext', label: 'Plugins / Themes' },
    { id: 'github', label: 'GitHub' },
    { id: 'svc', label: 'Servicios' }
  ] as const;
</script>

{#if notFound}
  <p class="text-sm text-zinc-500">Proyecto no encontrado.</p>
{:else if site}
  <a href="/" class="mb-3 inline-block text-sm text-blue-500 underline">← Proyectos</a>

  <div class="mb-4 flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold">{site.config.name}</h1>
      <p class="text-sm text-zinc-500">{hostLabel(site)}</p>
    </div>
    <div class="flex items-center gap-2">
      {#if site.status === 'migrationPending'}
        <button
          class="rounded px-3 py-1.5 text-sm text-zinc-400 hover:text-red-400 disabled:opacity-50"
          disabled={busy}
          onclick={cancelImport}
        >
          Cancelar
        </button>
        <button
          class="rounded bg-amber-600 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
          disabled={busy}
          onclick={migrate}
        >
          {busy ? '…' : 'Migrar y encender'}
        </button>
      {:else}
        {#if site.status === 'running'}
          <button class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500" onclick={openSite}>
            Abrir web
          </button>
          <button class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500" onclick={openAdmin}>
            Abrir admin
          </button>
        {/if}
        <button
          class="rounded bg-zinc-200 px-3 py-1.5 text-sm font-medium dark:bg-zinc-800"
          onclick={openFolder}
        >
          Abrir carpeta
        </button>
        <button
          class="rounded px-3 py-1.5 text-sm font-medium"
          class:bg-green-600={site.status !== 'running'}
          class:text-white={site.status !== 'running'}
          class:bg-zinc-200={site.status === 'running'}
          class:dark:bg-zinc-800={site.status === 'running'}
          disabled={busy}
          onclick={toggle}
        >
          {busy ? '…' : site.status === 'running' ? 'Detener' : 'Encender'}
        </button>
        <button
          class="rounded px-3 py-1.5 text-sm font-medium text-zinc-400 hover:text-red-500 disabled:opacity-50"
          disabled={busy}
          title="Eliminar proyecto"
          onclick={() => (deleteTarget = site)}
        >
          Eliminar
        </button>
      {/if}
    </div>
  </div>

  {#if error}
    <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
      {error}
    </div>
  {/if}

  <div class="mb-4 flex gap-1 border-b border-zinc-200 dark:border-zinc-800">
    {#each tabs as t (t.id)}
      <button
        class="border-b-2 px-3 py-2 text-sm"
        class:border-blue-500={tab === t.id}
        class:font-medium={tab === t.id}
        class:border-transparent={tab !== t.id}
        class:text-zinc-500={tab !== t.id}
        onclick={() => (tab = t.id)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  {#if tab === 'info'}
    <dl class="grid grid-cols-2 gap-2 text-sm">
      <dt class="text-zinc-500">Estado</dt>
      <dd>{site.status}</dd>
      <dt class="text-zinc-500">PHP</dt>
      <dd>{site.config.services.php.version}</dd>
      <dt class="text-zinc-500">Base de datos</dt>
      <dd>{site.config.services.db.type} {site.config.services.db.version} · {site.config.services.db.dbName}</dd>
      <dt class="text-zinc-500">SSL</dt>
      <dd>{site.config.services.nginx.ssl ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Auto-login</dt>
      <dd>{site.config.oneClickAdmin ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Headless</dt>
      <dd>{site.config.headless ? (site.config.frontendFramework ?? 'Sí') : 'No'}</dd>
      <dt class="text-zinc-500">MinIO</dt>
      <dd>{site.config.minio ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Ruta</dt>
      <dd class="truncate">{site.config.path}</dd>
    </dl>

    <div class="mt-6 flex flex-wrap items-end gap-3 border-t border-zinc-200 pt-4 dark:border-zinc-800">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-zinc-500">Grupo</span>
        <div class="flex gap-2">
          <input
            class="rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900"
            placeholder="Sin grupo"
            value={site.config.group ?? ''}
            onchange={(ev) => act(() => api.setSiteGroup(id, (ev.target as HTMLInputElement).value))}
          />
        </div>
      </label>
      {#if site.config.services.nginx.ssl}
        <button
          class="rounded bg-zinc-200 px-3 py-1.5 text-sm dark:bg-zinc-800"
          onclick={() => act(() => api.regenerateSsl(id))}
        >
          Regenerar SSL
        </button>
      {/if}
    </div>
  {:else if tab === 'logs'}
    {#if site.status !== 'running'}
      <p class="text-sm text-zinc-500">Enciende el proyecto para ver logs en vivo.</p>
    {:else}
      <pre class="h-96 overflow-auto rounded bg-zinc-900 p-3 text-xs leading-relaxed text-zinc-100">{logLines.join('') || 'Esperando logs…'}</pre>
    {/if}
  {:else if tab === 'ext'}
    <!-- Auto-login: reinyectar el mu-plugin si falta (típico tras importar de LocalWP) -->
    <div class="mb-4 flex flex-wrap items-center justify-between gap-2 rounded border border-zinc-200 px-3 py-2 text-sm dark:border-zinc-800">
      <span class="text-zinc-500">
        ¿No inicia sesión sola en el admin? Reinyecta el plugin de auto-login del panel.
      </span>
      <button
        class="rounded bg-zinc-200 px-3 py-1.5 font-medium disabled:opacity-50 dark:bg-zinc-800"
        disabled={repairing}
        onclick={repairAutologin}
      >
        {repairing ? '…' : 'Reparar auto-login'}
      </button>
    </div>
    {#if site.status !== 'running'}
      <p class="text-sm text-zinc-500">Enciende el proyecto para listar plugins y themes.</p>
    {:else if extLoading}
      <p class="text-sm text-zinc-500">Cargando…</p>
    {:else if extError}
      <p class="text-sm text-red-500">{extError}</p>
    {:else}
      <div class="grid grid-cols-2 gap-6">
        <div>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Plugins</h3>
          <div class="overflow-hidden rounded border border-zinc-200 text-sm dark:border-zinc-800">
            {#each plugins as p (p.name)}
              <div class="flex items-center justify-between border-b border-zinc-100 px-3 py-1.5 last:border-0 dark:border-zinc-800/60">
                <span>{p.name}</span>
                <span class="text-xs" class:text-green-500={p.status === 'active'} class:text-zinc-400={p.status !== 'active'}>{p.status}</span>
              </div>
            {:else}
              <div class="px-3 py-2 text-xs text-zinc-500">Sin plugins</div>
            {/each}
          </div>
        </div>
        <div>
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Themes</h3>
          <div class="overflow-hidden rounded border border-zinc-200 text-sm dark:border-zinc-800">
            {#each themes as t (t.name)}
              <div class="flex items-center justify-between border-b border-zinc-100 px-3 py-1.5 last:border-0 dark:border-zinc-800/60">
                <span>{t.name}</span>
                <span class="text-xs" class:text-green-500={t.status === 'active'} class:text-zinc-400={t.status !== 'active'}>{t.status}</span>
              </div>
            {:else}
              <div class="px-3 py-2 text-xs text-zinc-500">Sin themes</div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  {:else if tab === 'github'}
    <!-- Editar en VSCode: no requiere gh ni sesión de GitHub -->
    <div class="mb-4 flex items-center justify-between gap-3 rounded border border-zinc-200 px-3 py-2 dark:border-zinc-800">
      <div class="min-w-0 text-sm">
        <div class="font-medium">Editar en VSCode</div>
        <div class="text-xs text-zinc-500">Abre un workspace con <code>public/</code> como carpeta principal y cada repo git detectado como adicional. Se crea una vez; luego puedes editarlo a mano.</div>
      </div>
      <button class="shrink-0 rounded bg-blue-600 px-3 py-1 text-sm font-medium text-white disabled:opacity-50" disabled={ghBusy}
        onclick={() => ghAction(() => api.openVscode(id))}>Abrir en VSCode</button>
    </div>

    {#if ghError}
      <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{ghError}</div>
    {/if}

    <!-- Repos git detectados en wp-content (registrados + huérfanos) -->
    <div class="mb-2 flex items-center justify-between">
      <h3 class="text-xs font-semibold uppercase tracking-wide text-zinc-500">Repos git ({detected.length})</h3>
      <div class="flex gap-3 text-xs">
        <button class="text-blue-500 underline disabled:opacity-50" disabled={ghBusy} onclick={scanRepos}>Re-escanear</button>
        {#if detected.some((r) => r.registered)}
          <button class="text-blue-500 underline disabled:opacity-50" disabled={ghBusy} onclick={() => ghAction(() => api.ghPullAll(id))}>Pull todo</button>
        {/if}
      </div>
    </div>

    {#if detected.length === 0}
      <p class="mb-4 text-sm text-zinc-500">No se encontraron repos git en <code>wp-content</code>.</p>
    {:else}
      <div class="mb-5 flex flex-col gap-2">
        {#each detected as r (r.path)}
          <div class="flex items-center justify-between gap-2 rounded border px-3 py-2 text-sm {r.registered ? 'border-zinc-200 dark:border-zinc-800' : 'border-amber-300 dark:border-amber-900/60'}">
            <div class="min-w-0">
              <div class="truncate font-medium">{r.remote ?? r.name}</div>
              <div class="truncate text-xs text-zinc-500">{r.path}{r.branch ? ` · ${r.branch}` : ''}{r.remote ? '' : ' · sin remoto'}</div>
            </div>
            <div class="flex shrink-0 gap-2">
              {#if r.registered}
                {#if r.remote}
                  <button class="rounded bg-zinc-200 px-2 py-1 text-xs dark:bg-zinc-800" disabled={ghBusy}
                    onclick={() => ghAction(() => api.ghPull(id, r.path, r.branch ?? ''))}>Pull</button>
                {/if}
                <button class="rounded px-2 py-1 text-xs text-red-500" disabled={ghBusy} title="Quitar del proyecto (borra la carpeta)"
                  onclick={() => ghAction(() => api.ghRemove(id, r.path))}>✕</button>
              {:else}
                <button class="rounded bg-amber-500 px-2 py-1 text-xs font-medium text-white disabled:opacity-50" disabled={ghBusy} title="Registrar en el proyecto"
                  onclick={() => ghAction(() => api.ghRegister(id, r.path))}>Registrar</button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Clonar desde GitHub (requiere gh) -->
    {#if gh === null}
      <p class="text-sm text-zinc-500">Comprobando gh…</p>
    {:else if !gh.installed}
      <p class="text-sm text-amber-500">
        `gh` no está instalado. Instálalo con <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">sudo pacman -S github-cli</code> para clonar desde GitHub.
      </p>
    {:else if !gh.authenticated}
      <p class="text-sm text-amber-500">
        `gh` sin sesión. Ejecuta <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">gh auth login</code> para clonar desde GitHub.
      </p>
    {:else}
      <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">
        Clonar repo {#if gh.user}<span class="font-normal normal-case text-green-600 dark:text-green-400">· @{gh.user}</span>{/if}
      </h3>
      <div class="flex flex-wrap gap-2 text-sm">
        <input class="min-w-40 flex-1 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" placeholder="owner/repo" bind:value={cloneRepo} />
        <input class="w-24 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" placeholder="branch" bind:value={cloneBranch} />
        <select class="rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" bind:value={cloneKind}>
          <option value="plugin">plugins/</option>
          <option value="theme">themes/</option>
          <option value="muplugin">mu-plugins/</option>
        </select>
        <button class="rounded bg-blue-600 px-3 py-1 font-medium text-white disabled:opacity-50" disabled={ghBusy || !cloneRepo}
          onclick={() => ghAction(() => api.ghClone(id, cloneKind, cloneRepo, cloneBranch, clonePath || undefined).then(() => { cloneRepo = ''; clonePath = ''; }))}>Clonar</button>
      </div>
      <input class="mt-2 w-full rounded border border-zinc-300 px-2 py-1 text-sm dark:border-zinc-700 dark:bg-zinc-900" placeholder="ruta custom relativa a public/ (opcional; ignora la categoría)" bind:value={clonePath} />
    {/if}
  {:else if tab === 'svc'}
    {#if svcErr}
      <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{svcErr}</div>
    {/if}
    {#if svcMsg}
      <div class="mb-3 whitespace-pre-wrap rounded border border-blue-300 bg-blue-50 px-3 py-2 text-sm text-blue-700 dark:border-blue-900 dark:bg-blue-950 dark:text-blue-300">{svcMsg}</div>
    {/if}

    <!-- Base de datos -->
    <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Base de datos</h3>
    <div class="mb-4 flex flex-wrap items-center gap-2 text-sm">
      <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
        disabled={svcBusy || site.status !== 'running'}
        onclick={() => svcAction(() => api.openAdminer(id))}>Ver base de datos (Adminer)</button>
      <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
        disabled={svcBusy || site.status !== 'running'}
        onclick={() => svcAction(() => api.exportDb(id))}>Exportar base de datos</button>
      {#if site.status !== 'running'}<span class="text-xs text-zinc-500">(enciende el proyecto)</span>{/if}
    </div>

    <!-- Correo / S3 -->
    <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Servicios compartidos</h3>
    <div class="mb-4 flex flex-col gap-2 text-sm">
      <button class="self-start rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
        disabled={svcBusy} onclick={() => svcAction(() => api.openMailpit())}>Abrir Mailpit (correo)</button>
      <label class="flex items-center gap-2">
        <input type="checkbox" checked={site.config.minio} disabled={svcBusy}
          onchange={(ev) => svcAction(() => api.setSiteMinio(id, (ev.target as HTMLInputElement).checked))} />
        MinIO (S3 local compartido)
      </label>
      {#if site.config.minio}
        <button class="self-start rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
          disabled={svcBusy} onclick={() => svcAction(() => api.openMinio())}>Abrir consola MinIO</button>
      {/if}
    </div>

    <!-- WP-CLI terminal -->
    <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Terminal WP-CLI</h3>
    <div class="mb-2 flex flex-wrap items-center gap-2 text-sm">
      <button class="rounded bg-zinc-200 px-3 py-1.5 disabled:opacity-50 dark:bg-zinc-800"
        disabled={svcBusy || site.status !== 'running'}
        onclick={() => svcAction(() => api.openTerminal(id))}>Abrir terminal del proyecto</button>
      <button class="rounded border border-zinc-300 px-3 py-1.5 disabled:opacity-50 dark:border-zinc-700"
        disabled={svcBusy} onclick={() => svcAction(() => api.installCliWrapper())}>Solo instalar wrapper `wp`</button>
    </div>
    <p class="mb-4 text-xs text-zinc-500">
      Abre una terminal ya situada en la carpeta del proyecto con el wrapper
      <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">wp</code> listo. Dentro ejecuta p. ej.
      <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">wp plugin list</code> o
      <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">wp user list</code> — <strong>sin <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">sudo</code></strong>
      (corre dentro del container). El proyecto debe estar encendido.
    </p>

    <!-- Stubs -->
    <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Próximamente</h3>
    <div class="flex flex-wrap gap-2 text-sm">
      {#each [['cloudflare', 'Cloudflare Tunnel'], ['deploy', 'Deploy'], ['package', 'Empaquetar sitio']] as [key, label] (key)}
        <button class="rounded border border-dashed border-zinc-300 px-3 py-1.5 text-zinc-500 disabled:opacity-50 dark:border-zinc-700"
          disabled={svcBusy} onclick={() => svcAction(() => api.featureStub(key))}>{label}</button>
      {/each}
    </div>
  {/if}
{:else}
  <p class="text-sm text-zinc-500">Cargando…</p>
{/if}

<OpConsole open={consoleOpen} running={migrating} title="Migración" onClose={() => (consoleOpen = false)} />

<DeleteProjectModal bind:site={deleteTarget} onClose={(deleted) => deleted && goto('/')} />
