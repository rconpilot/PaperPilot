import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { openUrl } from '@tauri-apps/plugin-opener'
import { open as openDialog } from '@tauri-apps/plugin-dialog'

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
  /** AI Copilot: OpenAI-compatible endpoint root ('' = not configured). */
  aiBaseUrl: string
  /** AI Copilot API key (local settings only; may be empty for local models). */
  aiApiKey: string
  /** AI Copilot model id. */
  aiModel: string
}

export interface AiToolCall {
  id: string
  name: string
  /** Raw JSON argument string exactly as the model produced it. */
  arguments: string
}

export interface AiChatRequest {
  baseUrl: string
  apiKey: string
  model: string
  messages: Record<string, unknown>[]
  tools: Record<string, unknown>[]
}

export interface AiChatResponse {
  content: string | null
  toolCalls: AiToolCall[]
}

/**
 * The renderer's bridge to the app backend. In the Electron edition this was
 * an IPC preload; in the Tauri edition the exact same surface is implemented
 * with Tauri commands backed by the Rust port of the PaperPilot core, so the
 * renderer code is shared between both editions unchanged.
 */
const studio = {
  listTools: (): Promise<ToolInfo[]> => invoke('list_tools'),
  callTool: (
    name: string,
    args: Record<string, unknown>
  ): Promise<{ ok: boolean; text?: string; error?: string }> =>
    invoke('call_tool', { name, args }),
  status: (): Promise<ServerStatus> => invoke('status'),
  backups: (): Promise<{
    entries: { name: string; sizeBytes: number; modified: string }[]
    totalBytes: number
    dir: string
  }> => invoke('backups'),
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
  > => invoke('audit', { limit }),
  github: (): Promise<{ stars: number | null; url: string }> => invoke('github'),
  /** One AI Copilot round-trip. The model only proposes; tool execution
   *  stays on callTool with all of its guards. */
  aiChat: (req: AiChatRequest): Promise<AiChatResponse> => invoke('ai_chat', { req }),
  /** Native folder picker for the server directory. Null when cancelled. */
  pickFolder: async (): Promise<string | null> => {
    const picked = await openDialog({ directory: true, multiple: false })
    return typeof picked === 'string' ? picked : null
  },
  openExternal: (url: string): void => {
    void openUrl(url)
  },
  getSettings: (): Promise<StudioSettings> => invoke('get_settings'),
  setSettings: (next: StudioSettings): Promise<StudioSettings> =>
    invoke('set_settings', { next }),
  minimize: (): void => {
    void getCurrentWindow().minimize()
  },
  maximize: (): void => {
    void getCurrentWindow().toggleMaximize()
  },
  isMaximized: (): Promise<boolean> => getCurrentWindow().isMaximized(),
  close: (): void => {
    void getCurrentWindow().close()
  }
}

export type StudioAPI = typeof studio

declare global {
  interface Window {
    studio: StudioAPI
  }
}

/** Install the bridge on window before the app renders (Tauri has no preload). */
export function installStudio(): void {
  window.studio = studio
}
