import { invoke } from '@tauri-apps/api/core';
import type {
  SiteConfig,
  SiteState,
  WpVersion,
  NewSiteRequest,
  GhStatus,
  Endpoint,
  SystemStatus,
  Migration,
  LocalSite,
  ImportResult
} from './types';

// Capa fina sobre los comandos IPC de Tauri (src-tauri/src/lib.rs).

export const api = {
  getSites: () => invoke<SiteState[]>('get_sites'),
  startSite: (id: string) => invoke<void>('start_site', { id }),
  stopSite: (id: string) => invoke<void>('stop_site', { id }),
  stopAllSites: () => invoke<void>('stop_all_sites'),
  execWpcli: (id: string, args: string[]) => invoke<string>('exec_wpcli', { id, args }),
  createSite: (req: NewSiteRequest) => invoke<SiteConfig>('create_site', { req }),
  listWpVersions: () => invoke<WpVersion[]>('list_wp_versions'),
  panelEndpoint: () => invoke<Endpoint>('panel_endpoint'),
  // Fase 4: estado del sistema / primera configuración
  systemStatus: () => invoke<SystemStatus>('system_status'),
  createPanelNetwork: () => invoke<void>('create_panel_network'),
  resetEndpoint: () => invoke<void>('reset_endpoint'),
  migrateSite: (id: string) => invoke<Migration>('migrate_site', { id }),
  deleteSite: (id: string) => invoke<void>('delete_site', { id }),
  listLocalwpSites: () => invoke<LocalSite[]>('list_localwp_sites'),
  importLocalwpSite: (id: string) => invoke<ImportResult>('import_localwp_site', { id }),
  openAdmin: (id: string) => invoke<void>('open_admin', { id }),
  streamLogs: (id: string) => invoke<void>('stream_logs', { id }),
  stopLogs: (id: string) => invoke<void>('stop_logs', { id }),
  listPlugins: (id: string) => invoke<string>('list_plugins', { id }),
  listThemes: (id: string) => invoke<string>('list_themes', { id }),
  ghStatus: () => invoke<GhStatus>('gh_status'),
  ghClone: (id: string, kind: 'theme' | 'plugin', repo: string, branch: string) =>
    invoke<SiteConfig>('gh_clone', { id, kind, repo, branch }),
  ghPull: (id: string, path: string, branch: string) =>
    invoke<string>('gh_pull', { id, path, branch }),
  ghPullAll: (id: string) => invoke<string>('gh_pull_all', { id }),
  ghRemove: (id: string, kind: 'theme' | 'plugin', path: string) =>
    invoke<SiteConfig>('gh_remove', { id, kind, path }),
  regenerateSsl: (id: string) => invoke<void>('regenerate_ssl', { id }),
  setSiteGroup: (id: string, group: string | null) =>
    invoke<SiteConfig>('set_site_group', { id, group }),
  // Fase 3
  setSiteMinio: (id: string, enabled: boolean) =>
    invoke<SiteConfig>('set_site_minio', { id, enabled }),
  exportDb: (id: string) => invoke<string>('export_db', { id }),
  installCliWrapper: () => invoke<string>('install_cli_wrapper'),
  openMailpit: () => invoke<void>('open_mailpit'),
  openMinio: () => invoke<void>('open_minio'),
  featureStub: (feature: string) => invoke<string>('feature_stub', { feature })
};
