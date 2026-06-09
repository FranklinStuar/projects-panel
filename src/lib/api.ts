import { invoke } from '@tauri-apps/api/core';
import type {
  SiteConfig,
  SiteState,
  WpVersion,
  NewSiteRequest,
  GhStatus,
  DetectedRepo,
  Endpoint,
  SystemStatus,
  Migration,
  LocalSite,
  ImportResult,
  DisconnectedSite,
  SnapshotMeta,
  ExcludableEntry,
  WpUser,
  DumpLogEntry
} from './types';

// Capa fina sobre los comandos IPC de Tauri (src-tauri/src/lib.rs).

export const api = {
  getSites: () => invoke<SiteState[]>('get_sites'),
  startSite: (id: string) => invoke<void>('start_site', { id }),
  stopSite: (id: string) => invoke<void>('stop_site', { id }),
  stopAllSites: () => invoke<void>('stop_all_sites'),
  execWpcli: (id: string, args: string[]) => invoke<string>('exec_wpcli', { id, args }),
  openTerminal: (id: string) => invoke<void>('open_terminal', { id }),
  createSite: (req: NewSiteRequest) => invoke<SiteConfig>('create_site', { req }),
  listWpVersions: () => invoke<WpVersion[]>('list_wp_versions'),
  panelEndpoint: () => invoke<Endpoint>('panel_endpoint'),
  // Fase 4: estado del sistema / primera configuración
  systemStatus: () => invoke<SystemStatus>('system_status'),
  createPanelNetwork: () => invoke<void>('create_panel_network'),
  resetEndpoint: () => invoke<void>('reset_endpoint'),
  migrateSite: (id: string) => invoke<Migration>('migrate_site', { id }),
  deleteSite: (id: string, deleteFolder: boolean) =>
    invoke<void>('delete_site', { id, deleteFolder }),
  listLocalwpSites: () => invoke<LocalSite[]>('list_localwp_sites'),
  importLocalwpSite: (id: string) => invoke<ImportResult>('import_localwp_site', { id }),
  listDisconnectedSites: () => invoke<DisconnectedSite[]>('list_disconnected_sites'),
  importDisconnectedSite: (folderName: string) =>
    invoke<ImportResult>('import_disconnected_site', { folderName }),
  openAdmin: (id: string, userId?: number) => invoke<void>('open_admin', { id, userId }),
  listWpUsers: (id: string) => invoke<WpUser[]>('list_wp_users', { id }),
  repairAutologin: (id: string) => invoke<SiteConfig>('repair_autologin', { id }),
  repairAllPhpIni: () => invoke<string>('repair_all_php_ini'),
  openSite: (id: string) => invoke<void>('open_site', { id }),
  openFolder: (id: string) => invoke<void>('open_folder', { id }),
  streamLogs: (id: string) => invoke<void>('stream_logs', { id }),
  stopLogs: (id: string) => invoke<void>('stop_logs', { id }),
  listPlugins: (id: string) => invoke<string>('list_plugins', { id }),
  listThemes: (id: string) => invoke<string>('list_themes', { id }),
  ghStatus: () => invoke<GhStatus>('gh_status'),
  ghClone: (
    id: string,
    kind: 'theme' | 'plugin' | 'muplugin',
    repo: string,
    branch: string,
    path?: string,
  ) => invoke<SiteConfig>('gh_clone', { id, kind, repo, branch, path: path ?? null }),
  ghPull: (id: string, path: string, branch: string) =>
    invoke<string>('gh_pull', { id, path, branch }),
  ghPullAll: (id: string) => invoke<string>('gh_pull_all', { id }),
  ghRemove: (id: string, path: string) =>
    invoke<SiteConfig>('gh_remove', { id, path }),
  ghScan: (id: string) => invoke<DetectedRepo[]>('gh_scan', { id }),
  ghRegister: (id: string, path: string) =>
    invoke<SiteConfig>('gh_register', { id, path }),
  openVscode: (id: string) => invoke<void>('open_vscode', { id }),
  regenerateSsl: (id: string) => invoke<void>('regenerate_ssl', { id }),
  setSiteGroup: (id: string, group: string | null) =>
    invoke<SiteConfig>('set_site_group', { id, group }),
  // Fase 3
  setSiteMinio: (id: string, enabled: boolean) =>
    invoke<SiteConfig>('set_site_minio', { id, enabled }),
  exportDb: (id: string) => invoke<string>('export_db', { id }),
  // Log de volcados de DB: revisar y limpiar
  dumpLog: () => invoke<DumpLogEntry[]>('dump_log'),
  cleanDumpLog: (before: string | null, dbName: string | null) =>
    invoke<number>('clean_dump_log', { before, dbName }),
  installCliWrapper: () => invoke<string>('install_cli_wrapper'),
  openMailpit: () => invoke<void>('open_mailpit'),
  openMinio: () => invoke<void>('open_minio'),
  openAdminer: (id: string) => invoke<void>('open_adminer', { id }),
  featureStub: (feature: string) => invoke<string>('feature_stub', { feature }),
  // Fase 5: clones temporales + puntos de guardado
  createSnapshot: (id: string, label: string) =>
    invoke<SnapshotMeta>('create_snapshot', { id, label }),
  listSnapshots: (id: string) => invoke<SnapshotMeta[]>('list_snapshots', { id }),
  deleteSnapshot: (id: string, snapshotId: string) =>
    invoke<void>('delete_snapshot', { id, snapshotId }),
  detectExcludable: (id: string) =>
    invoke<ExcludableEntry[]>('detect_excludable', { id }),
  setSnapshotExcludes: (id: string, excludes: string[]) =>
    invoke<void>('set_snapshot_excludes', { id, excludes }),
  createClone: (parentId: string, snapshotId: string) =>
    invoke<SiteConfig>('create_clone', { parentId, snapshotId }),
  // Worktree-projects: probar una rama de un repo (theme/plugin) en aislamiento
  createWorktreeSite: (
    parentId: string,
    targetPath: string,
    branch: string,
    sharedDb: boolean,
    baseBranch?: string,
  ) =>
    invoke<SiteConfig>('create_worktree_site', {
      parentId,
      targetPath,
      branch,
      baseBranch: baseBranch ?? null,
      sharedDb
    }),
  removeWorktreeSite: (id: string, deleteBranch: boolean) =>
    invoke<void>('remove_worktree_site', { id, deleteBranch }),
  listWorktrees: (parentId: string) =>
    invoke<SiteConfig[]>('list_worktrees', { parentId })
};
