<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { api } from '$lib/api';
  import type { SiteState, SiteConfig, GhStatus, DetectedRepo, BranchStatus, Endpoint, SnapshotMeta, ExcludableEntry, WpUser } from '$lib/types';
  import OpConsole from '$lib/components/OpConsole.svelte';
  import DeleteProjectModal from '$lib/components/DeleteProjectModal.svelte';

  // Detalle embebido: lo monta el master-detail de `+page.svelte` por `id`.
  // `onChanged` refresca la lista izquierda (estado/encendido); `onDeleted` la
  // refresca tras borrar y deselecciona; `onSelect` cambia el proyecto activo
  // (clone/worktree recién creado, abrir worktree).
  let {
    id,
    onChanged,
    onDeleted,
    onSelect
  }: {
    id: string;
    onChanged?: () => void;
    onDeleted?: () => void;
    onSelect?: (id: string) => void;
  } = $props();

  let site = $state<SiteState | null>(null);
  let endpoint = $state<Endpoint | null>(null);
  let notFound = $state(false);
  let tab = $state<'info' | 'logs' | 'ext' | 'github' | 'svc' | 'snapshots'>('info');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let consoleOpen = $state(false);
  let migrating = $state(false);
  // Menú overflow «···» de acciones secundarias en la cabecera.
  let menuOpen = $state(false);

  // --- User picker para auto-login ------------------------------------------
  let wpUsers = $state<WpUser[]>([]);
  let wpUsersLoaded = $state(false);
  let wpUsersLoading = $state(false);

  function loadSavedUserId(): string {
    try {
      const raw = localStorage.getItem(`wp-panel:autologin:${id}`);
      if (raw) return JSON.parse(raw).userId ?? '';
    } catch {}
    return '';
  }
  let selectedUserId = $state(loadSavedUserId());

  function saveUserId(val: string) {
    selectedUserId = val;
    localStorage.setItem(`wp-panel:autologin:${id}`, JSON.stringify({ userId: val }));
  }

  async function loadWpUsers() {
    if (wpUsersLoaded || wpUsersLoading) return;
    wpUsersLoading = true;
    try {
      wpUsers = await api.listWpUsers(id);
      wpUsersLoaded = true;
    } catch {
      // silent — el select simplemente no aparece
    } finally {
      wpUsersLoading = false;
    }
  }

  $effect(() => {
    if (site?.status === 'running' && site.config.oneClickAdmin) {
      loadWpUsers();
    }
  });

  // --- Clones temporales / puntos de guardado --------------------------------
  let snapshots = $state<SnapshotMeta[]>([]);
  let snapshotsLoading = $state(false);
  let snapshotsError = $state<string | null>(null);
  // Formulario inline "Punto de guardado"
  let showSnapshotForm = $state(false);
  let snapshotLabel = $state('');
  let snapshotBusy = $state(false);
  // Consola de progreso al crear snapshot
  let snapshotConsoleOpen = $state(false);
  let snapshotRunning = $state(false);
  // Clone en curso (OpConsole)
  let cloneConsoleOpen = $state(false);
  let cloning = $state(false);
  // Worktree-projects (probar una rama de un repo en aislamiento)
  let worktrees = $state<SiteConfig[]>([]);
  let wtTargetPath = $state('');
  let wtBranch = $state('');
  let wtBaseBranch = $state('');
  let wtSharedDb = $state(true);
  let wtConsoleOpen = $state(false);
  let wtRunning = $state(false);
  let wtHelpOpen = $state(false);
  // Exclusiones del punto de guardado
  let showExcludes = $state(false);
  let excludable = $state<ExcludableEntry[]>([]);
  let excludablesLoading = $state(false);
  let selectedExcludes = $state<string[]>([]);
  let manualExclude = $state('');
  let excludesBusy = $state(false);

  /** Rutas seleccionadas que no salen en la lista detectada (añadidas a mano o ya no existen en disco). */
  let manualOnlyExcludes = $derived(
    selectedExcludes.filter((p) => !excludable.some((e) => e.path === p))
  );

  async function openExcludes() {
    showExcludes = true;
    selectedExcludes = [...(site?.config.snapshotExcludes ?? [])];
    excludablesLoading = true;
    try {
      excludable = await api.detectExcludable(id);
    } catch (err) {
      snapshotsError = String(err);
    } finally {
      excludablesLoading = false;
    }
  }

  function toggleExclude(path: string) {
    selectedExcludes = selectedExcludes.includes(path)
      ? selectedExcludes.filter((p) => p !== path)
      : [...selectedExcludes, path];
  }

  function addManualExclude() {
    const v = manualExclude.trim().replace(/^\.?\/+/, '').replace(/\/+$/, '');
    if (v && !selectedExcludes.includes(v)) selectedExcludes = [...selectedExcludes, v];
    manualExclude = '';
  }

  async function saveExcludes() {
    if (!site) return;
    excludesBusy = true;
    snapshotsError = null;
    try {
      await api.setSnapshotExcludes(id, selectedExcludes);
      site.config.snapshotExcludes = [...selectedExcludes];
      showExcludes = false;
    } catch (err) {
      snapshotsError = String(err);
    } finally {
      excludesBusy = false;
    }
  }

  function fmtBytes(b: number): string {
    if (b <= 0) return '';
    if (b >= 1_073_741_824) return `${(b / 1_073_741_824).toFixed(1)} GB`;
    if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(1)} MB`;
    if (b >= 1_024) return `${Math.round(b / 1_024)} KB`;
    return `${b} B`;
  }

  async function loadSnapshots() {
    snapshotsLoading = true;
    snapshotsError = null;
    try {
      snapshots = await api.listSnapshots(id);
    } catch (err) {
      snapshotsError = String(err);
    } finally {
      snapshotsLoading = false;
    }
  }

  async function createSnapshot() {
    if (!snapshotLabel.trim()) return;
    snapshotBusy = true;
    snapshotRunning = true;
    snapshotConsoleOpen = true;
    snapshotsError = null;
    try {
      await api.createSnapshot(id, snapshotLabel.trim());
      snapshotLabel = '';
      showSnapshotForm = false;
      await loadSnapshots();
    } catch (err) {
      snapshotsError = String(err);
    } finally {
      snapshotBusy = false;
      snapshotRunning = false;
    }
  }

  async function deleteSnapshot(snapshotId: string) {
    if (!confirm('¿Borrar este punto de guardado? No se puede deshacer.')) return;
    snapshotsError = null;
    try {
      await api.deleteSnapshot(id, snapshotId);
      await loadSnapshots();
    } catch (err) {
      snapshotsError = String(err);
    }
  }

  async function cloneFromSnapshot(snapshotId: string) {
    const snap = snapshots.find((s) => s.id === snapshotId);
    if (!snap) return;
    if (!confirm(`¿Crear un clone temporal desde «${snap.label}»?\nSe creará un proyecto nuevo basado en ese punto de guardado.`)) return;
    cloning = true;
    cloneConsoleOpen = true;
    try {
      const cloneSite = await api.createClone(id, snapshotId);
      onChanged?.();
      onSelect?.(cloneSite.id);
    } catch (err) {
      error = String(err);
    } finally {
      cloning = false;
    }
  }

  $effect(() => {
    if (tab === 'snapshots') loadSnapshots();
  });

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
    onChanged?.();
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
      onDeleted?.();
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
    const userId = selectedUserId ? Number(selectedUserId) : undefined;
    try {
      await api.openAdmin(id, userId);
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

  // Tope de subida (MB) por proyecto: evita el 413 al subir themes/plugins
  // grandes. Aplica upload_max_filesize + post_max_size en el php.ini y recarga
  // php-fpm en caliente si el proyecto está activo. 0/vacío = default del panel.
  let uploadMb = $state<number | undefined>(undefined);
  let savingUpload = $state(false);
  $effect(() => {
    // Re-sincroniza el input al cargar o cambiar de proyecto.
    uploadMb = site?.config.services.php.uploadMaxMb;
  });
  async function saveUpload() {
    if (!site) return;
    savingUpload = true;
    error = null;
    try {
      await api.setPhpUploadLimit(id, Number(uploadMb) || 0);
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      savingUpload = false;
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
  // Deploy directo (staging): panel expandible por repo
  let deployOpen = $state<string | null>(null); // path del repo con el panel abierto
  let deployBranch = $state('');
  let deployCmd = $state('');
  let deployDirs = $state<string[]>([]); // carpetas de build elegidas (relativas al repo)
  let dirCandidates = $state<string[]>([]); // carpetas con package.json detectadas
  let deployStatus = $state<BranchStatus | null>(null);
  let deployConsoleOpen = $state(false);
  let deployRunning = $state(false);

  async function toggleDeploy(r: DetectedRepo) {
    if (deployOpen === r.path) {
      deployOpen = null;
      return;
    }
    const reg = site?.config.github.repos.find((x) => x.path === r.path);
    deployBranch = reg?.branch ?? r.branch ?? '';
    deployCmd = reg?.buildCmd ?? '';
    deployDirs = [...(reg?.buildDirs ?? [])];
    deployStatus = null;
    deployOpen = r.path;
    dirCandidates = [];
    try {
      dirCandidates = await api.ghBuildDirs(id, r.path);
    } catch (e) {
      ghError = String(e);
    }
  }

  function toggleDir(dir: string) {
    deployDirs = deployDirs.includes(dir)
      ? deployDirs.filter((d) => d !== dir)
      : [...deployDirs, dir];
  }

  async function checkBranch(path: string) {
    ghBusy = true;
    ghError = null;
    deployStatus = null;
    try {
      deployStatus = await api.ghBranchStatus(id, path, deployBranch.trim());
    } catch (e) {
      ghError = String(e);
    } finally {
      ghBusy = false;
    }
  }

  async function runDeploy(path: string) {
    // Persistir rama+build antes, para que el backend use lo que hay en pantalla.
    try {
      await api.ghSetDeploy(id, path, deployBranch.trim(), deployCmd.trim() || null, deployDirs);
    } catch (e) {
      ghError = String(e);
      return;
    }
    deployConsoleOpen = true;
    deployRunning = true;
    ghError = null;
    try {
      await api.ghDeploy(id, path);
      await load();
      await scanRepos();
    } catch (e) {
      ghError = String(e);
    } finally {
      deployRunning = false;
    }
  }

  async function loadGh() {
    try {
      gh = await api.ghStatus();
    } catch (e) {
      ghError = String(e);
    }
    await scanRepos();
    await loadWorktrees();
  }

  async function loadWorktrees() {
    try {
      worktrees = await api.listWorktrees(id);
      if (!wtTargetPath && detected.length) wtTargetPath = detected[0].path;
    } catch (e) {
      ghError = String(e);
    }
  }

  async function createWorktree() {
    if (!wtTargetPath || !wtBranch.trim()) return;
    wtRunning = true;
    wtConsoleOpen = true;
    error = null;
    try {
      const wt = await api.createWorktreeSite(
        id,
        wtTargetPath,
        wtBranch.trim(),
        wtSharedDb,
        wtBaseBranch.trim() || undefined,
      );
      onChanged?.();
      onSelect?.(wt.id);
    } catch (err) {
      error = String(err);
    } finally {
      wtRunning = false;
    }
  }

  async function removeWorktree(wtId: string, branch: string) {
    if (
      !confirm(
        `¿Eliminar el worktree «${branch}»?\n\nSe borrará el proyecto de prueba y su container, pero la RAMA se conserva en el proyecto principal para seguir trabajándola.`,
      )
    )
      return;
    wtRunning = true;
    wtConsoleOpen = true;
    try {
      await api.removeWorktreeSite(wtId, false);
      await loadWorktrees();
      onChanged?.();
    } catch (err) {
      error = String(err);
    } finally {
      wtRunning = false;
    }
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
    { id: 'svc', label: 'Servicios' },
    { id: 'snapshots', label: 'Puntos de guardado' }
  ] as const;
</script>

{#if notFound}
  <p class="text-sm text-zinc-500">Proyecto no encontrado.</p>
{:else if site}
  <div class="mb-4 flex items-start justify-between gap-4">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <h1 class="truncate text-lg font-semibold">{site.config.name}</h1>
        {#if site.config.cloneOf}
          <span class="shrink-0 rounded border border-amber-400 px-1.5 py-0.5 text-xs font-medium text-amber-500">Clone temporal</span>
        {/if}
        {#if site.config.worktreeOf}
          <span class="shrink-0 rounded border border-violet-400 px-1.5 py-0.5 text-xs font-medium text-violet-500">Worktree</span>
        {/if}
        <!-- Menú «···» de acciones secundarias -->
        <div class="relative">
          <button
            class="flex h-7 w-7 items-center justify-center rounded text-zinc-400 hover:bg-zinc-200 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
            title="Más acciones"
            aria-label="Más acciones"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onclick={() => (menuOpen = !menuOpen)}
          >
            <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
              <path d="M6 10a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Zm5.5 0a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Zm4 1.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
            </svg>
          </button>
          {#if menuOpen}
            <button class="fixed inset-0 z-10 cursor-default" aria-label="Cerrar menú" onclick={() => (menuOpen = false)}></button>
            <div class="absolute left-0 z-20 mt-1 w-52 overflow-hidden rounded border border-zinc-200 bg-white py-1 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-900" role="menu">
              <button class="block w-full px-3 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800" onclick={() => { menuOpen = false; openFolder(); }}>Abrir carpeta</button>
              {#if !site.config.cloneOf}
                <button class="block w-full px-3 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800" onclick={() => { menuOpen = false; tab = 'snapshots'; showSnapshotForm = true; }}>Punto de guardado</button>
              {/if}
              {#if site.config.services.nginx.ssl}
                <button class="block w-full px-3 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800" onclick={() => { menuOpen = false; act(() => api.regenerateSsl(id)); }}>Regenerar SSL</button>
              {/if}
              <div class="my-1 border-t border-zinc-200 dark:border-zinc-800"></div>
              <button class="block w-full px-3 py-1.5 text-left text-red-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" disabled={busy} onclick={() => { menuOpen = false; deleteTarget = site; }}>Eliminar</button>
            </div>
          {/if}
        </div>
      </div>
      <p class="text-sm text-zinc-500">{hostLabel(site)}</p>

      {#if site.status === 'running'}
        <!-- Accesos rápidos (estilo LocalWP: carpeta · admin · web) -->
        <div class="mt-1 flex flex-wrap items-center gap-3 text-sm">
          <button class="flex items-center gap-1 text-zinc-600 hover:text-blue-500 dark:text-zinc-400" onclick={openSite}>
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path d="M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16Zm5.3 5h-2.2a12.6 12.6 0 0 0-1-2.5A6 6 0 0 1 15.3 7ZM10 4c.6.8 1.1 1.8 1.4 3H8.6c.3-1.2.8-2.2 1.4-3ZM4.3 12a6 6 0 0 1 0-4h2.5a14 14 0 0 0 0 4H4.3Zm.4 1h2.2c.3.9.6 1.7 1 2.5A6 6 0 0 1 4.7 13Zm2.2-6H4.7a6 6 0 0 1 3.2-2.5c-.4.8-.7 1.6-1 2.5ZM10 16c-.6-.8-1.1-1.8-1.4-3h2.8c-.3 1.2-.8 2.2-1.4 3Zm1.7-4H8.3a12.4 12.4 0 0 1 0-4h3.4a12.4 12.4 0 0 1 0 4Zm.4 3.5c.4-.8.7-1.6 1-2.5h2.2a6 6 0 0 1-3.2 2.5ZM13.2 12a14 14 0 0 0 0-4h2.5a6 6 0 0 1 0 4h-2.5Z" /></svg>
            Abrir web
          </button>
          <button class="flex items-center gap-1 text-zinc-600 hover:text-blue-500 dark:text-zinc-400" onclick={openAdmin}>
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path d="M10 10a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm0 1.5c-3 0-6 1.5-6 3.5v1h12v-1c0-2-3-3.5-6-3.5Z" /></svg>
            Abrir admin
          </button>
          <button class="flex items-center gap-1 text-zinc-600 hover:text-blue-500 dark:text-zinc-400" onclick={openFolder}>
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h4l1.5 1.5h5.5A1.5 1.5 0 0 1 16 7v6.5A1.5 1.5 0 0 1 14.5 15h-11A1.5 1.5 0 0 1 2 13.5v-8Z" /></svg>
            Abrir carpeta
          </button>
        </div>
      {/if}
    </div>

    <!-- Acción primaria: encender/detener (o migrar si está pendiente) -->
    <div class="flex shrink-0 items-center gap-2">
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
        <button
          class="rounded px-4 py-1.5 text-sm font-medium"
          class:bg-green-600={site.status !== 'running'}
          class:text-white={site.status !== 'running'}
          class:hover:bg-green-500={site.status !== 'running'}
          class:bg-zinc-200={site.status === 'running'}
          class:dark:bg-zinc-800={site.status === 'running'}
          disabled={busy}
          onclick={toggle}
        >
          {busy ? '…' : site.status === 'running' ? 'Detener' : 'Encender'}
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
      <dt class="text-zinc-500">Auto-login (One-click admin)</dt>
      <dd>
        {#if site.config.oneClickAdmin}
          {#if site.status === 'running'}
            <select
              class="rounded border border-zinc-300 bg-white py-1 pl-2 pr-6 text-sm dark:border-zinc-700 dark:bg-zinc-900"
              value={selectedUserId}
              onchange={(e) => saveUserId((e.target as HTMLSelectElement).value)}
              title="Usuario con el que abrir el admin"
            >
              <option value="">Primer admin</option>
              {#each wpUsers as u (u.ID)}
                <option value={u.ID}>{u.display_name} ({u.user_login})</option>
              {/each}
            </select>
          {:else}
            Sí <span class="text-xs text-zinc-500">(enciende para elegir el usuario)</span>
          {/if}
        {:else}
          No
        {/if}
      </dd>
      <dt class="text-zinc-500">Tope de subida</dt>
      <dd class="flex items-center gap-2">
        <input
          type="number"
          min="0"
          placeholder="64"
          bind:value={uploadMb}
          class="w-20 rounded border border-zinc-300 bg-white py-1 px-2 text-sm dark:border-zinc-700 dark:bg-zinc-900"
          title="MB — 0 o vacío = default del panel (64M)"
        />
        <span class="text-xs text-zinc-500">MB (0 = default)</span>
        <button
          class="rounded bg-zinc-200 px-2.5 py-1 text-sm font-medium disabled:opacity-50 dark:bg-zinc-800"
          disabled={savingUpload}
          onclick={saveUpload}
        >
          {savingUpload ? '…' : 'Guardar'}
        </button>
      </dd>
      <dt class="text-zinc-500">Headless</dt>
      <dd>{site.config.headless ? (site.config.frontendFramework ?? 'Sí') : 'No'}</dd>
      <dt class="text-zinc-500">MinIO</dt>
      <dd>{site.config.minio ? 'Sí' : 'No'}</dd>
      <dt class="text-zinc-500">Ruta</dt>
      <dd class="truncate">{site.config.path}</dd>
    </dl>
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
        {#if site.config.worktreeOf}
          <div class="text-xs text-zinc-500">Abre el <code>git worktree</code> (<code>wt/{site.config.worktreeOf.targetPath.split('/').pop()}</code>, rama <code>{site.config.worktreeOf.branch}</code>). Los cambios se reflejan en vivo en el sitio.</div>
        {:else}
          <div class="text-xs text-zinc-500">Abre un workspace con <code>public/</code> como carpeta principal y cada repo git detectado como adicional. Se crea una vez; luego puedes editarlo a mano.</div>
        {/if}
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
          <div class="rounded border {r.registered ? 'border-zinc-200 dark:border-zinc-800' : 'border-amber-300 dark:border-amber-900/60'}">
            <div class="flex items-center justify-between gap-2 px-3 py-2 text-sm">
              <div class="min-w-0">
                <div class="truncate font-medium">{r.remote ?? r.name}</div>
                <div class="truncate text-xs text-zinc-500">{r.path}{r.branch ? ` · ${r.branch}` : ''}{r.remote ? '' : ' · sin remoto'}</div>
              </div>
              <div class="flex shrink-0 gap-2">
                {#if r.registered}
                  {#if r.remote}
                    <button class="rounded bg-emerald-600 px-2 py-1 text-xs font-medium text-white disabled:opacity-50" disabled={ghBusy || deployRunning}
                      title="Ver estado y desplegar (pull + build) desde aquí"
                      onclick={() => toggleDeploy(r)}>Deploy ▾</button>
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

            {#if r.registered && r.remote && deployOpen === r.path}
              <div class="border-t border-zinc-200 px-3 py-3 text-sm dark:border-zinc-800">
                <div class="mb-2 flex flex-wrap items-end gap-3">
                  <label class="flex flex-col gap-1">
                    <span class="text-xs text-zinc-500">Rama a desplegar</span>
                    <input class="w-48 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" placeholder="main" bind:value={deployBranch} />
                  </label>
                  <label class="flex flex-1 flex-col gap-1">
                    <span class="text-xs text-zinc-500">Comando de build (host, tras el pull · opcional)</span>
                    <input class="w-full rounded border border-zinc-300 px-2 py-1 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900" placeholder="pnpm install && pnpm build" bind:value={deployCmd} />
                  </label>
                </div>

                <!-- Carpetas de build: dónde correr el comando (raíz por defecto) -->
                {#if deployCmd.trim()}
                  <div class="mb-2">
                    <div class="mb-1 text-xs text-zinc-500">Carpeta(s) de build <span class="text-zinc-400">(sin marcar = raíz del repo · marca varias si el build va en más de una)</span></div>
                    {#if dirCandidates.length === 0}
                      <div class="text-xs text-zinc-400">No se detectaron carpetas con <code>package.json</code>. Se usará la raíz del repo.</div>
                    {:else}
                      <div class="flex flex-wrap gap-2">
                        {#each dirCandidates as d (d)}
                          <button
                            class="rounded border px-2 py-1 text-xs {deployDirs.includes(d) ? 'border-emerald-500 bg-emerald-500 text-white' : 'border-zinc-300 dark:border-zinc-700'}"
                            onclick={() => toggleDir(d)}>{d === '' ? 'raíz' : d}</button>
                        {/each}
                      </div>
                    {/if}
                    {#if deployDirs.length}
                      <div class="mt-1 text-xs text-zinc-500">Build en: {deployDirs.map((d) => (d === '' ? 'raíz' : d)).join(', ')}</div>
                    {/if}
                  </div>
                {/if}

                <div class="flex flex-wrap gap-2">
                  <button class="rounded bg-zinc-200 px-3 py-1 text-xs dark:bg-zinc-800 disabled:opacity-50" disabled={ghBusy || deployRunning}
                    onclick={() => checkBranch(r.path)}>Ver estado</button>
                  <button class="rounded bg-emerald-600 px-3 py-1 text-xs font-medium text-white disabled:opacity-50" disabled={ghBusy || deployRunning}
                    title="Guarda la config, hace checkout + git pull --ff-only y ejecuta el build"
                    onclick={() => runDeploy(r.path)}>Pull + build</button>
                  <button class="rounded px-3 py-1 text-xs text-blue-500 underline disabled:opacity-50" disabled={ghBusy || deployRunning}
                    onclick={() => ghAction(() => api.ghSetDeploy(id, r.path, deployBranch.trim(), deployCmd.trim() || null, deployDirs))}>Solo guardar config</button>
                </div>
                {#if deployStatus}
                  <div class="mt-3 rounded px-3 py-2 text-xs {deployStatus.canPull ? 'bg-emerald-50 text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-300' : deployStatus.dirty || !deployStatus.hasRemote ? 'bg-amber-50 text-amber-800 dark:bg-amber-950/40 dark:text-amber-300' : 'bg-zinc-100 text-zinc-600 dark:bg-zinc-800/60 dark:text-zinc-300'}">
                    <div>Rama actual: <code>{deployStatus.current}</code> · objetivo: <code>{deployStatus.target}</code></div>
                    <div>↓ {deployStatus.behind} por traer · ↑ {deployStatus.ahead} por delante · {deployStatus.dirty ? 'árbol con cambios' : 'árbol limpio'}</div>
                    <div class="mt-1 font-medium">{deployStatus.message}</div>
                  </div>
                {/if}
              </div>
            {/if}
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

    <!-- Worktrees: probar una rama de un repo en aislamiento -->
    <hr class="my-5 border-zinc-200 dark:border-zinc-800" />
    {#if site.config.worktreeOf}
      <div class="rounded border border-violet-300 bg-violet-50 px-3 py-2 text-sm text-violet-800 dark:border-violet-900/60 dark:bg-violet-950/40 dark:text-violet-300">
        Este proyecto <strong>es un worktree</strong> de otro (rama
        <code class="rounded bg-violet-200/60 px-1 dark:bg-violet-900/60">{site.config.worktreeOf.branch}</code>
        sobre <code class="rounded bg-violet-200/60 px-1 dark:bg-violet-900/60">{site.config.worktreeOf.targetPath}</code>).
        Comparte el código del proyecto principal por montaje; solo el repo objetivo es independiente.
        Elimínalo desde el proyecto principal cuando termines — la rama quedará guardada.
      </div>
    {:else}
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-xs font-semibold uppercase tracking-wide text-zinc-500">Worktrees ({worktrees.length})</h3>
        <button
          class="flex h-5 w-5 items-center justify-center rounded-full border border-zinc-300 text-xs font-bold text-zinc-500 hover:bg-zinc-100 dark:border-zinc-600 dark:hover:bg-zinc-800"
          title="¿Qué es esto?"
          aria-label="¿Qué es un worktree?"
          aria-expanded={wtHelpOpen}
          onclick={() => (wtHelpOpen = !wtHelpOpen)}>?</button>
      </div>
      <p class="mb-3 text-xs text-zinc-500">
        Crea un proyecto de prueba ligero atado a un repo y una <strong>rama nueva</strong>. El resto de WordPress
        se comparte (sin copiar); solo el repo objetivo es un <code class="rounded bg-zinc-200 px-1 dark:bg-zinc-800">git worktree</code>
        independiente con su propia URL. Al eliminarlo, la rama queda en el proyecto principal.
      </p>

      {#if wtHelpOpen}
        <div class="mb-4 rounded-lg border border-violet-200 bg-violet-50 p-4 text-sm leading-relaxed text-zinc-700 dark:border-violet-900/50 dark:bg-violet-950/30 dark:text-zinc-300">
          <div class="mb-2 flex items-center justify-between">
            <h4 class="font-semibold text-violet-800 dark:text-violet-300">¿Qué es un worktree y para qué sirve?</h4>
            <button class="text-xs text-zinc-500 underline" onclick={() => (wtHelpOpen = false)}>Cerrar</button>
          </div>

          <p class="mb-2">
            En Git, cada rama suele compartir <em>una sola carpeta</em> de trabajo: para cambiar de rama haces
            <code class="rounded bg-violet-200/60 px-1 dark:bg-violet-900/60">git checkout</code> y los archivos de esa carpeta se reemplazan.
            Un <strong>worktree</strong> es una carpeta <em>extra</em> del mismo repo donde tienes
            <strong>otra rama abierta a la vez</strong>, sin tocar tu carpeta principal.
          </p>

          <p class="mb-2">
            El problema en WordPress: para probar esa otra rama de tu theme o plugin necesitarías
            <strong>otro WordPress completo</strong> (base de datos, core, plugins…). Montar todo eso solo para una prueba es pesado.
          </p>

          <p class="mb-1 font-medium text-violet-800 dark:text-violet-300">Lo que hace este botón:</p>
          <ul class="mb-2 ml-4 list-disc space-y-1">
            <li>Crea la <strong>rama nueva</strong> y la abre en un worktree, <strong>sin copiar</strong> todo WordPress: el resto del sitio se comparte con el proyecto principal.</li>
            <li>Te da una <strong>URL propia</strong> para ver esa rama funcionando, separada del sitio principal.</li>
            <li>Puedes <strong>compartir la base de datos</strong> del proyecto principal o usar una <strong>copia</strong> (para cambiar datos sin riesgo hasta que la rama esté lista).</li>
            <li>Al terminar, eliminas el worktree y <strong>la rama se queda guardada</strong> en el proyecto principal para seguir con ella. No queda rastro del proyecto de prueba.</li>
          </ul>

          <p class="mb-2">
            <span class="font-medium">En resumen:</span> pruebas una rama de tu theme/plugin de forma aislada, con su propia URL,
            sin duplicar el sitio ni ensuciar el proyecto principal — y deshaces todo con un clic conservando tu trabajo.
          </p>

          <p class="text-xs text-zinc-500">
            <strong>Base de datos compartida</strong>: ves los mismos contenidos que el sitio principal (ideal para cambios solo de código/diseño).
            <strong>Copia</strong>: una base de datos propia que puedes modificar libremente sin afectar al principal.
          </p>
        </div>
      {/if}

      {#if worktrees.length}
        <div class="mb-4 flex flex-col gap-2">
          {#each worktrees as w (w.id)}
            <div class="flex items-center justify-between gap-2 rounded border border-violet-200 px-3 py-2 text-sm dark:border-violet-900/50">
              <div class="min-w-0">
                <div class="truncate font-medium">{w.worktreeOf?.branch}</div>
                <div class="truncate text-xs text-zinc-500">
                  {w.worktreeOf?.targetPath} · {w.domain} · BD {w.worktreeOf?.sharedDb ? 'compartida' : 'copia'}
                </div>
              </div>
              <div class="flex shrink-0 gap-2">
                <button class="rounded bg-zinc-200 px-2 py-1 text-xs dark:bg-zinc-800" disabled={ghBusy}
                  onclick={() => onSelect?.(w.id)}>Abrir</button>
                <button class="rounded px-2 py-1 text-xs text-red-500" disabled={wtRunning} title="Eliminar worktree (conserva la rama)"
                  onclick={() => removeWorktree(w.id, w.worktreeOf?.branch ?? '')}>✕</button>
              </div>
            </div>
          {/each}
        </div>
      {/if}

      {#if detected.length === 0}
        <p class="text-sm text-zinc-500">Necesitas al menos un repo git en <code>wp-content</code> para crear un worktree.</p>
      {:else}
        <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">Nuevo worktree</h3>
        <div class="flex flex-wrap gap-2 text-sm">
          <select class="min-w-44 flex-1 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" bind:value={wtTargetPath}>
            {#each detected as r (r.path)}
              <option value={r.path}>{r.name} ({r.path})</option>
            {/each}
          </select>
          <input class="w-40 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" placeholder="rama nueva" bind:value={wtBranch} />
          <input class="w-32 rounded border border-zinc-300 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900" placeholder="base (opcional)" bind:value={wtBaseBranch} />
          <button class="rounded bg-violet-600 px-3 py-1 font-medium text-white disabled:opacity-50" disabled={wtRunning || !wtBranch.trim()}
            onclick={createWorktree}>Crear worktree</button>
        </div>
        <label class="mt-2 flex items-center gap-2 text-sm">
          <input type="checkbox" bind:checked={wtSharedDb} />
          Compartir la base de datos del proyecto principal
          <span class="text-xs text-zinc-500">(desmarca para una copia aislada y poder cambiar datos sin afectar al principal)</span>
        </label>
      {/if}
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
  {:else if tab === 'snapshots'}
    <!-- Puntos de guardado -->
    {#if snapshotsError}
      <div class="mb-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">{snapshotsError}</div>
    {/if}

    <!-- Formulario para crear nuevo punto de guardado (solo si no es un clone) -->
    {#if !site.config.cloneOf}
      <div class="mb-4">
        {#if showSnapshotForm}
          <div class="flex items-center gap-2 rounded border border-zinc-200 px-3 py-2 dark:border-zinc-800">
            <input
              class="flex-1 rounded border border-zinc-300 px-2 py-1 text-sm dark:border-zinc-700 dark:bg-zinc-900"
              placeholder="Etiqueta del punto de guardado…"
              bind:value={snapshotLabel}
              onkeydown={(e) => e.key === 'Enter' && createSnapshot()}
            />
            <button
              class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
              disabled={snapshotBusy || !snapshotLabel.trim()}
              onclick={createSnapshot}
            >
              {snapshotBusy ? '…' : 'Guardar'}
            </button>
            <button
              class="rounded px-2 py-1.5 text-sm text-zinc-400 hover:text-zinc-600"
              onclick={() => { showSnapshotForm = false; snapshotLabel = ''; }}
            >
              ✕
            </button>
          </div>
        {:else}
          <button
            class="rounded bg-zinc-200 px-3 py-1.5 text-sm dark:bg-zinc-800"
            onclick={() => (showSnapshotForm = true)}
          >
            + Nuevo punto de guardado
          </button>
        {/if}
      </div>

      <!-- Exclusiones: carpetas que NO se guardan en el punto de guardado -->
      <div class="mb-5 rounded border border-zinc-200 dark:border-zinc-800">
        <button
          class="flex w-full items-center justify-between px-3 py-2 text-left text-sm"
          onclick={() => (showExcludes ? (showExcludes = false) : openExcludes())}
        >
          <span class="font-medium">Exclusiones</span>
          <span class="text-xs text-zinc-500">
            {#if (site.config.snapshotExcludes?.length ?? 0) > 0}
              {site.config.snapshotExcludes!.length} carpeta(s) excluida(s) · editar
            {:else}
              ninguna · configurar
            {/if}
            <span class="ml-1">{showExcludes ? '▾' : '▸'}</span>
          </span>
        </button>

        {#if showExcludes}
          <div class="border-t border-zinc-200 px-3 py-3 dark:border-zinc-800">
            <p class="mb-2 text-xs text-zinc-500">
              Siempre se excluyen uploads, caché, wp-config.php y *.log. Marca aquí
              carpetas adicionales (backups de plugins, carpetas propias) para que
              no pesen en cada punto de guardado.
            </p>

            {#if excludablesLoading}
              <p class="text-sm text-zinc-500">Escaneando carpetas…</p>
            {:else}
              {#if excludable.length === 0 && manualOnlyExcludes.length === 0}
                <p class="text-sm text-zinc-500">No se detectaron carpetas. Añade una ruta a mano abajo.</p>
              {/if}

              <div class="flex flex-col gap-1">
                {#each excludable as ex (ex.path)}
                  <label class="flex items-center gap-2 rounded px-1.5 py-1 text-sm hover:bg-zinc-100 dark:hover:bg-zinc-900">
                    <input
                      type="checkbox"
                      checked={selectedExcludes.includes(ex.path)}
                      onchange={() => toggleExclude(ex.path)}
                    />
                    <span class="font-mono text-xs">{ex.path}</span>
                    {#if ex.known}
                      <span class="rounded bg-amber-100 px-1.5 text-[10px] font-medium text-amber-800 dark:bg-amber-950 dark:text-amber-300">
                        {ex.label} · recomendado
                      </span>
                    {/if}
                    <span class="ml-auto text-xs text-zinc-400">{fmtBytes(ex.bytes)}</span>
                  </label>
                {/each}

                <!-- Excludes persistidos/manuales que no están en disco -->
                {#each manualOnlyExcludes as p (p)}
                  <label class="flex items-center gap-2 rounded px-1.5 py-1 text-sm hover:bg-zinc-100 dark:hover:bg-zinc-900">
                    <input type="checkbox" checked={true} onchange={() => toggleExclude(p)} />
                    <span class="font-mono text-xs">{p}</span>
                    <span class="rounded bg-zinc-100 px-1.5 text-[10px] text-zinc-500 dark:bg-zinc-800">manual</span>
                  </label>
                {/each}
              </div>

              <div class="mt-3 flex items-center gap-2">
                <input
                  class="flex-1 rounded border border-zinc-300 px-2 py-1 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900"
                  placeholder="ruta relativa, p. ej. wp-content/mi-carpeta"
                  bind:value={manualExclude}
                  onkeydown={(e) => e.key === 'Enter' && addManualExclude()}
                />
                <button
                  class="rounded bg-zinc-200 px-2.5 py-1 text-xs dark:bg-zinc-800 disabled:opacity-50"
                  disabled={!manualExclude.trim()}
                  onclick={addManualExclude}
                >
                  Añadir
                </button>
              </div>

              <div class="mt-3 flex justify-end gap-2">
                <button
                  class="rounded px-3 py-1.5 text-sm text-zinc-400 hover:text-zinc-600"
                  onclick={() => (showExcludes = false)}
                >
                  Cancelar
                </button>
                <button
                  class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
                  disabled={excludesBusy}
                  onclick={saveExcludes}
                >
                  {excludesBusy ? '…' : 'Guardar exclusiones'}
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {:else}
      <p class="mb-4 text-sm text-zinc-500">
        Los puntos de guardado se crean en el proyecto original, no en el clone.
      </p>
    {/if}

    <!-- Lista de snapshots -->
    {#if snapshotsLoading}
      <p class="text-sm text-zinc-500">Cargando…</p>
    {:else if snapshots.length === 0}
      <p class="text-sm text-zinc-500">No hay puntos de guardado todavía.</p>
    {:else}
      <div class="flex flex-col gap-2">
        {#each snapshots as snap (snap.id)}
          <div class="flex items-center justify-between gap-3 rounded border border-zinc-200 px-3 py-2 text-sm dark:border-zinc-800">
            <div class="min-w-0">
              <div class="font-medium">{snap.label}</div>
              <div class="text-xs text-zinc-500">
                {new Date(snap.createdAt).toLocaleString()}
                · {snap.dbType} · {snap.dbName}
              </div>
              {#if snap.codeBytes > 0 || snap.dbBytes > 0}
                <div class="mt-0.5 flex gap-3 text-xs text-zinc-400">
                  {#if snap.codeBytes > 0}
                    <span title="Código comprimido (code.tar.zst)">código {fmtBytes(snap.codeBytes)}</span>
                  {/if}
                  {#if snap.dbBytes > 0}
                    <span title="Dump de base de datos (db.sql)">BD {fmtBytes(snap.dbBytes)}</span>
                  {/if}
                  <span class="font-medium text-zinc-300">total {fmtBytes(snap.codeBytes + snap.dbBytes)}</span>
                </div>
              {/if}
              {#if snap.excludes && snap.excludes.length > 0}
                <div class="mt-0.5 text-xs text-zinc-500" title={snap.excludes.join('\n')}>
                  {snap.excludes.length} carpeta(s) excluida(s)
                </div>
              {/if}
            </div>
            <div class="flex shrink-0 gap-2">
              {#if !site.config.cloneOf}
                <button
                  class="rounded bg-amber-600 px-2 py-1.5 text-xs font-medium text-white disabled:opacity-50"
                  disabled={cloning}
                  onclick={() => cloneFromSnapshot(snap.id)}
                >
                  Clonar desde aquí
                </button>
              {/if}
              <button
                class="rounded px-2 py-1.5 text-xs text-red-500 hover:text-red-700"
                onclick={() => deleteSnapshot(snap.id)}
              >
                Borrar
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
{:else}
  <p class="text-sm text-zinc-500">Cargando…</p>
{/if}

<OpConsole open={consoleOpen} running={migrating} title="Migración" onClose={() => (consoleOpen = false)} />
<OpConsole open={snapshotConsoleOpen} running={snapshotRunning} title="Punto de guardado" onClose={() => (snapshotConsoleOpen = false)} />
<OpConsole open={cloneConsoleOpen} running={cloning} title="Crear clone" onClose={() => (cloneConsoleOpen = false)} />
<OpConsole open={wtConsoleOpen} running={wtRunning} title="Worktree" onClose={() => (wtConsoleOpen = false)} />
<OpConsole open={deployConsoleOpen} running={deployRunning} title="Deploy" onClose={() => (deployConsoleOpen = false)} />

<DeleteProjectModal bind:site={deleteTarget} onClose={(deleted) => deleted && onDeleted?.()} />
