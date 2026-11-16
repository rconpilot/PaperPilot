import { useCallback, useEffect, useRef, useState } from 'react'
import type { ServerStatus, StudioSettings } from '../../preload/index'
import { ROLE_LABEL, ROLE_TIER, accentTextFor } from './lib'
import Overview from './pages/Overview'
import Tools from './pages/Tools'
import Players from './pages/Players'
import Backups from './pages/Backups'
import Logs from './pages/Logs'
import Audit from './pages/Audit'
import Copilot from './pages/Copilot'
import SettingsPage from './pages/SettingsPage'

export type Page =
  | 'overview'
  | 'tools'
  | 'players'
  | 'backups'
  | 'logs'
  | 'audit'
  | 'copilot'
  | 'settings'

const NAV: { id: Page; label: string; icon: string }[] = [
  { id: 'overview', label: 'Overview', icon: '▦' },
  { id: 'tools', label: 'Tools', icon: '⚙' },
  { id: 'players', label: 'Players', icon: '👥' },
  { id: 'backups', label: 'Backups', icon: '🗄' },
  { id: 'logs', label: 'Logs', icon: '☰' },
  { id: 'audit', label: 'Audit Log', icon: '✓' },
  { id: 'copilot', label: 'AI Copilot', icon: '✦' },
  { id: 'settings', label: 'Settings', icon: '⛭' }
]

