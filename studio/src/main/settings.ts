import { app } from 'electron'
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'

export type ThemeId =
  | 'fluent-dark'
  | 'fluent-light'
  | 'catppuccin'
  | 'neo-brutalist'
  | 'neo-sky'
  | 'claude'

/** A local Studio user. Roles map onto the PaperPilot core's tiers:
 *  owner/admin → admin, operator → operator, viewer → readonly. */
export type TeamRole = 'owner' | 'admin' | 'operator' | 'viewer'

export interface TeamMember {
  id: string
  name: string
  role: TeamRole
}

/** Studio's own settings — stored locally in userData, never in the repo. */
export interface StudioSettings {
  rconHost: string
  rconPort: number
  /** Kept only in this local settings file; the core also accepts the
   *  PAPERPILOT_RCON_PASSWORD / RCON_PASSWORD environment variables, which
   *  take precedence when set. */
  rconPassword: string
  /** Minecraft server directory — used to resolve logs/latest.log and backups/. */
  serverDir: string
  /** run_command allowlist (enforced by the PaperPilot core). */
  allowlist: string[]
  /** Require an explicit confirmation before admin-tier tools run from the UI. */
  requireApproval: boolean
  theme: ThemeId
  accent: string
  density: 'comfortable' | 'compact'
  /** Local team: the owner plus any admins the owner added with limited roles. */
  team: TeamMember[]
  /** Id of the team member currently using Studio (drives tool permissions). */
  activeUserId: string
}

export const DEFAULT_SETTINGS: StudioSettings = {
  rconHost: '127.0.0.1',
  rconPort: 25575,
  rconPassword: '',
  serverDir: '',
  allowlist: ['say', 'list', 'tps', 'save-all', 'whitelist', 'weather', 'time set day'],
  requireApproval: true,
  theme: 'claude',
  accent: '#D97757',
  density: 'comfortable',
  team: [{ id: 'owner', name: 'Owner', role: 'owner' }],
  activeUserId: 'owner'
}

function settingsPath(): string {
  return join(app.getPath('userData'), 'studio-settings.json')
}

export function loadSettings(): StudioSettings {
  try {
    const p = settingsPath()
    if (existsSync(p)) {
      const raw = JSON.parse(readFileSync(p, 'utf8')) as Partial<StudioSettings>
      return { ...DEFAULT_SETTINGS, ...raw }
    }
  } catch {
    /* fall through to defaults */
  }
  return { ...DEFAULT_SETTINGS }
}

export function saveSettings(next: StudioSettings): StudioSettings {
  const p = settingsPath()
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, JSON.stringify(next, null, 2), 'utf8')
  return next
}
