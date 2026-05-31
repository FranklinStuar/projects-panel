<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import type { DbType, WpVersion, NewSiteRequest } from '$lib/types';

  const PHP_VERSIONS = ['8.4', '8.3', '8.2', '8.1', '8.0', '7.4'];
  const DB_VERSIONS: Record<DbType, string[]> = {
    mysql: ['8.4', '8.0'],
    mariadb: ['11.4', '10.11', '10.6'],
    postgres: ['17', '16', '15']
  };
  const LOCALES = ['es_ES', 'en_US', 'es_MX', 'pt_BR', 'fr_FR', 'de_DE'];

  let name = $state('');
  let domainTouched = $state(false);
  let domain = $state('');
  let wpVersion = $state('');
  let locale = $state('es_ES');
  let phpVersion = $state('8.4');
  let dbType = $state<DbType>('mysql');
  let dbVersion = $state('8.0');
  let adminUser = $state('admin');
  let adminPassword = $state('');
  let showPass = $state(false);
  let adminEmail = $state('');
  let title = $state('');
  let ssl = $state(true);
  let oneClickAdmin = $state(true);
  let xdebug = $state(false);

  let versions = $state<WpVersion[]>([]);
  let versionsError = $state<string | null>(null);
  let submitting = $state(false);
  let error = $state<string | null>(null);

  function slugify(s: string) {
    return s
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '');
  }

  // dominio + título derivados del nombre, mientras el usuario no los edite a mano
  $effect(() => {
    const slug = slugify(name);
    if (!domainTouched) domain = slug ? `${slug}.test` : '';
  });

  // versión de DB válida al cambiar de motor
  $effect(() => {
    if (!DB_VERSIONS[dbType].includes(dbVersion)) dbVersion = DB_VERSIONS[dbType][0];
  });

  onMount(async () => {
    try {
      versions = await api.listWpVersions();
      const latest = versions.find((v) => v.status === 'latest');
      wpVersion = latest?.version ?? versions[0]?.version ?? '';
    } catch (e) {
      versionsError = String(e);
      wpVersion = '';
    }
  });

  async function submit(ev: Event) {
    ev.preventDefault();
    error = null;
    if (!name || !wpVersion || !adminPassword || !adminEmail) {
      error = 'Completa nombre, versión WP, contraseña y email.';
      return;
    }
    submitting = true;
    const req: NewSiteRequest = {
      name,
      domain: domain || undefined,
      wpVersion,
      locale,
      phpVersion,
      dbType,
      dbVersion,
      adminUser,
      adminPassword,
      adminEmail,
      title: title || name,
      ssl,
      oneClickAdmin,
      xdebug,
      group: null
    };
    try {
      await api.createSite(req);
      await goto('/');
    } catch (e) {
      error = String(e);
      submitting = false;
    }
  }

  // agrupación visual de versiones WP
  let grouped = $derived.by(() => {
    const latest = versions.filter((v) => v.status === 'latest');
    const rest = versions.filter((v) => v.status !== 'latest');
    return { latest, rest };
  });
</script>

