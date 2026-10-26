import { useCallback, useEffect, useRef, useState } from 'react'
import type { AppState, WalletBalance } from '../../../shared/types'
import { LoaderIcon, RefreshIcon } from './Icons'

export const walletMoney = (value: number | null | undefined) =>
  typeof value === 'number' && Number.isFinite(value) ? `¥${value.toFixed(2)}` : '—'

/**
 * Wallet balance for the home page. The recharge dialog used to be the only place
 * showing it, so users had to open it just to read their balance.
 */
export function BalanceChip({
  state,
  refresh,
  onOpen
}: {
  state: AppState
  refresh: number
  onOpen: () => void
}) {
  const [wallet, setWallet] = useState<WalletBalance | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const seq = useRef(0)
  const loggedIn = state.account.loggedIn
  const load = useCallback(async () => {
    const id = ++seq.current
    if (!loggedIn) {
      setWallet(null)
      setError('')
      setLoading(false)
      return
    }
    setLoading(true)
    try {
      const next = await window.api.payments.balance()
      if (id === seq.current) {
        setWallet(next)
        setError('')
      }
    } catch (err) {
      if (id === seq.current) setError(err instanceof Error ? err.message : String(err))
    } finally {
      if (id === seq.current) setLoading(false)
    }
  }, [loggedIn])
  useEffect(() => {
    void load()
    return () => {
      ++seq.current
    }
  }, [load, refresh, state.account.userId, state.relayBaseUrl])
  useEffect(() => {
    if (!loggedIn) return
    // Balances also change while a payment settles or from another device.
    const onVisible = () => {
      if (document.visibilityState === 'visible') void load()
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => document.removeEventListener('visibilitychange', onVisible)
  }, [loggedIn, load])
  if (!loggedIn) return null
  const value = wallet ? walletMoney(wallet.balance) : error ? '—' : '…'
  return (
    <button
      type="button"
      className={`balance-chip${error ? ' balance-chip-error' : ''}`}
      onClick={onOpen}
      title={error || '查看余额与充值记录'}
      aria-label={`余额 ${wallet ? value : '加载中'}`}
    >
      <span>余额</span>
      <strong>{value}</strong>
      {loading ? <LoaderIcon size={12} /> : <RefreshIcon size={12} />}
    </button>
  )
}
