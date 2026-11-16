import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

export interface ToolInfo {
  name: string
  description: string
  tier: string
  args: { name: string; description: string; required: boolean; kind: 'string' | 'number' | 'boolean' }[]
}

export interface ServerStatus {
  online: boolean
  version?: string
  playersOnline?: number
  playersMax?: number
  players?: string[]
  tps?: number[]
  mspt?: number | null
  raw: string
  error?: string
}

export type ThemeId =
  | 'fluent-dark'
  | 'fluent-light'
  | 'catppuccin'
  | 'neo-brutalist'
  | 'neo-sky'
  | 'claude'

export type TeamRole = 'owner' | 'admin' | 'operator' | 'viewer'

export interface TeamMember {
  id: string
  name: string
  role: TeamRole
}

export interface StudioSettings {
  rconHost: string
  rconPort: number
  rconPassword: string
  serverDir: string
  allowlist: string[]
  requireApproval: boolean
  theme: ThemeId
  accent: string
  density: 'comfortable' | 'compact'
  team: TeamMember[]
  activeUserId: string
}

const studio = {
  listTools: (): Promise<ToolInfo[]> => ipcRenderer.invoke('tools:list'),
  callTool: (
    name: string,
    args: Record<string, unknown>
  ): Promise<{ ok: boolean; text?: string; error?: string }> =>
    ipcRenderer.invoke('tools:call', name, args),
  status: (): Promise<ServerStatus> => ipcRenderer.invoke('pp:status'),
  backups: (): Promise<{
    entries: { name: string; sizeBytes: number; modified: string }[]
    totalBytes: number
    dir: string
  }> => ipcRenderer.invoke('pp:backups'),
  audit: (
    limit?: number
  ): Promise<
    {
      ts: string
      tool: string
      args: string
      outcome: string
      tier: string
      detail?: string
      user?: string
    }[]
  > => ipcRenderer.invoke('pp:audit', limit),
  github: (): Promise<{ stars: number | null; url: string }> => ipcRenderer.invoke('pp:github'),
  openExternal: (url: string): void => ipcRenderer.send('shell:open', url),
  getSettings: (): Promise<StudioSettings> => ipcRenderer.invoke('settings:get'),
  setSettings: (next: StudioSettings): Promise<StudioSettings> =>
    ipcRenderer.invoke('settings:set', next),
  minimize: (): void => ipcRenderer.send('win:minimize'),
  maximize: (): void => ipcRenderer.send('win:maximize'),
  close: (): void => ipcRenderer.send('win:close')
}

export type StudioAPI = typeof studio

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('studio', studio)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.studio = studio
}
