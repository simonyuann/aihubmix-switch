import { useEffect, useId, useRef, type ReactNode } from 'react'
import { ExternalIcon } from './Icons'

interface ModalProps {
  open: boolean
  title: string
  subtitle?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  width?: number
  busy?: boolean
}

export function Modal({
  open,
  title,
  subtitle,
  onClose,
  children,
  footer,
  width,
  busy = false
}: ModalProps) {
  const ref = useRef<HTMLDivElement>(null)
  const titleId = useId()
  const closeRef = useRef(onClose)
  const busyRef = useRef(busy)
  closeRef.current = onClose
  busyRef.current = busy
  useEffect(() => {
    if (!open) return
    const previous = document.activeElement as HTMLElement | null
    const overflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    const container = ref.current
    const first = container?.querySelector<HTMLElement>(
      'input:not([disabled]), select:not([disabled]), [data-autofocus]'
    )
    ;(first || container)?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        if (!busyRef.current) closeRef.current()
      }
      if (event.key !== 'Tab') return
      const elements = Array.from(
        container?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), summary, a[href], [tabindex="0"]'
        ) || []
      ).filter((el) => !el.closest('details:not([open])') || el.tagName === 'SUMMARY')
      const first = elements[0]
      const last = elements[elements.length - 1]
      if (!first) {
        event.preventDefault()
        container?.focus()
        return
      }
      if (
        event.shiftKey &&
        (document.activeElement === first || document.activeElement === container)
      ) {
        event.preventDefault()
        last?.focus()
      } else if (
        !event.shiftKey &&
        (document.activeElement === last || document.activeElement === container)
      ) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      document.body.style.overflow = overflow
      if (previous?.isConnected) previous.focus()
    }
  }, [open])
  if (!open) return null
  return (
    <div
      className="modal-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose()
      }}
    >
      <div
        ref={ref}
        tabIndex={-1}
        className="modal"
        style={width ? { maxWidth: width } : undefined}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <div className="modal-head">
          <div>
            <h2 id={titleId}>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          <button
            className="icon-btn close-btn"
            onClick={onClose}
            disabled={busy}
            aria-label="关闭"
          >
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  )
}

export function LinkButton({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button
      className="link-btn"
      onClick={() => {
        void window.api.openExternal(href).catch(() => undefined)
      }}
    >
      {children}
      <ExternalIcon size={13} />
    </button>
  )
}
