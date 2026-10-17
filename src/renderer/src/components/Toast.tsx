import { AlertIcon, CheckIcon, LoaderIcon } from './Icons'

export type ToastKind = 'success' | 'error' | 'info' | 'loading'

export interface ToastState {
  id: number
  kind: ToastKind
  message: string
}

export function Toast({ toast }: { toast: ToastState | null }) {
  if (!toast) return null
  return (
    <div className={`toast toast-${toast.kind}`} role="status">
      <span className="toast-icon">
        {toast.kind === 'success' ? (
          <CheckIcon size={16} />
        ) : toast.kind === 'loading' ? (
          <LoaderIcon size={16} />
        ) : toast.kind === 'error' ? (
          <AlertIcon size={16} />
        ) : (
          <span className="toast-dot" />
        )}
      </span>
      <span>{toast.message}</span>
    </div>
  )
}
