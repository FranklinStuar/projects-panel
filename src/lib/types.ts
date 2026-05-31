// Espejo de los modelos de `src-tauri/src/config.rs` (serde camelCase).

export type DbType = 'mysql' | 'mariadb' | 'postgres';

export interface PhpService {
  version: string;
}

export interface NginxService {
  ssl: boolean;
}

export interface DbService {
  type: DbType;
  version: string;
  dbName: string;
}

export interface Services {
  php: PhpService;
  nginx: NginxService;
  db: DbService;
}

export interface GithubRepo {
  repo: string;
  branch: string;
  path: string;
}

export interface GithubConfig {
  theme: GithubRepo | null;
  plugins: GithubRepo[];
}

export interface SiteConfig {
  id: string;
  name: string;
  path: string;
  domain: string;
  group: string | null;
  createdAt: string;
  services: Services;
  github: GithubConfig;
  oneClickAdmin: boolean;
  xdebugEnabled: boolean;
  headless: boolean;
  frontendFramework: string | null;
  minio: boolean;
  migrationPending: boolean;
  lastMigratedAt: string | null;
}

/// Espejo de `config::Endpoint`: dónde publica el panel en el host.
export interface Endpoint {
  loopbackIp: string;
  httpPort: number;
  httpsPort: number;
}

/// URL pública del sitio según el endpoint (puerto solo si no es el estándar).
export function siteUrl(ep: Endpoint, domain: string, ssl: boolean): string {
  if (ssl) {
    return ep.httpsPort === 443 ? `https://${domain}` : `https://${domain}:${ep.httpsPort}`;
  }
  return ep.httpPort === 80 ? `http://${domain}` : `http://${domain}:${ep.httpPort}`;
}

/// Espejo de `system::SystemStatus`: estado de los prerequisitos del panel.
export interface SystemStatus {
  dockerOk: boolean;
  networkOk: boolean;
  dnsmasqOk: boolean;
  mkcertOk: boolean;
  cliWrapperOk: boolean;
  plasmoidOk: boolean;
  endpoint: Endpoint;
  projectsRoot: string;
  configDir: string;
}

/// Espejo de `migrate::Migration`: config migrada + aviso opcional.
export interface Migration {
  site: SiteConfig;
  note: string | null;
}

export type SiteStatus = 'running' | 'stopped' | 'migrationPending';

export interface SiteState {
  config: SiteConfig;
  status: SiteStatus;
}

export interface WpVersion {
  version: string;
  status: string; // "latest" | "outdated" | "insecure"
}

export interface GhStatus {
  installed: boolean;
  authenticated: boolean;
  user: string | null;
}

export interface NewSiteRequest {
  name: string;
  domain?: string;
  wpVersion: string;
  locale?: string;
  phpVersion: string;
  dbType: DbType;
  dbVersion: string;
  adminUser: string;
  adminPassword: string;
  adminEmail: string;
  title: string;
  ssl?: boolean;
  oneClickAdmin?: boolean;
  xdebug?: boolean;
  headless?: boolean;
  frontendFramework?: string | null;
  minio?: boolean;
  group?: string | null;
}
