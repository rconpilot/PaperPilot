import { memo, useCallback, useEffect, useRef, useState } from 'react'
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
import logoUrl from './assets/logo.png'
import { AnimatePresence, motion } from 'motion/react'

// Pages are memoized so the 5-second status poll (which re-renders App)
// does not re-render pages whose props did not actually change — that
// cascade was a visible source of jank on page switches and theme changes.
const MemoTools = memo(Tools)
const MemoBackups = memo(Backups)
const MemoLogs = memo(Logs)
const MemoAudit = memo(Audit)
const MemoCopilot = memo(Copilot)

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
  const [maximized, setMaximized] = useState(false)
  const [navQuery, setNavQuery] = useState('')
  const statusRef = useRef<ServerStatus | null>(null)

  useEffect(() => {
    const syncMax = (): void => {
      window.studio
        .isMaximized()
        .then(setMaximized)
        .catch(() => undefined)
    }
    syncMax()
    window.addEventListener('resize', syncMax)
    return () => window.removeEventListener('resize', syncMax)
  }, [])

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
      <aside className="nav" data-tauri-drag-region>
        <div className="app-row">
          <motion.img
            className="app-icon app-logo"
            src={logoUrl}
            alt="PaperPilot Studio logo"
            whileHover={{ scale: 1.1, rotate: -4 }}
            whileTap={{ scale: 0.92 }}
            transition={{ type: 'spring', stiffness: 400, damping: 17 }}
          />
          <div className="app-name">PaperPilot Studio</div>
        </div>
        <input
          className="nav-search"
          style={{ width: 'calc(100% - 16px)', color: 'var(--text)', outline: 'none' }}
          placeholder="Search pages…"
          value={navQuery}
          onChange={(e) => setNavQuery(e.target.value)}
        />
        <nav className="nav-items">
          {navItems
            .filter((n) => n.label.toLowerCase().includes(navQuery.trim().toLowerCase()))
            .map((n) => (
            <button
              key={n.id}
              className={`nav-item ${shownPage === n.id ? 'active' : ''}`}
              onClick={() => setPage(n.id)}
            >
              {shownPage === n.id && (
                <motion.span
                  className="nav-active-bar"
                  layoutId="nav-active-bar"
                  transition={{ type: 'spring', stiffness: 500, damping: 40 }}
                />
              )}
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
                ? 'Not connected — connect from Overview'
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
          <AnimatePresence>
          {userPop && settings && (
            <motion.div
              className="user-pop"
              initial={{ opacity: 0, y: 6, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 6, scale: 0.97 }}
              transition={{ type: 'spring', stiffness: 500, damping: 34 }}
            >
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
            </motion.div>
          )}
          </AnimatePresence>
        </div>
      </aside>
      <div className="main">
        <div className="titlebar" data-tauri-drag-region>
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
            <button
              className="win-btn"
              onClick={() => window.studio.minimize()}
              title="Minimize"
              aria-label="Minimize"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <path d="M0 5 H10" stroke="currentColor" strokeWidth="1" />
              </svg>
            </button>
            <button
              className="win-btn"
              onClick={() => window.studio.maximize()}
              title={maximized ? 'Restore' : 'Maximize'}
              aria-label={maximized ? 'Restore' : 'Maximize'}
            >
              {maximized ? (
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                  <path
                    d="M2.5 2.5 V0.75 H9.25 V7.5 H7.5 M0.75 2.5 H7.5 V9.25 H0.75 Z"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1"
                  />
                </svg>
              ) : (
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                  <rect
                    x="0.75"
                    y="0.75"
                    width="8.5"
                    height="8.5"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1"
                  />
                </svg>
              )}
            </button>
            <button
              className="win-btn close"
              onClick={() => window.studio.close()}
              title="Close"
              aria-label="Close"
            >
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <path
                  d="M0.75 0.75 L9.25 9.25 M9.25 0.75 L0.75 9.25"
                  stroke="currentColor"
                  strokeWidth="1"
                />
              </svg>
            </button>
          </div>
        </div>
        <div className="content">
          {/* Opacity-only, no exit wait: the old mode="wait" crossfade with a
              vertical slide made every page switch feel like a ~1s freeze in
              the webview (exit had to finish before the new page painted). */}
          <motion.div
            key={shownPage}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.12, ease: 'easeOut' }}
          >
            {shownPage === 'overview' && (
              <Overview
                status={status}
                history={history}
                go={setPage}
                refresh={poll}
                settings={settings}
                onSave={saveSettings}
              />
            )}
            {shownPage === 'tools' && settings && <MemoTools settings={settings} />}
            {shownPage === 'players' && settings && <Players status={status} settings={settings} />}
            {shownPage === 'backups' && settings && <MemoBackups settings={settings} />}
            {shownPage === 'logs' && <MemoLogs />}
            {shownPage === 'audit' && <MemoAudit />}
            {shownPage === 'copilot' && settings && (
              <MemoCopilot settings={settings} onSave={saveSettings} status={status} />
            )}
            {shownPage === 'settings' && settings && (
              <SettingsPage settings={settings} onSave={saveSettings} status={status} />
            )}
            {!settings && shownPage !== 'overview' && <p className="muted">Loading settings…</p>}
          </motion.div>
        </div>
      </div>
    </div>
  )
}

export default App
