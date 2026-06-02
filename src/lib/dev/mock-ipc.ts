// Capa de mock de IPC para testing de la GUI SIN backend ni Docker.
//
// Activa `window.__TAURI_INTERNALS__` con `mockIPC` y responde cada comando de
// `api.ts` con los fixtures de `fixtures.ts`. Solo se importa cuando
// VITE_MOCK_IPC=1 (ver src/routes/+layout.ts). En `pnpm tauri dev` real este
// módulo no se carga.
//
// Los flujos largos (migrar/importar/borrar) emiten líneas por el evento
// `op-log` con pequeños retardos, así la consola `OpConsole` se ve poblándose en
// vivo — exactamente como con el backend real (src-tauri/src/progress.rs).

import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { Migration, ImportResult, SiteState } from '$lib/types';
import {
  endpoint,
  systemStatus,
  initialSites,
  initialLocalSites,
  makeSite,
  wpVersions
} from './fixtures';

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Estado mutable en memoria (se reinicia al recargar la página).
let sites = initialSites();
let localSites = initialLocalSites();
const status = { ...systemStatus };

function find(id: string): SiteState | undefined {
  return sites.find((s) => s.config.id === id);
}

/** Emite varias líneas `op-log` con retardo para simular progreso real. */
async function progress(lines: string[], step = 350) {
  for (const l of lines) {
    await sleep(step);
    await emit('op-log', l);
  }
}

export function installMockIpc() {
  mockIPC(async (cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    const id = args.id as string;
    switch (cmd) {
      // --- lectura ---
      // Devolvemos copias frescas (como el backend real, que deserializa cada
      // vez): si retornáramos la misma referencia que mutamos, Svelte 5 no
      // detectaría el cambio al reasignar el estado.
      case 'get_sites':
        return sites.map((s) => ({ config: { ...s.config }, status: s.status }));
      case 'panel_endpoint':
        return endpoint;
      case 'system_status':
        return { ...status };
      case 'list_wp_versions':
        return wpVersions;
      case 'list_localwp_sites':
        return localSites.map((l) => ({ ...l }));
      case 'gh_status':
        return { installed: true, authenticated: false, user: null };

      // --- ciclo de vida ---
      case 'start_site': {
        const s = find(id);
        if (s) s.status = 'running';
        return null;
      }
      case 'stop_site': {
        const s = find(id);
        if (s) s.status = 'stopped';
        return null;
      }
      case 'stop_all_sites':
        sites.forEach((s) => (s.status = 'stopped'));
        return null;

      // --- migración (emite progreso) ---
      case 'migrate_site': {
        const s = find(id);
        await progress([
          `▶ Migrando «${s?.config.name}»…`,
          '• Arrancando base de datos y creando el esquema…',
          '• Generando certificado SSL (mkcert)…',
          '• Encendiendo el proyecto…',
          '• Regenerando wp-config.php…',
          '• Importando base de datos (42 MB), espera…',
          '• Ajustando URLs del sitio…',
          `✓ «${s?.config.name}» migrado y encendido.`
        ]);
        if (s) {
          s.config.migrationPending = false;
          s.config.lastMigratedAt = new Date().toISOString();
          s.status = 'running';
        }
        return { site: s!.config, note: null } satisfies Migration;
      }

      case 'repair_autologin': {
        const s = find(id);
        if (s) s.config.oneClickAdmin = true;
        return s ? { ...s.config } : null;
      }

      // --- cancelar importación (borra el proyecto) ---
      case 'delete_site':
        sites = sites.filter((s) => s.config.id !== id);
        return null;

      // --- import desde LocalWP (emite progreso) ---
      case 'import_localwp_site': {
        const lw = localSites.find((l) => l.id === id);
        await progress([
          `▶ Importando «${lw?.name}» desde LocalWP…`,
          '• Copiando archivos (app/public, puede tardar)…',
          '• Copiando dump de la base de datos (42 MB)…',
          `✓ «${lw?.name}» importado → usa «Migrar y encender» en Proyectos.`
        ]);
        const cfg = makeSite({
          id: `imported-${id}`,
          name: lw?.name ?? 'Importado',
          group: 'LocalWP',
          migrationPending: true
        });
        sites = [...sites, { config: cfg, status: 'migrationPending' }];
        if (lw) lw.alreadyImported = true;
        return { site: cfg, note: null } satisfies ImportResult;
      }

      case 'create_site': {
        const req = args.req as { name: string; phpVersion: string; dbVersion: string };
        const cfg = makeSite({ id: `new-${Date.now()}`, name: req.name });
        cfg.services.php.version = req.phpVersion;
        cfg.services.db.version = req.dbVersion;
        sites = [...sites, { config: cfg, status: 'stopped' }];
        return cfg;
      }

      case 'set_site_group': {
        const s = find(id);
        if (s) s.config.group = (args.group as string | null) ?? null;
        return s?.config;
      }
      case 'set_site_minio': {
        const s = find(id);
        if (s) s.config.minio = args.enabled as boolean;
        return s?.config;
      }

      // --- acciones de settings ---
      case 'create_panel_network':
        status.networkOk = true;
        return null;
      case 'install_cli_wrapper':
        status.cliWrapperOk = true;
        return 'Wrappers instalados en ~/.local/bin';
      case 'reset_endpoint':
        return null;
      case 'export_db':
        return '/home/u/panel-wp/demo/app/sql/db-2026.sql';

      // --- comandos sin efecto observable en mock ---
      case 'open_admin':
      case 'open_mailpit':
      case 'open_minio':
      case 'open_terminal':
      case 'stream_logs':
      case 'stop_logs':
      case 'regenerate_ssl':
        return null;
      case 'list_plugins':
      case 'list_themes':
        return '(mock) sin elementos';
      case 'feature_stub':
        return `(mock) ${String(args.feature)} no disponible`;

      default:
        console.warn('[mock-ipc] comando no mockeado:', cmd, args);
        return null;
    }
  }, { shouldMockEvents: true });

  console.info('[mock-ipc] activo — GUI con fixtures, sin backend.');
}

installMockIpc();
