import { useEffect, useRef, useState, type ReactNode } from 'react'
import { AnimatePresence, motion } from 'motion/react'

export function Modal({
  title,
  children,
  onClose
}: {
  title: string
  children: ReactNode
  onClose: () => void
}): React.JSX.Element {
  return (
    <motion.div
      className="modal-backdrop"
      onClick={onClose}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
    >
      <motion.div
        className="modal"
        onClick={(e) => e.stopPropagation()}
        initial={{ opacity: 0, scale: 0.95, y: 10 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 6 }}
        transition={{ type: 'spring', stiffness: 480, damping: 32 }}
      >
        <h3>{title}</h3>
        {children}
      </motion.div>
    </motion.div>
  )
}

export function ConfirmModal({
  title,
  body,
  confirmLabel,
  danger,
  onConfirm,
  onCancel
}: {
  title: string
  body: string
  confirmLabel: string
  danger?: boolean
  onConfirm: () => void
  onCancel: () => void
}): React.JSX.Element {
  return (
    <Modal title={title} onClose={onCancel}>
      <p className="muted" style={{ lineHeight: 1.6, marginBottom: 18 }}>
        {body}
      </p>
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end' }}>
        <button className="btn subtle" onClick={onCancel}>
          Cancel
        </button>
        <button className={`btn ${danger ? 'danger' : 'accent'}`} onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </Modal>
  )
}

export function ResultBox({ text, error }: { text?: string; error?: string }): React.JSX.Element {
  return <div className="result-box">{error ? `Error: ${error}` : text}</div>
}

/**
 * A designed dropdown that matches the app's themes, replacing the native
 * <select> (which renders as a raw OS control in the webview and clashes
 * with every theme). Options are {value, label} pairs.
 */
export function Select({
  value,
  options,
  onChange,
  width,
  height
}: {
  value: string
  options: { value: string; label: string }[]
  onChange: (v: string) => void
  width?: number
  height?: number
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDoc = (e: MouseEvent): void => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDoc)
      document.removeEventListener('keydown', onKey)
    }
  }, [open ])

  const current = options.find((o) => o.value === value)
  return (
    <div className="select" ref={ref} style={width ? { width } : undefined}>
      <button
        type="button"
        className="input select-btn"
        style={height ? { height } : undefined}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <span className="select-label">{current?.label ?? value}</span>
        <svg
          className={`select-chevron ${open ? 'open' : ''}`}
          width="10"
          height="10"
          viewBox="0 0 10 10"
          aria-hidden="true"
        >
          <path
            d="M1.5 3.5 L5 7 L8.5 3.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="select-menu"
            role="listbox"
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -4, scale: 0.98 }}
            transition={{ duration: 0.12, ease: 'easeOut' }}
          >
            {options.map((o) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === value}
                className={`select-item ${o.value === value ? 'selected' : ''}`}
                onClick={() => {
                  onChange(o.value)
                  setOpen(false)
                }}
              >
                <span style={{ flex: 1 }}>{o.label}</span>
                {o.value === value && <span className="select-check">✓</span>}
              </button>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

export function Toggle({
  on,
  onChange
}: {
  on: boolean
  onChange: (v: boolean) => void
}): React.JSX.Element {
  return (
    <button
      className={`toggle ${on ? 'on' : ''}`}
      onClick={() => onChange(!on)}
      aria-pressed={on}
    />
  )
}
