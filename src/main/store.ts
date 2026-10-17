import { app, safeStorage } from 'electron'
import fs from 'node:fs'
import path from 'node:path'
import type { AgentId, AgentOptions, SiteInfo } from '../shared/types'
import { DEFAULT_BASE_URL, DEFAULT_DOC_URL } from '../shared/constants'
import { normalizeBaseUrl } from '../shared/url'

export interface AccountSession {
  email: string
  userId?: number
  accessTokenEnc: string
  refreshTokenEnc: string
  expiresAt: number
}

export interface AppliedRecord {
  model: string
  appliedAt: string
  backupId: string
}

export interface PersistedState {
  version: 1
  relayBaseUrl: string
  apiKeyEnc: string
  account: AccountSession | null
  agentModels: Partial<Record<AgentId, string>>
  options: AgentOptions
  applied: Partial<Record<AgentId, AppliedRecord>>
  site: SiteInfo | null
}

const DEFAULT_OPTIONS: AgentOptions = {
  piApi: 'openai-responses',
  codexWireApi: 'responses'
}

function defaultState(): PersistedState {
  return {
    version: 1,
    relayBaseUrl: DEFAULT_BASE_URL,
    apiKeyEnc: '',
    account: null,
    agentModels: {},
    options: { ...DEFAULT_OPTIONS },
    applied: {},
    site: null
  }
}

function encode(value: string): string {
  if (!value) return ''
  if (safeStorage.isEncryptionAvailable()) {
    return `enc:${safeStorage.encryptString(value).toString('base64')}`
  }
  return `raw:${Buffer.from(value, 'utf8').toString('base64')}`
}

function decode(value: string): string {
  if (!value) return ''
  try {
    if (value.startsWith('enc:')) {
      return safeStorage.decryptString(Buffer.from(value.slice(4), 'base64'))
    }
    if (value.startsWith('raw:')) {
      return Buffer.from(value.slice(4), 'base64').toString('utf8')
    }
  } catch {
    return ''
  }
  return ''
}

export class Store {
  private readonly file: string
  private state: PersistedState

  constructor(userDataDir?: string) {
    const dir = userDataDir || app.getPath('userData')
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 })
    this.file = path.join(dir, 'state.json')
    this.state = this.load()
  }

  private load(): PersistedState {
    try {
      const raw = fs.readFileSync(this.file, 'utf8')
      const parsed = JSON.parse(raw) as Partial<PersistedState>
      return {
        ...defaultState(),
        ...parsed,
        options: { ...DEFAULT_OPTIONS, ...(parsed.options || {}) },
        agentModels: { ...(parsed.agentModels || {}) },
        applied: { ...(parsed.applied || {}) }
      }
    } catch {
      return defaultState()
    }
  }

  private persist(): void {
    const dir = path.dirname(this.file)
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 })
    const tmp = `${this.file}.tmp-${process.pid}-${Date.now()}`
    fs.writeFileSync(tmp, JSON.stringify(this.state, null, 2), { mode: 0o600 })
    fs.renameSync(tmp, this.file)
    try {
      fs.chmodSync(this.file, 0o600)
    } catch {
      // Windows 不支持 chmod，忽略
    }
  }

  get relayBaseUrl(): string {
    return this.state.relayBaseUrl || DEFAULT_BASE_URL
  }

  set relayBaseUrl(value: string) {
    this.state.relayBaseUrl = normalizeBaseUrl(value)
    this.persist()
  }

  get apiKey(): string {
    return decode(this.state.apiKeyEnc)
  }

  setApiKey(key: string): void {
    this.state.apiKeyEnc = encode(key)
    this.persist()
  }

  clearApiKey(): void {
    this.state.apiKeyEnc = ''
    this.persist()
  }

  get account(): AccountSession | null {
    return this.state.account
  }

  setAccount(session: AccountSession | null): void {
    this.state.account = session
    this.persist()
  }

  setAccountSession(
    email: string,
    userId: number | undefined,
    accessToken: string,
    refreshToken: string,
    expiresAt: number
  ): void {
    this.state.account = {
      email,
      userId,
      accessTokenEnc: encode(accessToken),
      refreshTokenEnc: encode(refreshToken),
      expiresAt
    }
    this.persist()
  }

  get accessToken(): string {
    return decode(this.state.account?.accessTokenEnc || '')
  }

  get refreshToken(): string {
    return decode(this.state.account?.refreshTokenEnc || '')
  }

  setTokens(accessToken: string, refreshToken: string, expiresAt: number): void {
    if (!this.state.account) return
    this.state.account.accessTokenEnc = encode(accessToken)
    this.state.account.refreshTokenEnc = encode(refreshToken)
    this.state.account.expiresAt = expiresAt
    this.persist()
  }

  get options(): AgentOptions {
    return { ...this.state.options }
  }

  setOptions(patch: Partial<AgentOptions>): AgentOptions {
    this.state.options = { ...this.state.options, ...patch }
    this.persist()
    return this.options
  }

  get agentModels(): Record<AgentId, string> {
    return this.state.agentModels as Record<AgentId, string>
  }

  setAgentModel(agent: AgentId, model: string): void {
    this.state.agentModels[agent] = model
    this.persist()
  }

  get applied(): Partial<Record<AgentId, AppliedRecord>> {
    return { ...this.state.applied }
  }

  setApplied(agent: AgentId, record: AppliedRecord): void {
    this.state.applied[agent] = record
    this.persist()
  }

  clearApplied(agent: AgentId): void {
    delete this.state.applied[agent]
    this.persist()
  }

  get site(): SiteInfo {
    if (this.state.site) return this.state.site
    return {
      name: 'AIHubMix',
      subtitle: '订阅转 API 的 AI 聚合平台',
      apiBaseUrl: this.relayBaseUrl,
      docUrl: DEFAULT_DOC_URL,
      registerUrl: `${this.relayBaseUrl}/register`,
      loginUrl: `${this.relayBaseUrl}/login`
    }
  }

  setSite(site: SiteInfo): void {
    this.state.site = site
    this.persist()
  }

  get secureStorageAvailable(): boolean {
    return safeStorage.isEncryptionAvailable()
  }
}
