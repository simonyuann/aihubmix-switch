// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { RechargeModal } from '../src/renderer/src/components/RechargeModal'
import { previewState } from '../src/renderer/src/preview'
import type { PaymentsApi, RechargeOrder, RechargeOverview, RendererApi } from '../src/shared/types'
vi.mock('qrcode', () => ({
  default: { toDataURL: vi.fn(async () => 'data:image/png;base64,AA==') }
}))
let api: PaymentsApi
let overview: RechargeOverview
let pending: RechargeOrder
const onPaid = vi.fn()
const onLogin = vi.fn()
beforeEach(() => {
  pending = {
    id: 123,
    amount: 12.34,
    payAmount: 12.34,
    bonusAmount: 0,
    currency: 'CNY',
    method: 'wxpay',
    status: 'PENDING',
    createdAt: new Date().toISOString(),
    expiresAt: new Date(Date.now() + 900000).toISOString(),
    tradeNo: 'mock-order-123',
    qrCode: 'weixin://mock-order'
  }
  overview = {
    enabled: true,
    email: 'simon@example.com',
    balance: 3.21,
    currency: 'CNY',
    minAmount: 1,
    maxAmount: 500,
    methods: [{ id: 'wxpay', name: '微信支付', minAmount: 1, maxAmount: 500, feeRate: 0 }],
    orders: []
  }
  api = {
    overview: vi.fn(async () => structuredClone(overview)),
    create: vi.fn(async () => structuredClone(pending)),
    order: vi.fn(async () => structuredClone(pending)),
    verify: vi.fn(async () => ({ ...pending, status: 'COMPLETED' })),
    cancel: vi.fn(async () => ({ ...pending, status: 'CANCELLED' }))
  }
  window.api = { payments: api, openExternal: vi.fn() } as unknown as RendererApi
  onPaid.mockReset()
  onLogin.mockReset()
})
afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})
async function ready(loggedIn = true) {
  render(
    <RechargeModal
      open
      state={{ ...previewState, balance: 999, account: { ...previewState.account, loggedIn } }}
      onClose={() => {}}
      onLogin={onLogin}
      onPaid={onPaid}
    />
  )
  if (loggedIn) await screen.findByText('¥3.21')
}
it('requires account login even when a local API key is present', async () => {
  await ready(false)
  expect(api.overview).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '登录后充值' }))
  expect(onLogin).toHaveBeenCalledOnce()
  expect(api.create).not.toHaveBeenCalled()
})
it('loads the real wallet and methods without creating or verifying any order', async () => {
  await ready()
  expect(screen.queryByText('¥999.00')).toBeNull()
  expect(screen.getByRole('option', { name: '微信支付' })).toBeTruthy()
  expect(api.create).not.toHaveBeenCalled()
  expect(api.verify).not.toHaveBeenCalled()
})
it('keeps an unavailable wallet unknown instead of displaying zero', async () => {
  overview.balance = null
  render(
    <RechargeModal open state={previewState} onClose={() => {}} onLogin={onLogin} onPaid={onPaid} />
  )
  await screen.findByRole('button', { name: '生成付款码' })
  expect(screen.getByText('—')).toBeTruthy()
  expect(screen.queryByText('¥0.00')).toBeNull()
})
it('rejects fractional cents and only creates once after explicit confirmation', async () => {
  await ready()
  const input = screen.getByLabelText('充值金额（元）')
  fireEvent.change(input, { target: { value: '12.345' } })
  expect((screen.getByRole('button', { name: '生成付款码' }) as HTMLButtonElement).disabled).toBe(
    true
  )
  fireEvent.change(input, { target: { value: '12.34' } })
  let finish!: (order: RechargeOrder) => void
  vi.mocked(api.create).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const button = screen.getByRole('button', { name: '生成付款码' })
  fireEvent.click(button)
  fireEvent.click(button)
  expect(api.create).toHaveBeenCalledTimes(1)
  expect(api.create).toHaveBeenCalledWith({ amount: '12.34', method: 'wxpay' })
  await act(async () => finish(pending))
  await screen.findByAltText('当前订单付款二维码')
  expect(onPaid).not.toHaveBeenCalled()
  expect(screen.getByText('¥3.21')).toBeTruthy()
})
it('does not automatically retry an ambiguous order result', async () => {
  await ready()
  vi.mocked(api.create).mockRejectedValue(
    new Error('读取充值结果失败，请先刷新充值记录，勿重复下单')
  )
  fireEvent.click(screen.getByRole('button', { name: '生成付款码' }))
  await screen.findByText('下单结果尚未确认，请先刷新充值记录，避免重复下单。')
  expect((screen.getByRole('button', { name: '生成付款码' }) as HTMLButtonElement).disabled).toBe(
    true
  )
  overview.orders = [pending]
  fireEvent.click(screen.getByRole('button', { name: '刷新余额与充值记录' }))
  await screen.findByText('#123')
  expect((screen.getByRole('button', { name: '生成付款码' }) as HTMLButtonElement).disabled).toBe(
    true
  )
  expect(api.create).toHaveBeenCalledTimes(1)
})
it('shows completion only after the server confirms and refreshes the wallet without guessing credit', async () => {
  await ready()
  fireEvent.click(screen.getByRole('button', { name: '生成付款码' }))
  await screen.findByAltText('当前订单付款二维码')
  const completed = { ...pending, status: 'COMPLETED' }
  vi.mocked(api.verify).mockResolvedValue(completed)
  overview.balance = 15.55
  overview.orders = [completed]
  fireEvent.click(screen.getByRole('button', { name: '查询付款结果' }))
  await screen.findByText('¥15.55')
  expect(within(screen.getByLabelText('当前充值订单')).getByRole('status').textContent).toBe(
    '充值成功'
  )
  expect(screen.queryByAltText('当前订单付款二维码')).toBeNull()
  expect(onPaid).toHaveBeenCalledTimes(1)
  expect(api.verify).toHaveBeenCalledWith(123)
})
it('can inspect and cancel an existing pending order without generating another one', async () => {
  overview.orders = [pending]
  await ready()
  expect(api.cancel).not.toHaveBeenCalled()
  fireEvent.click(screen.getByText('#123'))
  await screen.findByRole('button', { name: '取消订单' })
  fireEvent.click(screen.getByRole('button', { name: '取消订单' }))
  await waitFor(() =>
    expect(within(screen.getByLabelText('当前充值订单')).getByRole('status').textContent).toBe(
      '已取消'
    )
  )
  expect(api.cancel).toHaveBeenCalledWith(123)
  expect(api.create).not.toHaveBeenCalled()
  expect(onPaid).not.toHaveBeenCalled()
})
it('ignores a stale pending poll after an explicit successful verification', async () => {
  await ready()
  let finish!: (order: RechargeOrder) => void
  vi.mocked(api.order).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  fireEvent.click(screen.getByRole('button', { name: '生成付款码' }))
  await screen.findByAltText('当前订单付款二维码')
  await waitFor(() => expect(api.order).toHaveBeenCalled())
  fireEvent.click(screen.getByRole('button', { name: '查询付款结果' }))
  await waitFor(() => expect(onPaid).toHaveBeenCalledOnce())
  await act(async () => finish(pending))
  expect(within(screen.getByLabelText('当前充值订单')).getByRole('status').textContent).toBe(
    '充值成功'
  )
})
it('does not offer a payment action when the server disables recharge', async () => {
  overview.enabled = false
  await ready()
  expect(screen.queryByRole('button', { name: '生成付款码' })).toBeNull()
  expect(screen.queryByLabelText('充值金额（元）')).toBeNull()
})
