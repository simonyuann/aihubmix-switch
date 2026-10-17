import { useEffect } from 'react'
import type { ReactNode } from 'react'
import { ExternalIcon } from './Icons'

interface ModalProps {
  open: boolean
  title: string
  subtitle?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  width?: number
}

export function Modal({ open, title, subtitle, onClose, children, footer, width }: ModalProps) {
  useEffect(() => {
    if (!open) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [open, onClose])

  if (!open) return null

  return (
    <div className="modal-overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="modal" style={width ? { maxWidth: width } : undefined} role="dialog" aria-modal="true">
        <div className="modal-head">
          <div>
            <h2>{title}</h2>
            {subtitle ? <p>{subtitle}</p> : null}
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer ? <div className="modal-foot">{footer}</div> : null}
      </div>
    </div>
  )
}

export function LinkButton({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button
      className="link-btn"
      onClick={() => {
        void window.api.openExternal(href)
      }}
    >
      {children}
      <ExternalIcon size={13} />
    </button>
  )
}
