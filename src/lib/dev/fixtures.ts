// Datos de muestra para el modo mock (testing de la GUI sin backend ni Docker).
// Solo se cargan cuando VITE_MOCK_IPC=1 (ver src/lib/dev/mock-ipc.ts).

import type {
  SiteConfig,
  SiteState,
  Endpoint,
  SystemStatus,
  LocalSite,
  WpVersion
} from '$lib/types';

/** Construye un SiteConfig de muestra con valores por defecto razonables. */
export function makeSite(over: Partial<SiteConfig> & { id: string; name: string }): SiteConfig {
  const slug = over.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
  return {
    path: `/home/u/panel-wp/${slug}`,
    domain: `${slug}.test`,
    group: null,
    createdAt: '2026-05-01T10:00:00Z',
    services: {
      php: { version: '8.3' },
      nginx: { ssl: true },
      db: { type: 'mysql', version: '8.0', dbName: `${slug.replace(/-/g, '_')}_db` }
    },
    github: { theme: null, plugins: [] },
    oneClickAdmin: true,
    xdebugEnabled: false,
    headless: false,
    frontendFramework: null,
    minio: false,
    migrationPending: false,
    lastMigratedAt: null,
    ...over
  };
}

/** Endpoint con puerto alterno (coexistencia con LocalWP) → URL muestra `:8443`. */
export const endpoint: Endpoint = {
  loopbackIp: '127.0.0.1',
  httpPort: 8080,
  httpsPort: 8443
};

export const systemStatus: SystemStatus = {
  dockerOk: true,
  networkOk: true,
  dnsmasqOk: true,
  mkcertOk: true,
  cliWrapperOk: false, // ✗ para ver el botón "Instalar"
  plasmoidOk: false, // ✗
  endpoint,
  projectsRoot: '/home/u/panel-wp',
  configDir: '/home/u/.config/wordpress-panel'
};

/** Tres proyectos: uno corriendo, uno parado, uno pendiente de migración. */
export function initialSites(): SiteState[] {
  return [
    {
      config: makeSite({ id: 'site-running', name: 'Tienda Demo', group: 'Cliente A' }),
      status: 'running'
    },
    {
      config: makeSite({ id: 'site-stopped', name: 'Blog Personal', group: 'Cliente A' }),
      status: 'stopped'
    },
    {
      config: makeSite({
        id: 'site-pending',
        name: 'Sitio Importado',
        group: 'LocalWP',
        migrationPending: true
      }),
      status: 'migrationPending'
    }
  ];
}

export function initialLocalSites(): LocalSite[] {
  return [
    {
      id: 'lw-1',
      name: 'Proyecto Viejo',
      domain: 'proyecto-viejo.test',
      path: '/home/u/Local Sites/proyecto-viejo',
      phpVersion: '8.4',
      dbVersion: '8.0',
      multisite: false,
      xdebug: false,
      alreadyImported: false
    },
    {
      id: 'lw-2',
      name: 'Sitio Importado',
      domain: 'sitio-importado.test',
      path: '/home/u/Local Sites/sitio-importado',
      phpVersion: '8.3',
      dbVersion: '8.0',
      multisite: true,
      xdebug: true,
      alreadyImported: true
    }
  ];
}

export const wpVersions: WpVersion[] = [
  { version: '6.7.1', status: 'latest' },
  { version: '6.6.2', status: 'outdated' },
  { version: '6.4.0', status: 'insecure' }
];
