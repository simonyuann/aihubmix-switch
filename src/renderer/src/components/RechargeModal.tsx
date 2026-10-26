import { useCallback, useEffect, useRef, useState } from 'react'
import QRCode from 'qrcode'
import type { AppState, RechargeOrder, RechargeOverview } from '../../../shared/types'
import { CheckIcon, ExternalIcon, LoaderIcon, RefreshIcon } from './Icons'
import { Modal } from './Modal'

const statusNames: Record<string, string> = {
  PENDING: '待付款',
  PAID: '已付款，正在到账',
  RECHARGING: '正在到账',
  COMPLETED: '充值成功',
  EXPIRED: '已过期',
  CANCELLED: '已取消',
  FAILED: '充值失败',
  REFUNDED: '已退款',
  REFUND_REQUESTED: '已申请退款',
  REFUNDING: '退款中',
  REFUND_PENDING: '退款处理中',
  PARTIALLY_REFUNDED: '部分退款',
  REFUND_FAILED: '退款失败'
}
const pending = (order: RechargeOrder | null) =>
  !!order && ['PENDING', 'PAID', 'RECHARGING'].includes(order.status)
const money = (value: number | null | undefined) =>
  typeof value === 'number' && Number.isFinite(value) ? `¥${value.toFixed(2)}` : '—'
const message = (error: unknown) => (error instanceof Error ? error.message : String(error))

