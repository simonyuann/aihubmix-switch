import type { AccountState, AccountAuthInput, ApiKeyInfo, GroupInfo } from '../shared/types'
import { apiV1, httpJson } from './relay'
import { keyPreview } from '../shared/url'
import type { Store } from './store'

interface AuthPayload {
  access_token?: string
  refresh_token?: string
  expires_in?: number
  requires_2fa?: boolean
  user?: {
    id?: number
    email?: string
  }
}

function unwrap(result: { ok: boolean; status: number; data: unknown; text: string }): unknown {
  const data = result.data
  if (data && typeof data === 'object' && 'code' in data) {
    const record = data as Record<string, unknown>
    if (record.code !== 0) {
      throw new Error(String(record.message || '请求失败'))
    }
    return record.data
  }
  if (!result.ok) {
    const record = (data || {}) as Record<string, unknown>
    const nested = record.error as Record<string, unknown> | undefined
    throw new Error(
      String(record.message || nested?.message || `请求失败 (HTTP ${result.status})`)
    )
  }
  return data
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {}
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

export class AccountService {
  constructor(private readonly store: Store) {}

  get baseUrl(): string {
    return this.store.relayBaseUrl
  }

  state(): AccountState {
    const account = this.store.account
    if (!account) return { loggedIn: false }
    if (!this.store.accessToken && !this.store.refreshToken) return { loggedIn: false }
    return {
      loggedIn: true,
      email: account.email,
      userId: account.userId
    }
  }

  private async request(
    path: string,
    init: RequestInit = {},
    withAuth = true,
    retry = true
  ): Promise<unknown> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...((init.headers as Record<string, string> | undefined) || {})
    }
    if (withAuth) {
      const token = await this.ensureAccessToken()
      headers.Authorization = `Bearer ${token}`
    }

    const result = await httpJson(`${apiV1(this.baseUrl)}${path}`, { ...init, headers })
    if (withAuth && result.status === 401 && retry) {
      await this.refresh()
      return this.request(path, init, withAuth, false)
    }
    return unwrap(result)
  }

  private async ensureAccessToken(): Promise<string> {
    const account = this.store.account
    if (!account) throw new Error('请先登录账号')
    const expiresAt = account.expiresAt || 0
    if (expiresAt && Date.now() > expiresAt - 60_000) {
      await this.refresh()
    }
    const token = this.store.accessToken
    if (!token) throw new Error('登录已过期，请重新登录')
    return token
  }

  async refresh(): Promise<void> {
    const refreshToken = this.store.refreshToken
    if (!refreshToken) {
      this.store.setAccount(null)
      throw new Error('登录已过期，请重新登录')
    }
    try {
      const data = asRecord(
        unwrap(
          await httpJson(`${apiV1(this.baseUrl)}/auth/refresh`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ refresh_token: refreshToken })
          })
        )
      )
      const accessToken = asString(data.access_token)
      const nextRefresh = asString(data.refresh_token, refreshToken)
      if (!accessToken) throw new Error('刷新登录状态失败')
      const expiresAt = Date.now() + asNumber(data.expires_in, 3600) * 1000
      this.store.setTokens(accessToken, nextRefresh, expiresAt)
    } catch (error) {
      this.store.setAccount(null)
      throw new Error(error instanceof Error ? error.message : '登录已过期，请重新登录')
    }
  }

  private saveAuth(payload: AuthPayload): AccountState {
    if (payload.requires_2fa) {
      throw new Error('该账号启用了两步验证，请先在网页端登录后再申请 API Key')
    }
    const accessToken = asString(payload.access_token)
    const refreshToken = asString(payload.refresh_token)
    if (!accessToken) throw new Error('登录失败：服务没有返回访问令牌')
    const expiresAt = Date.now() + asNumber(payload.expires_in, 3600) * 1000
    const email = asString(payload.user?.email)
    const userId =
      typeof payload.user?.id === 'number' && payload.user.id > 0 ? payload.user.id : undefined
    this.store.setAccountSession(email, userId, accessToken, refreshToken, expiresAt)
    return this.state()
  }

  async register(input: AccountAuthInput): Promise<AccountState> {
    const payload = asRecord(
      await this.request(
        '/auth/register',
        {
          method: 'POST',
          body: JSON.stringify({
            email: input.email.trim(),
            password: input.password,
            invitation_code: input.invitationCode?.trim() || undefined
          })
        },
        false
      )
    ) as AuthPayload
    return this.saveAuth(payload)
  }

  async login(input: AccountAuthInput): Promise<AccountState> {
    const payload = asRecord(
      await this.request(
        '/auth/login',
        {
          method: 'POST',
          body: JSON.stringify({
            email: input.email.trim(),
            password: input.password
          })
        },
        false
      )
    ) as AuthPayload
    return this.saveAuth(payload)
  }

  async logout(): Promise<AccountState> {
    try {
      await this.request('/auth/logout', { method: 'POST' }, true, false)
    } catch {
      // 本地退出优先，忽略服务端错误
    }
    this.store.setAccount(null)
    return this.state()
  }

  async groups(): Promise<GroupInfo[]> {
    const data = await this.request('/groups/available')
    const list = Array.isArray(data) ? data : []
    return list
      .map((item) => {
        const record = asRecord(item)
        return {
          id: asNumber(record.id),
          name: asString(record.name, `分组 ${asNumber(record.id)}`),
          description: asString(record.description),
          platform: asString(record.platform),
          rateMultiplier: asNumber(record.rate_multiplier, 1),
          allowMessagesDispatch: Boolean(record.allow_messages_dispatch),
          subscriptionType: asString(record.subscription_type, 'standard')
        }
      })
      .filter((group) => group.id > 0)
  }

  async keys(): Promise<ApiKeyInfo[]> {
    const data = await this.request('/keys?page=1&page_size=100')
    const record = asRecord(data)
    const list = Array.isArray(data)
      ? data
      : Array.isArray(record.items)
        ? record.items
        : Array.isArray(record.data)
          ? record.data
          : []
    return list.map((item) => {
      const key = asRecord(item)
      const group = asRecord(key.group)
      return {
        id: asNumber(key.id),
        name: asString(key.name, '未命名 Key'),
        keyMasked: keyPreview(asString(key.key)),
        groupId: typeof key.group_id === 'number' ? key.group_id : null,
        groupName: asString(group.name, ''),
        status: asString(key.status, 'active'),
        quotaUsed: asNumber(key.quota_used),
        quota: asNumber(key.quota),
        createdAt: asString(key.created_at)
      }
    })
  }

  async createKey(groupId: number, name?: string): Promise<{ key: string; info: ApiKeyInfo }> {
    const data = asRecord(
      await this.request('/keys', {
        method: 'POST',
        body: JSON.stringify({
          name: name?.trim() || 'AIHubMix Switch',
          group_id: groupId
        })
      })
    )
    const key = asString(data.key)
    if (!key) throw new Error('创建成功，但服务没有返回 Key')
    const group = asRecord(data.group)
    return {
      key,
      info: {
        id: asNumber(data.id),
        name: asString(data.name, 'AIHubMix Switch'),
        keyMasked: keyPreview(key),
        groupId: typeof data.group_id === 'number' ? data.group_id : groupId,
        groupName: asString(group.name),
        status: asString(data.status, 'active'),
        quotaUsed: asNumber(data.quota_used),
        quota: asNumber(data.quota),
        createdAt: asString(data.created_at)
      }
    }
  }

  async keyById(id: number): Promise<string> {
    const data = asRecord(await this.request(`/keys/${id}`))
    const key = asString(data.key)
    if (!key) throw new Error('没有找到这个 Key')
    return key
  }
}
