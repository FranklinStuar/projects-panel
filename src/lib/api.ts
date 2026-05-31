import { invoke } from '@tauri-apps/api/core';
import type { SiteState } from './types';

// Capa fina sobre los comandos IPC de Tauri (src-tauri/src/main.rs).

export const api = {
  getSites: () => invoke<SiteState[]>('get_sites'),
  startSite: (id: string) => invoke<void>('start_site', { id }),
  stopSite: (id: string) => invoke<void>('stop_site', { id }),
  stopAllSites: () => invoke<void>('stop_all_sites'),
  execWpcli: (id: string, args: string[]) => invoke<string>('exec_wpcli', { id, args })
};
