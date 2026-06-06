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
  repos: GithubRepo[];
  // Legacy: el backend los pliega en `repos` al cargar; ya no se usan en la UI.
  theme?: GithubRepo | null;
  plugins?: GithubRepo[];
}

/** Repo git encontrado en disco bajo wp-content (registrado o huérfano). */
export interface DetectedRepo {
  path: string;
  name: string;
  remote: string | null;
  branch: string | null;
  registered: boolean;
}

/** Espejo de `config::CloneInfo`: metadatos del proyecto padre. */
export interface CloneInfo {
  parentId: string;
  parentDirname: string;
  snapshotId: string;
  createdAt: string;
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
  cloneOf?: CloneInfo | null;
  /** Rutas (relativas a public) excluidas del punto de guardado. */
  snapshotExcludes?: string[];
}

/** Espejo de `snapshot::SnapshotMeta`. */
export interface SnapshotMeta {
  id: string;
  label: string;
  createdAt: string;
  dbName: string;
  dbType: DbType;
  /** Bytes del code.tar.zst; 0 en snapshots antiguos. */
  codeBytes: number;
  /** Bytes del db.sql; 0 en snapshots antiguos. */
  dbBytes: number;
  /** Rutas extra excluidas en este snapshot; vacío en snapshots antiguos. */
  excludes?: string[];
}

/** Espejo de `snapshot::ExcludableEntry`: carpeta candidata a excluir. */
export interface ExcludableEntry {
  /** Ruta relativa a public, p. ej. `wp-content/updraft`. */
  path: string;
  /** Tamaño en disco en bytes. */
  bytes: number;
  /** true si es carpeta de backup conocida (recomendado excluir). */
  known: boolean;
  /** Plugin de origen si `known`, p. ej. "UpdraftPlus". */
  label: string | null;
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

/// Espejo de `localwp::LocalSite`: un sitio de LocalWP candidato a importar.
export interface LocalSite {
  id: string;
  name: string;
  domain: string;
  path: string;
  phpVersion: string;
  dbVersion: string;
  multisite: boolean;
  xdebug: boolean;
  alreadyImported: boolean;
}

/// Espejo de `localwp::ImportResult`.
export interface ImportResult {
  site: SiteConfig;
  note: string | null;
}

/// Espejo de `config::DisconnectedSite`: una carpeta de `~/panel-wp/` que ya no
/// está en el panel pero sigue en disco, candidata a re-importar.
export interface DisconnectedSite {
  folderName: string;
  path: string;
  name: string;
  domain: string;
  phpVersion: string;
  dbVersion: string;
  dbType: DbType;
  hasDump: boolean;
  kind: 'preserved' | 'reconstructed';
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


export interface WpUser {
  ID: string;
  user_login: string;
  display_name: string;
  roles: string;
}

/** Una entrada del log de volcados de DB (espejo de `DumpLogEntry` en Rust). */
export interface DumpLogEntry {
  timestamp: string;
  siteId: string;
  siteName: string;
  dbName: string;
  file: string;
  bytes: number;
  /** `auto` | `stop` | `manual` */
  source: string;
}