function App(): React.JSX.Element {
  const [page, setPage] = useState<Page>('overview')
  const [settings, setSettings] = useState<StudioSettings | null>(null)
  const [status, setStatus] = useState<ServerStatus | null>(null)
  const [history, setHistory] = useState<number[]>([])
  const [stars, setStars] = useState<number | null>(null)
  const [userPop, setUserPop] = useState(false)
  const statusRef = useRef<ServerStatus | null>(null)

  const poll = useCallback(async () => {
    try {
      const s = await window.studio.status()
      statusRef.current = s
      setStatus(s)
      if (s.online && s.tps && s.tps.length > 0) {
        setHistory((h) => [...h.slice(-29), s.tps![0]])
      }
    } catch {
      setStatus({ online: false, raw: '', error: 'Bridge unavailable' })
    }
  }, [])

  useEffect(() => {
    window.studio.getSettings().then(setSettings).catch(() => undefined)
    poll()
    const id = setInterval(poll, 5000)
    const fetchStars = (): void => {
      window.studio
        .github()
        .then((g) => setStars(g.stars))
        .catch(() => undefined)
    }
    fetchStars()
    const gid = setInterval(fetchStars, 10 * 60 * 1000)
    return () => {
      clearInterval(id)
      clearInterval(gid)
    }
  }, [poll])

  useEffect(() => {
    if (!settings) return
    document.documentElement.dataset.theme = settings.theme
    document.documentElement.style.setProperty('--accent', settings.accent)
    document.documentElement.style.setProperty(
      '--accent-text',
      accentTextFor(settings.accent)
    )
  }, [settings])

  const saveSettings = useCallback(async (next: StudioSettings) => {
    const saved = await window.studio.setSettings(next)
    setSettings(saved)
    poll()
  }, [poll])

  const online = status?.online ?? false
  const me = settings?.team.find((m) => m.id === settings.activeUserId) ?? settings?.team[0]
  const canManage = !me || me.role === 'owner' || me.role === 'admin'
  const navItems = NAV.filter((n) => n.id !== 'settings' || canManage)
  const shownPage: Page = page === 'settings' && !canManage ? 'overview' : page

  const switchUser = (id: string): void => {
    if (!settings) return
    setUserPop(false)
    saveSettings({ ...settings, activeUserId: id }).catch(() => undefined)
  }

  return (
    <div className="app">
      <aside className="nav">
        <div className="app-row">
          <div className="app-icon">P</div>
          <div className="app-name">PaperPilot Studio</div>
        </div>
        <div className="nav-search">Search tools, players…</div>
        <nav className="nav-items">
          {navItems.map((n) => (
            <button
              key={n.id}
              className={`nav-item ${shownPage === n.id ? 'active' : ''}`}
              onClick={() => setPage(n.id)}
            >
              <span style={{ width: 18, textAlign: 'center' }}>{n.icon}</span>
              {n.label}
            </button>
          ))}
        </nav>
        <div className="nav-spacer" />
        <div className="nav-status">
          <div className="status-line">
            <span
              className="dot"
              style={{ background: online ? 'var(--green)' : 'var(--red)' }}
            />
            {status === null ? 'Connecting…' : online ? 'Server online' : 'Server offline'}
          </div>
          <div className="status-sub">
            {online && status
              ? `TPS ${status.tps?.[0] ?? '—'} · MSPT ${status.mspt ?? '—'} · ${status.playersOnline ?? 0} players`
              : status?.error
                ? 'Check Settings → Connection'
                : 'Trying to reach the server…'}
          </div>
          <button
            className="profile-row"
            style={{ width: '100%', textAlign: 'left', cursor: 'pointer', background: 'none' }}
            onClick={() => setUserPop((v) => !v)}
            title="Switch user"
          >
            <span className="avatar" style={{ width: 30, height: 30, fontSize: 13 }}>
              {(me?.name ?? 'O').slice(0, 1).toUpperCase()}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600, fontSize: 13 }}>{me?.name ?? 'Owner'}</div>
              <div className="muted" style={{ fontSize: 11 }}>
                {ROLE_LABEL[me?.role ?? 'owner']} · {ROLE_TIER[me?.role ?? 'owner']} tier
              </div>
            </div>
            <span className="muted" style={{ fontSize: 11 }}>
              ⇅
            </span>
          </button>
          {userPop && settings && (
            <div className="user-pop">
              <div className="small muted" style={{ padding: '4px 10px 8px', fontWeight: 600 }}>
                Switch user — permissions apply instantly
              </div>
              {settings.team.map((m) => (
                <button
                  key={m.id}
                  className={`nav-item ${m.id === me?.id ? 'active' : ''}`}
                  onClick={() => switchUser(m.id)}
                >
                  <span className="avatar" style={{ width: 24, height: 24, fontSize: 11 }}>
                    {m.name.slice(0, 1).toUpperCase()}
                  </span>
                  <span style={{ flex: 1 }}>{m.name}</span>
                  <span className="role-badge">{ROLE_LABEL[m.role]}</span>
                </button>
              ))}
              {canManage && (
                <button
                  className="nav-item"
                  onClick={() => {
                    setUserPop(false)
                    setPage('settings')
                  }}
                >
                  <span style={{ width: 24, textAlign: 'center' }}>＋</span>
                  <span style={{ flex: 1 }}>Manage team…</span>
                </button>
              )}
            </div>
          )}
        </div>
      </aside>
      <div className="main">
        <div className="titlebar">
          <button
            className="gh-chip"
            onClick={() => window.studio.openExternal('https://github.com/rconpilot/PaperPilot')}
            title="Open the PaperPilot project on GitHub"
          >
            <span>⌂ GitHub · rconpilot/PaperPilot</span>
            {stars !== null && (
              <span>
                <span className="star">★</span> {stars}
              </span>
            )}
          </button>
          <div className="win-controls">
            <button className="win-btn" onClick={() => window.studio.minimize()}>
              —
            </button>
            <button className="win-btn" onClick={() => window.studio.maximize()}>
              □
            </button>
            <button className="win-btn close" onClick={() => window.studio.close()}>
              ✕
            </button>
          </div>
        </div>
        <div className="content">
          {shownPage === 'overview' && (
            <Overview status={status} history={history} go={setPage} refresh={poll} />
          )}
          {shownPage === 'tools' && settings && <Tools settings={settings} />}
          {shownPage === 'players' && settings && <Players status={status} settings={settings} />}
          {shownPage === 'backups' && settings && <Backups settings={settings} />}
          {shownPage === 'logs' && <Logs />}
          {shownPage === 'audit' && <Audit />}
          {shownPage === 'copilot' && settings && <Copilot settings={settings} />}
          {shownPage === 'settings' && settings && (
            <SettingsPage settings={settings} onSave={saveSettings} status={status} />
          )}
          {!settings && shownPage !== 'overview' && <p className="muted">Loading settings…</p>}
        </div>
      </div>
    </div>
  )
}

export default App
