import { useCallback, useEffect, useRef, useState } from 'react'
import type { UpdateStatus } from '../../shared/types'

const initial: UpdateStatus = {
  phase: 'idle',
  currentVersion: '',
  version: null,
  notes: null,
  downloaded: 0,
  total: null,
  canInstall: false,
  format: '',
  error: null
}
export function useAppUpdates() {
  const [status, setStatus] = useState<UpdateStatus>(initial)
  const [pending, setPending] = useState(false)
  const pendingRef = useRef(false)
  const mounted = useRef(true)
  const run = useCallback(async (action: 'check' | 'download' | 'install') => {
    if (pendingRef.current) return
    pendingRef.current = true
    setPending(true)
    try {
      const value =
        action === 'install'
          ? await window.api.updates.install(true)
          : await window.api.updates[action]()
      if (mounted.current) setStatus(value)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      try {
        const actual = await window.api.updates.status()
        if (mounted.current) setStatus({ ...actual, error: actual.error || message })
      } catch {
        if (mounted.current) setStatus((s) => ({ ...s, phase: 'error', error: message }))
      }
    } finally {
      pendingRef.current = false
      if (mounted.current) setPending(false)
    }
  }, [])
  useEffect(() => {
    mounted.current = true
    let disposed = false
    let unlisten: (() => void) | undefined
    void window.api.updates
      .onStatus((value) => {
        if (!disposed) setStatus(value)
      })
      .then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
      .catch(() => {
        /* A manual check can still return its status. */
      })
    void window.api.updates
      .status()
      .then((value) => {
        if (!disposed) setStatus(value)
      })
      .catch(() => {})
    const startup = setTimeout(() => {
      void run('check')
    }, 1500)
    const daily = setInterval(
      () => {
        void run('check')
      },
      24 * 60 * 60 * 1000
    )
    return () => {
      mounted.current = false
      disposed = true
      clearTimeout(startup)
      clearInterval(daily)
      unlisten?.()
    }
  }, [run])
  return {
    status,
    pending,
    check: () => run('check'),
    download: () => run('download'),
    install: () => run('install')
  }
}
export type UpdateController = ReturnType<typeof useAppUpdates>
