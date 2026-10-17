import { describe, expect, it } from 'vitest'
import { AccountService } from '../src/main/account'
import type { Store } from '../src/main/store'

const BASE = process.env.TEST_BASE_URL || 'https://csmy.fun:23457'
const EMAIL = process.env.TEST_ACCOUNT_EMAIL || ''
const PASSWORD = process.env.TEST_ACCOUNT_PASSWORD || ''

class FakeStore {
  relayBaseUrl = BASE
  private access = ''
  private refresh = ''
  private session: { email: string; userId?: number; expiresAt: number } | null = null

  get account() {
    return this.session
  }
  get accessToken() {
    return this.access
  }
  get refreshToken() {
    return this.refresh
  }
  setAccountSession(email: string, userId: number | undefined, access: string, refresh: string, expiresAt: number) {
    this.session = { email, userId, expiresAt }
    this.access = access
    this.refresh = refresh
  }
  setTokens(access: string, refresh: string, expiresAt: number) {
    this.access = access
    this.refresh = refresh
    if (this.session) this.session.expiresAt = expiresAt
  }
  setAccount(session: typeof this.session) {
    this.session = session
    if (!session) {
      this.access = ''
      this.refresh = ''
    }
  }
}

describe('live account flow', () => {
  it.skipIf(!EMAIL || !PASSWORD)('logs in, lists groups/keys and creates a key', async () => {
    const store = new FakeStore()
    const service = new AccountService(store as unknown as Store)

    const state = await service.login({ email: EMAIL, password: PASSWORD })
    expect(state.loggedIn).toBe(true)

    const groups = await service.groups()
    expect(groups.length).toBeGreaterThan(0)
    const group = groups.find((item) => item.platform === 'openai') || groups[0]!

    const created = await service.createKey(group.id, 'AIHubMix Switch Live Test')
    expect(created.key).toMatch(/^sk-/)
    expect(created.info.id).toBeGreaterThan(0)

    try {
      const keys = await service.keys()
      expect(keys.some((item) => item.id === created.info.id)).toBe(true)
      expect(await service.keyById(created.info.id)).toBe(created.key)
    } finally {
      await fetch(`${BASE}/api/v1/keys/${created.info.id}`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${store.accessToken}` }
      })
    }
  })
})