<div class="mx-auto max-w-xl">
  <a href="/" class="mb-4 inline-block text-sm text-blue-500 underline">← Proyectos</a>
  <h1 class="mb-4 text-lg font-semibold">Nuevo proyecto</h1>

  {#if submitting}
    <div class="rounded border border-blue-300 bg-blue-50 p-6 text-sm dark:border-blue-900 dark:bg-blue-950">
      <p class="font-medium">Creando «{name}»…</p>
      <p class="mt-1 text-zinc-500">
        Descargando WordPress, levantando DB compartida e instalando. La primera vez puede tardar
        (descarga de imágenes Docker).
      </p>
    </div>
  {:else}
    <form class="flex flex-col gap-4 text-sm" onsubmit={submit}>
      {#if error}
        <div class="rounded border border-red-300 bg-red-50 px-3 py-2 text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
          {error}
        </div>
      {/if}

      <label class="flex flex-col gap-1">
        <span class="text-zinc-500">Nombre del proyecto</span>
        <input class="input" bind:value={name} placeholder="my-project" />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-zinc-500">Dominio local</span>
        <input
          class="input"
          bind:value={domain}
          oninput={() => (domainTouched = true)}
          placeholder="my-project.test"
        />
      </label>

      <fieldset class="rounded border border-zinc-200 p-3 dark:border-zinc-800">
        <legend class="px-1 text-xs uppercase tracking-wide text-zinc-500">WordPress</legend>
        <div class="grid grid-cols-2 gap-3">
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Versión</span>
            {#if versionsError}
              <input class="input" bind:value={wpVersion} placeholder="6.7.2" />
            {:else}
              <select class="input" bind:value={wpVersion}>
                {#each grouped.latest as v (v.version)}
                  <option value={v.version}>{v.version} (latest)</option>
                {/each}
                {#each grouped.rest as v (v.version)}
                  <option value={v.version}>
                    {v.version}{v.status === 'insecure' ? ' ⚠' : ''}
                  </option>
                {/each}
              </select>
            {/if}
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Idioma</span>
            <select class="input" bind:value={locale}>
              {#each LOCALES as l (l)}<option value={l}>{l}</option>{/each}
            </select>
          </label>
        </div>
        {#if versionsError}
          <p class="mt-2 text-xs text-amber-500">No se pudo cargar la lista de versiones; escribe una manualmente.</p>
        {/if}
      </fieldset>

      <fieldset class="rounded border border-zinc-200 p-3 dark:border-zinc-800">
        <legend class="px-1 text-xs uppercase tracking-wide text-zinc-500">Entorno</legend>
        <div class="grid grid-cols-3 gap-3">
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">PHP</span>
            <select class="input" bind:value={phpVersion}>
              {#each PHP_VERSIONS as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Motor DB</span>
            <select class="input" bind:value={dbType}>
              <option value="mysql">MySQL</option>
              <option value="mariadb">MariaDB</option>
              <option value="postgres">PostgreSQL</option>
            </select>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Versión DB</span>
            <select class="input" bind:value={dbVersion}>
              {#each DB_VERSIONS[dbType] as v (v)}<option value={v}>{v}</option>{/each}
            </select>
          </label>
        </div>
      </fieldset>

      <fieldset class="rounded border border-zinc-200 p-3 dark:border-zinc-800">
        <legend class="px-1 text-xs uppercase tracking-wide text-zinc-500">Administrador</legend>
        <div class="grid grid-cols-2 gap-3">
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Usuario</span>
            <input class="input" bind:value={adminUser} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Contraseña</span>
            <div class="flex gap-1">
              {#if showPass}
                <input class="input flex-1" type="text" bind:value={adminPassword} />
              {:else}
                <input class="input flex-1" type="password" bind:value={adminPassword} />
              {/if}
              <button
                type="button"
                class="rounded border border-zinc-300 px-2 dark:border-zinc-700"
                onclick={() => (showPass = !showPass)}
              >
                {showPass ? '🙈' : '👁'}
              </button>
            </div>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Email</span>
            <input class="input" type="email" bind:value={adminEmail} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-zinc-500">Título del sitio</span>
            <input class="input" bind:value={title} placeholder={name} />
          </label>
        </div>
      </fieldset>

      <fieldset class="rounded border border-zinc-200 p-3 dark:border-zinc-800">
        <legend class="px-1 text-xs uppercase tracking-wide text-zinc-500">Opciones</legend>
        <div class="flex flex-col gap-2">
          <label class="flex items-center gap-2"><input type="checkbox" bind:checked={ssl} /> SSL (HTTPS)</label>
          <label class="flex items-center gap-2"><input type="checkbox" bind:checked={oneClickAdmin} /> Auto-login al admin</label>
          <label class="flex items-center gap-2"><input type="checkbox" bind:checked={xdebug} /> XDebug</label>
        </div>
      </fieldset>

      <div class="flex justify-end gap-2">
        <a href="/" class="rounded px-4 py-2 hover:bg-zinc-200 dark:hover:bg-zinc-800">Cancelar</a>
        <button type="submit" class="rounded bg-blue-600 px-4 py-2 font-medium text-white hover:bg-blue-500">
          Crear proyecto
        </button>
      </div>
    </form>
  {/if}
</div>

<style>
  :global(.input) {
    @apply rounded border border-zinc-300 bg-white px-2 py-1.5 dark:border-zinc-700 dark:bg-zinc-900;
  }
</style>
