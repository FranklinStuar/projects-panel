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
  migrationPending: boolean;
  lastMigratedAt: string | null;
}

export type SiteStatus = 'running' | 'stopped' | 'migrationPending';

export interface SiteState {
  config: SiteConfig;
  status: SiteStatus;
}