export function RechargeModal({
  open,
  state,
  onClose,
  onLogin,
  onPaid
}: {
  open: boolean
  state: AppState
  onClose: () => void
  onLogin: () => void
  onPaid: () => void
}) {
  const [overview, setOverview] = useState<RechargeOverview | null>(null)
  const [amount, setAmount] = useState('50')
  const [method, setMethod] = useState('')
  const [order, setOrder] = useState<RechargeOrder | null>(null)
  const [qr, setQr] = useState('')
  const [qrError, setQrError] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [uncertain, setUncertain] = useState(false)
  const [now, setNow] = useState(Date.now())
  const gate = useRef(false)
  const mounted = useRef(true)
  const paid = useRef(new Set<number>())
  const revision = useRef(0)
  const links = useRef(new Map<number, Pick<RechargeOrder, 'qrCode' | 'payUrl'>>())
  const paidCallback = useRef(onPaid)
  paidCallback.current = onPaid
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const result = await window.api.payments.overview()
      if (!mounted.current) return
      setOverview(result)
      setMethod((current) =>
        result.methods.some((m) => m.id === current) ? current : result.methods[0]?.id || ''
      )
      setUncertain(false)
    } catch (err) {
      if (mounted.current) {
        setError(message(err))
        setOverview((current) => (current ? { ...current, balance: null } : null))
      }
    } finally {
      if (mounted.current) setLoading(false)
    }
  }, [])
  useEffect(() => {
    if (!open || !state.account.loggedIn) return
    setError('')
    void refresh()
  }, [open, state.account.loggedIn, refresh])
  const applyOrder = useCallback(
    (next: RechargeOrder) => {
      if (!mounted.current) return
      if (next.qrCode || next.payUrl)
        links.current.set(next.id, { qrCode: next.qrCode, payUrl: next.payUrl })
      setOrder((previous) => ({
        ...(previous?.id === next.id ? previous : {}),
        ...next,
        ...links.current.get(next.id)
      }))
      setOverview((current) =>
        current
          ? {
              ...current,
              orders: [next, ...current.orders.filter((o) => o.id !== next.id)],
              balance: next.status === 'COMPLETED' ? null : current.balance
            }
          : current
      )
      if (next.status === 'COMPLETED' && !paid.current.has(next.id)) {
        paid.current.add(next.id)
        void refresh()
        paidCallback.current()
      }
    },
    [refresh]
  )
  useEffect(() => {
    if (!open || !order || !pending(order)) return
    let stopped = false
    let polling = false
    const poll = async () => {
      if (gate.current || polling) return
      polling = true
      const currentRevision = revision.current
      try {
        const next = await window.api.payments.order(order.id)
        if (!stopped && currentRevision === revision.current) applyOrder(next)
      } catch (err) {
        if (!stopped && currentRevision === revision.current) setError(message(err))
      } finally {
        polling = false
      }
    }
    void poll()
    const timer = setInterval(() => void poll(), 5000)
    const clock = setInterval(() => setNow(Date.now()), 1000)
    return () => {
      stopped = true
      clearInterval(timer)
      clearInterval(clock)
    }
  }, [open, order?.id, order?.status, applyOrder])
  useEffect(() => {
    let stopped = false
    setQr('')
    setQrError('')
    const content = order?.qrCode || order?.payUrl
    if (content && order?.status === 'PENDING') {
      void QRCode.toDataURL(content, { width: 260, margin: 2, errorCorrectionLevel: 'M' })
        .then((data) => {
          if (!stopped) setQr(data)
        })
        .catch(() => {
          if (!stopped) setQrError('付款码生成失败，可打开支付页面，或先取消订单后重新生成。')
        })
    }
    return () => {
      stopped = true
    }
  }, [order?.id, order?.qrCode, order?.payUrl, order?.status])
  async function operate(action: () => Promise<RechargeOrder>, creation = false) {
    if (gate.current) return
    gate.current = true
    ++revision.current
    setBusy(true)
    setError('')
    try {
      const next = await action()
      if (mounted.current) {
        applyOrder(next)
        setUncertain(false)
      }
    } catch (err) {
      if (mounted.current) {
        setError(message(err))
        if (creation) setUncertain(true)
      }
    } finally {
      gate.current = false
      if (mounted.current) setBusy(false)
    }
  }
  async function openPage(url: string) {
    try {
      await window.api.openExternal(url)
    } catch (err) {
      setError(message(err))
    }
  }
  const selected = overview?.methods.find((m) => m.id === method)
  const minimum = Math.max(0.01, overview?.minAmount || 0, selected?.minAmount || 0)
  const maximum = Math.min(
    ...[overview?.maxAmount, selected?.maxAmount].filter((v): v is number => !!v && v > 0)
  )
  const value = Number(amount)
  const valid =
    !!selected && /^\d{1,9}(\.\d{0,2})?$/.test(amount) && value >= minimum && value <= maximum
  const outstanding = overview?.orders.some((o) => o.status === 'PENDING') || pending(order)
  const expires = order ? Date.parse(order.expiresAt) : NaN
  const currentTime = Math.max(now, Date.now())
  const expired = Number.isFinite(expires) && currentTime >= expires
  const remaining = Math.max(0, Math.ceil((expires - currentTime) / 1000))
  return (
    <Modal open={open} title="账户充值" onClose={onClose} width={510} busy={busy}>
      {!state.account.loggedIn ? (
        <div className="form-stack">
          <p className="fine-print">登录你的 AIHubMix 账号后，为该账号的钱包充值。</p>
          <button className="btn primary block" onClick={onLogin}>
            登录后充值
          </button>
        </div>
      ) : (
        <div className="recharge-content">
          <div className="recharge-wallet">
            <div>
              <span>账户余额</span>
              <strong>{money(overview?.balance)}</strong>
              <small>{overview?.email || state.account.email}</small>
            </div>
            <button
              className="icon-btn"
              aria-label="刷新余额与充值记录"
              disabled={loading || busy}
              onClick={() => {
                setError('')
                void refresh()
              }}
            >
              <RefreshIcon size={17} className={loading ? 'spin' : ''} />
            </button>
          </div>
          {error && (
            <div className="notice notice-error" role="alert">
              {error}
              {error.includes('登录') && (
                <button className="link-btn" disabled={busy} onClick={onLogin}>
                  重新登录
                </button>
              )}
            </div>
          )}
          {loading && !overview && (
            <p role="status">
              <LoaderIcon size={16} /> 正在读取充值信息…
            </p>
          )}
          {overview && !overview.enabled && (
            <p className="fine-print">当前未开放余额充值，请稍后重试。</p>
          )}
          {overview?.enabled && overview.methods.length === 0 && (
            <p className="fine-print">当前没有可用的扫码充值通道，可前往网页充值查看其他方式。</p>
          )}
          {overview?.enabled && overview.methods.length > 0 && !pending(order) && (
            <form
              className="form-stack"
              onSubmit={(event) => {
                event.preventDefault()
                if (valid && !loading && !uncertain && !outstanding)
                  void operate(() => window.api.payments.create({ amount, method }), true)
              }}
            >
              <label className="field">
                <span>充值金额（元）</span>
                <input
                  aria-label="充值金额（元）"
                  inputMode="decimal"
                  value={amount}
                  disabled={busy}
                  onChange={(event) => setAmount(event.target.value)}
                />
              </label>
              <div className="recharge-presets">
                {[10, 50, 100, 200]
                  .filter((n) => n >= minimum && n <= maximum)
                  .map((n) => (
                    <button
                      key={n}
                      type="button"
                      className={`btn sm ${amount === String(n) ? 'outline-blue' : ''}`}
                      disabled={busy}
                      onClick={() => setAmount(String(n))}
                    >
                      {money(n)}
                    </button>
                  ))}
              </div>
              <label className="field">
                <span>支付方式</span>
                <select
                  aria-label="支付方式"
                  value={method}
                  disabled={busy}
                  onChange={(event) => setMethod(event.target.value)}
                >
                  {overview.methods.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
                </select>
              </label>
              <p className="fine-print">
                最低 {money(minimum)}
                {Number.isFinite(maximum) ? `，最高 ${money(maximum)}` : ''}。
                {selected && selected.feeRate > 0 ? `通道手续费 ${selected.feeRate}%。` : ''}
                实付和到账金额将在订单中显示。
              </p>
              <button
                className="btn primary block"
                type="submit"
                disabled={busy || loading || !valid || uncertain || !!outstanding}
              >
                {busy ? <LoaderIcon size={16} /> : null}生成付款码
              </button>
            </form>
          )}
          {uncertain && (
            <p className="fine-print">下单结果尚未确认，请先刷新充值记录，避免重复下单。</p>
          )}
          {outstanding && !order && (
            <p className="fine-print">存在待付款订单，请先在下方记录中查询或取消。</p>
          )}
          {order && (
            <div className="recharge-order" aria-label="当前充值订单">
              <strong role="status">
                {order.status === 'COMPLETED' && <CheckIcon size={16} />}
                {statusNames[order.status] || order.status}
              </strong>
              <div className="recharge-totals">
                <span>
                  实付金额<strong>{money(order.payAmount)}</strong>
                </span>
                <span>
                  到账金额<strong>{money(order.amount)}</strong>
                </span>
              </div>
              {!!order.bonusAmount && order.bonusAmount > 0 && (
                <p className="fine-print">到账金额已含赠送 {money(order.bonusAmount)}</p>
              )}
              {order.status === 'PENDING' && !expired && (
                <>
                  {qr ? (
                    <img className="recharge-qr" src={qr} alt="当前订单付款二维码" />
                  ) : (
                    <p className="fine-print">
                      {qrError ||
                        (order.qrCode || order.payUrl
                          ? '正在生成付款码…'
                          : '本次会话没有该订单的付款码；可先查询状态，或取消后重新生成。')}
                    </p>
                  )}
                  {qr && (
                    <p className="fine-print">
                      请使用{order.method === 'wxpay' ? '微信' : '支付宝'}扫一扫
                    </p>
                  )}
                  {order.payUrl && (
                    <button
                      className="link-btn"
                      disabled={busy}
                      onClick={() => void openPage(order.payUrl!)}
                    >
                      打开支付页面 <ExternalIcon size={12} />
                    </button>
                  )}
                  {Number.isFinite(expires) && (
                    <p className="fine-print">
                      付款码剩余 {Math.floor(remaining / 60)}:
                      {String(remaining % 60).padStart(2, '0')}
                    </p>
                  )}
                </>
              )}
              {order.status === 'PENDING' && expired && (
                <p className="fine-print">付款码已到期，正在确认订单状态。</p>
              )}
              <small className="recharge-trade">订单号：{order.tradeNo || order.id}</small>
              <div className="recharge-actions">
                <button
                  className="btn sm"
                  disabled={busy}
                  onClick={() => void operate(() => window.api.payments.verify(order.id))}
                >
                  查询付款结果
                </button>
                {order.status === 'PENDING' && (
                  <button
                    className="link-btn"
                    disabled={busy}
                    onClick={() => void operate(() => window.api.payments.cancel(order.id))}
                  >
                    取消订单
                  </button>
                )}
              </div>
            </div>
          )}
          {!!overview?.orders.length && (
            <div className="recharge-history">
              <h3>最近充值记录</h3>
              {overview.orders.map((item) => (
                <button
                  key={item.id}
                  className={`recharge-history-row ${item.id === order?.id ? 'selected' : ''}`}
                  disabled={busy}
                  onClick={() => {
                    setOrder((current) => (current?.id === item.id ? current : item))
                    void operate(() => window.api.payments.order(item.id))
                  }}
                >
                  <span>
                    #{item.id}
                    <small>{item.createdAt ? new Date(item.createdAt).toLocaleString() : ''}</small>
                  </span>
                  <span>
                    {money(item.payAmount)}
                    <small>{statusNames[item.status] || item.status}</small>
                  </span>
                </button>
              ))}
            </div>
          )}
          <button
            className="link-btn recharge-web"
            disabled={busy}
            onClick={() => void openPage(`${state.relayBaseUrl.replace(/\/$/, '')}/purchase`)}
          >
            打开网页充值 <ExternalIcon size={12} />
          </button>
        </div>
      )}
    </Modal>
  )
}
