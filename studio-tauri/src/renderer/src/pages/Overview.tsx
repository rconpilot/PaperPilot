import { useEffect, useState } from 'react'
import type { ServerStatus, StudioSettings } from '../../../preload/index'
import type { Page } from '../App'
import { fmtDateTime } from '../lib'
import { ResultBox } from '../components/ui'

interface AuditRow {
  ts: string
  tool: string
  args: string
  outcome: string
  detail?: string
}

export default function Overview({
  status,
  history,
  go,
  refresh,
  settings,
  onSave
}: {
  status: ServerStatus | null
  history: number[]
  go: (p: Page) => void
  refresh: () => void
  settings: StudioSettings | null
  onSave: (next: StudioSettings) => Promise<void>
}): React.JSX.Element {
  const [plugins, setPlugins] = useState<number | null>(null)
  const [activity, setActivity] = useState<AuditRow[]>([])
  const [diag, setDiag] = useState<{ text?: string; error?: string } | null>(null)
  const [diagBusy, setDiagBusy] = useState(false)
  const [draft, setDraft] = useState({ rconHost: '', rconPort: 25575, rconPassword: '', serverDir: '' })
  const [connecting, setConnecting] = useState(false)

  useEffect(() => {
    if (settings) {
      setDraft({
        rconHost: settings.rconHost,
        rconPort: settings.rconPort,
        rconPassword: settings.rconPassword,
        serverDir: settings.serverDir
      })
    }
  }, [settings?.rconHost, settings?.rconPort, settings?.rconPassword, settings?.serverDir])

  const connect = async (): Promise<void> => {
    if (!settings) return
    setConnecting(true)
    await onSave({ ...settings, ...draft })
    refresh()
    setConnecting(false)
  }

  useEffect(() => {
    window.studio
      .callTool('list_plugins', {})
      .then((r) => {
        if (r.ok && r.text) {
          const header = r.text.match(/(\d+)\s+plugin/)
          if (header) {
            setPlugins(Number(header[1]))
          } else {
            const names = r.text.split('\n').filter((l) => l.trim().startsWith('-')).length
            setPlugins(names > 0 ? names : null)
          }
        }
      })
      .catch(() => undefined)
    window.studio
      .audit(6)
      .then((rows) => setActivity(rows as AuditRow[]))
      .catch(() => undefined)
  }, [status?.online])

  const runDiagnostics = async (): Promise<void> => {
    setDiagBusy(true)
    const r = await window.studio.callTool('diagnose_crash', { lines: 500 })
    setDiag(r.ok ? { text: r.text } : { error: r.error })
    setDiagBusy(false)
  }

  if (status && !status.online) {
    return (
      <div>
        <div className="page-title">Overview</div>
        <p className="page-sub">Your Paper server at a glance — live data via PaperPilot</p>
        <div className="card" style={{ maxWidth: 680 }}>
          <h3>Connect to your server</h3>
          <p className="muted" style={{ lineHeight: 1.6, marginBottom: 6 }}>
            Studio isn’t connected yet. Point it at your Paper server — local or on a VPS —
            with the RCON details from its <code>server.properties</code> (
            <code>enable-rcon=true</code>, <code>rcon.port</code>,{' '}
            <code>rcon.password</code>). Your details are saved on this device, so from now on
            Studio connects on its own every time it opens.
          </p>
          {status.error && (
            <p className="small" style={{ marginBottom: 16, color: 'var(--yellow)' }}>
              Last attempt: {status.error}
            </p>
          )}
          <div className="grid" style={{ gridTemplateColumns: '1fr 130px 1fr' }}>
            <label>
              <span className="small muted">Server address (host or IP)</span>
              <input
                className="input"
                style={{ marginTop: 6 }}
                placeholder="127.0.0.1 or play.example.com"
                value={draft.rconHost}
                onChange={(e) => setDraft({ ...draft, rconHost: e.target.value })}
              />
            </label>
            <label>
              <span className="small muted">RCON port</span>
              <input
                className="input"
                style={{ marginTop: 6 }}
                type="number"
                value={draft.rconPort}
                onChange={(e) => setDraft({ ...draft, rconPort: Number(e.target.value) })}
              />
            </label>
            <label>
              <span className="small muted">RCON password</span>
              <input
                className="input"
                style={{ marginTop: 6 }}
                type="password"
                value={draft.rconPassword}
                onChange={(e) => setDraft({ ...draft, rconPassword: e.target.value })}
              />
            </label>
          </div>
          <label style={{ display: 'block', marginTop: 14 }}>
            <span className="small muted">
              Server folder on THIS PC (optional — only for logs and backups; leave empty for a
              remote server)
            </span>
            <span style={{ display: 'flex', gap: 8, marginTop: 6 }}>
              <input
                className="input"
                style={{ flex: 1 }}
                placeholder="C:\\path\\to\\your\\paper-server"
                value={draft.serverDir}
                onChange={(e) => setDraft({ ...draft, serverDir: e.target.value })}
              />
              <button
                className="btn subtle"
                type="button"
                onClick={async () => {
                  const picked = await window.studio.pickFolder()
                  if (picked) setDraft({ ...draft, serverDir: picked })
                }}
              >
                Browse…
              </button>
            </span>
          </label>
          <div style={{ display: 'flex', gap: 10, marginTop: 18 }}>
            <button className="btn accent" onClick={connect} disabled={connecting || !settings}>
              {connecting ? 'Connecting…' : 'Connect'}
            </button>
            <button className="btn subtle" onClick={refresh}>
              Retry
            </button>
            <button className="btn subtle" onClick={() => go('settings')}>
              All settings
            </button>
          </div>
        </div>
      </div>
    )
  }

  const tps = status?.tps?.[0]
  const maxBar = 20
  return (
    <div>
      <div className="page-title">Overview</div>
      <p className="page-sub">
        Your Paper server at a glance — live data via PaperPilot
        {status?.version ? ` · ${status.version}` : ''}
      </p>
      <div className="grid" style={{ gridTemplateColumns: 'repeat(4, 1fr)' }}>
        <div className="card">
          <div className="muted small" style={{ fontWeight: 600 }}>
            TPS — 1 MIN AVG
          </div>
          <div className="stat-value" style={{ color: 'var(--green)' }}>
            {tps ?? '—'}
          </div>
          <div className="small muted">
            {tps !== undefined ? (tps >= 19.5 ? 'Healthy · above 19.5' : 'Under pressure') : 'Waiting for data…'}
          </div>
        </div>
        <div className="card">
          <div className="muted small" style={{ fontWeight: 600 }}>
            MSPT
          </div>
          <div className="stat-value" style={{ color: 'var(--accent)' }}>
            {status?.mspt != null ? `${status.mspt} ms` : '—'}
          </div>
          <div className="small muted">Server tick time (50 ms budget)</div>
        </div>
        <div className="card">
          <div className="muted small" style={{ fontWeight: 600 }}>
            PLAYERS ONLINE
          </div>
          <div className="stat-value">
            {status?.playersOnline ?? 0} / {status?.playersMax ?? '—'}
          </div>
          <div className="small muted">{status?.players?.join(', ') || 'Nobody online right now'}</div>
        </div>
        <div className="card">
          <div className="muted small" style={{ fontWeight: 600 }}>
            PLUGINS
          </div>
          <div className="stat-value" style={{ color: 'var(--mauve)' }}>
            {plugins ?? '—'}
          </div>
          <div className="small muted">From list_plugins, live</div>
        </div>
      </div>

      <div className="grid" style={{ gridTemplateColumns: '2fr 1fr', marginTop: 16 }}>
        <div className="card">
          <h3>Performance — TPS samples since Studio opened</h3>
          {history.length < 2 ? (
            <p className="muted">Collecting samples every 5 seconds…</p>
          ) : (
            <div
              style={{
                display: 'flex',
                alignItems: 'flex-end',
                gap: 4,
                height: 150,
                borderBottom: '1px solid var(--stroke)'
              }}
            >
              {history.map((v, i) => (
                <div
                  key={i}
                  title={`${v} TPS`}
                  style={{
                    flex: 1,
                    height: `${Math.max(4, (v / maxBar) * 100)}%`,
                    background: 'var(--accent)',
                    opacity: 0.35 + 0.65 * (v / maxBar),
                    borderRadius: '2px 2px 0 0'
                  }}
                />
              ))}
            </div>
          )}
          <div
            className="muted small"
            style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8 }}
          >
            <span>{history.length} samples</span>
            <span>now: {tps ?? '—'} TPS</span>
          </div>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <div className="card">
            <h3>Quick actions</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <button className="btn accent" onClick={() => go('backups')}>
                Backups
              </button>
              <button className="btn subtle" onClick={runDiagnostics} disabled={diagBusy}>
                {diagBusy ? 'Running…' : 'Run diagnostics'}
              </button>
            </div>
            {diag && (
              <div style={{ marginTop: 12 }}>
                <ResultBox text={diag.text} error={diag.error} />
              </div>
            )}
          </div>
          <div className="card">
            <h3>Recent activity</h3>
            {activity.length === 0 ? (
              <p className="muted small">
                Nothing yet — tool runs (yours and Copilot’s) are recorded in the audit log.
              </p>
            ) : (
              activity.slice(0, 5).map((a, i) => (
                <div
                  key={i}
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    gap: 10,
                    padding: '7px 0',
                    borderTop: i ? '1px solid var(--stroke)' : 'none'
                  }}
                >
                  <span>
                    <strong>{a.tool}</strong>{' '}
                    <span className={a.outcome === 'denied' ? 'error-text' : 'muted'}>
                      {a.outcome}
                    </span>
                  </span>
                  <span className="muted small">{fmtDateTime(a.ts)}</span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
