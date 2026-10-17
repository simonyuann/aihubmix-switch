import { app, ipcMain, shell } from 'electron'
import path from 'node:path'
import type {
  AccountAuthInput,
  AgentId,
  AgentOptions,
  AgentState,
  AppState,
  ApplyAgentInput,
  ApplyAgentResult,
  ApplyAllResult,
  CreateKeyInput,
  CreatedKey,
  IpcResponse,
  RelayModel,
  RelayProbe,
  RestoreResult,
  SaveKeyInput,
  TestRelayInput
} from '../shared/types'
import { AGENT_ORDER, CODEX_EFFORTS } from '../shared/constants'
import { isSelectableModel, keyPreview, normalizeBaseUrl } from '../shared/url'
import { Store } from './store'
import { AccountService } from './account'
import { fetchSiteInfo, probeRelay, errorMessage } from './relay'
import { adapters, agentConfigPath, detectAgents } from './agents'
import type { AgentContext } from './agents/types'
import { ensureDir } from './agents/fs-utils'

let store: Store
let account: AccountService

let cachedModels: RelayModel[] = []
let cachedBalance: number | null = null
let cachedRateMultiplier: number | null = null
let cachedMessages: AppState['messages'] = 'unknown'
let cachedError: string | null = null
let agentCache: { at: number; states: AgentState[] } | null = null

function context(): AgentContext {
  return {
    home: app.getPath('home'),
    userDataDir: app.getPath('userData'),
    env: process.env,
    platform: process.platform,
    baseUrl: store.relayBaseUrl,
    apiKey: store.apiKey,
    options: store.options
  }
}

function isAgent(value: unknown): value is AgentId {
  return typeof value === 'string' && (AGENT_ORDER as string[]).includes(value)
}

function chooseDefaultModel(agent: AgentId, models: RelayModel[]): string {
  const selectable = models.filter((model) => isSelectableModel(model.id))
  if (!selectable.length) return models[0]?.id || ''
  if (agent === 'claude' && cachedMessages === 'supported') {
    const claude = selectable.find((model) => model.id.toLowerCase().startsWith('claude'))
    if (claude) return claude.id
  }
  return selectable[0]?.id || ''
}

function agentModels(models: RelayModel[]): Record<AgentId, string> {
  const stored = store.agentModels
  const result = {} as Record<AgentId, string>
  for (const agent of AGENT_ORDER) {
    const saved = stored[agent]
    result[agent] =
      saved && models.some((model) => model.id === saved) ? saved : chooseDefaultModel(agent, models)
  }
  return result
}

async function detectCached(force = false): Promise<AgentState[]> {
  if (!force && agentCache && Date.now() - agentCache.at < 20_000) {
    return agentCache.states
  }
  const states = await detectAgents(context())
  agentCache = { at: Date.now(), states }
  return states
}

async function buildState(forceAgents = false): Promise<AppState> {
  const agents = await detectCached(forceAgents)
  return {
    version: app.getVersion(),
    secureStorage: store.secureStorageAvailable,
    site: store.site,
    relayBaseUrl: store.relayBaseUrl,
    hasApiKey: Boolean(store.apiKey),
    keyPreview: keyPreview(store.apiKey),
    account: account.state(),
    models: cachedModels,
    balance: cachedBalance,
    rateMultiplier: cachedRateMultiplier,
    messages: cachedMessages,
    relayError: cachedError,
    agents,
    agentModels: agentModels(cachedModels),
    options: store.options
  }
}

async function refreshRelay(): Promise<RelayProbe | null> {
  const baseUrl = store.relayBaseUrl
  const site = await fetchSiteInfo(baseUrl)
  store.setSite(site)

  const apiKey = store.apiKey
  if (!apiKey) {
    cachedModels = []
    cachedBalance = null
    cachedRateMultiplier = null
    cachedMessages = 'unknown'
    cachedError = null
    return null
  }

  const probe = await probeRelay(baseUrl, apiKey)
  if (probe.ok) {
    cachedModels = probe.models
    cachedBalance = probe.balance
    cachedRateMultiplier = probe.rateMultiplier
    cachedMessages = probe.messages
    cachedError = null
  } else {
    cachedModels = []
    cachedBalance = null
    cachedRateMultiplier = null
    cachedMessages = 'unknown'
    cachedError = probe.error || '连接失败'
  }
  return probe
}

async function saveKeyInternal(input: SaveKeyInput): Promise<{ probe: RelayProbe; state: AppState }> {
  const apiKey = (input.apiKey || '').trim()
  if (!apiKey) throw new Error('请填写 API Key')
  const probe = await probeRelay(input.baseUrl, apiKey)
  if (!probe.ok) throw new Error(probe.error || 'API Key 验证失败')

  const nextBase = normalizeBaseUrl(input.baseUrl)
  if (normalizeBaseUrl(store.relayBaseUrl) !== nextBase) {
    store.setAccount(null)
  }
  store.relayBaseUrl = nextBase
  store.setApiKey(apiKey)
  agentCache = null

  cachedModels = probe.models
  cachedBalance = probe.balance
  cachedRateMultiplier = probe.rateMultiplier
  cachedMessages = probe.messages
  cachedError = null

  const site = await fetchSiteInfo(nextBase)
  store.setSite(site)

  return { probe, state: await buildState() }
}

async function ensureModels(): Promise<void> {
  if (!cachedModels.length && store.apiKey) {
    await refreshRelay()
  }
}

function applyOne(input: ApplyAgentInput): ApplyAgentResult {
  if (!isAgent(input.agent)) throw new Error('未知的 Agent')
  const model = (input.model || '').trim()
  if (!model) throw new Error('请选择模型')
  if (!store.apiKey) throw new Error('请先保存 API Key')
  if (input.agent === 'claude' && cachedMessages === 'unsupported') {
    throw new Error(
      '当前 Key 所属分组未开启 /v1/messages 调度，Claude Code 无法使用。请在平台后台开启 allow_messages_dispatch，或使用 Anthropic 分组。'
    )
  }

  const ctx = context()
  const result = adapters[input.agent].apply(ctx, { model, models: cachedModels })
  store.setAgentModel(input.agent, model)
  store.setApplied(input.agent, {
    model,
    appliedAt: new Date().toISOString(),
    backupId: result.backupId || ''
  })
  agentCache = null
  return {
    agent: input.agent,
    configPath: result.configPath,
    backupId: result.backupId,
    message: result.message
  }
}

function validateOptions(patch: Partial<AgentOptions>): Partial<AgentOptions> {
  const result: Partial<AgentOptions> = {}
  if (patch.piApi !== undefined) {
    if (patch.piApi !== 'openai-responses' && patch.piApi !== 'openai-completions') {
      throw new Error('Pi API 格式不正确')
    }
    result.piApi = patch.piApi
  }
  if (patch.codexWireApi !== undefined) {
    if (patch.codexWireApi !== 'responses' && patch.codexWireApi !== 'chat') {
      throw new Error('Codex 接口格式不正确')
    }
    result.codexWireApi = patch.codexWireApi
  }
  if (patch.codexReasoningEffort !== undefined) {
    const effort = String(patch.codexReasoningEffort)
    if (effort !== 'auto' && !(CODEX_EFFORTS as readonly string[]).includes(effort)) {
      throw new Error('Codex 思考强度不正确')
    }
    result.codexReasoningEffort = effort
  }
  return result
}

function register<T>(channel: string, handler: (...args: any[]) => Promise<T> | T): void {
  ipcMain.handle(channel, async (_event, ...args): Promise<IpcResponse<T>> => {
    try {
      return { ok: true, data: await handler(...args) }
    } catch (error) {
      return { ok: false, error: errorMessage(error) }
    }
  })
}

export function registerIpc(): void {
  store = new Store()
  account = new AccountService(store)

  register('app:get-state', () => buildState())

  register('app:refresh', async (forceAgents?: boolean) => {
    await refreshRelay()
    return buildState(Boolean(forceAgents))
  })

  register('relay:test', (input: TestRelayInput) => {
    if (!input || typeof input.apiKey !== 'string') throw new Error('参数不正确')
    return probeRelay(input.baseUrl, input.apiKey)
  })

  register('key:save', (input: SaveKeyInput) => saveKeyInternal(input))

  register('key:clear', async () => {
    store.clearApiKey()
    agentCache = null
    cachedModels = []
    cachedBalance = null
    cachedRateMultiplier = null
    cachedMessages = 'unknown'
    cachedError = null
    return buildState()
  })

  register('agent:apply', async (input: ApplyAgentInput) => {
    await ensureModels()
    return applyOne(input)
  })

  register('agent:apply-all', async (): Promise<ApplyAllResult> => {
    await ensureModels()
    const results: ApplyAgentResult[] = []
    const errors: Array<{ agent: AgentId; message: string }> = []
    for (const agent of AGENT_ORDER) {
      const model = agentModels(cachedModels)[agent]
      if (!model) {
        errors.push({ agent, message: '没有可用模型' })
        continue
      }
      try {
        results.push(applyOne({ agent, model }))
      } catch (error) {
        errors.push({ agent, message: errorMessage(error) })
      }
    }
    return { results, errors }
  })

  register('agent:restore', async (agent: AgentId): Promise<RestoreResult> => {
    if (!isAgent(agent)) throw new Error('未知的 Agent')
    const message = adapters[agent].restore(context())
    store.clearApplied(agent)
    agentCache = null
    return { agent, message }
  })

  register('agent:set-model', async (agent: AgentId, model: string) => {
    if (!isAgent(agent)) throw new Error('未知的 Agent')
    if (typeof model !== 'string' || !model.trim()) throw new Error('模型不能为空')
    store.setAgentModel(agent, model.trim())
    return buildState()
  })

  register('options:update', async (patch: Partial<AgentOptions>) => {
    store.setOptions(validateOptions(patch || {}))
    return buildState()
  })

  register('shell:open-external', async (url: string) => {
    if (typeof url !== 'string') throw new Error('链接不正确')
    let parsed: URL
    try {
      parsed = new URL(url)
    } catch {
      throw new Error('链接不正确')
    }
    if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
      throw new Error('只允许打开 http/https 链接')
    }
    await shell.openExternal(parsed.toString())
  })

  register('shell:open-path', async (target: string) => {
    const ctx = context()
    let dir: string
    if (target === 'userData') {
      dir = app.getPath('userData')
    } else if (target === 'backups') {
      dir = path.join(app.getPath('userData'), 'backups')
    } else if (isAgent(target)) {
      dir = path.dirname(agentConfigPath(target, ctx))
    } else {
      throw new Error('未知目录')
    }
    ensureDir(dir)
    const error = await shell.openPath(dir)
    if (error) throw new Error(error)
  })

  register('account:register', (input: AccountAuthInput) => {
    if (!input?.email || !input?.password) throw new Error('请填写邮箱和密码')
    return account.register(input)
  })

  register('account:login', (input: AccountAuthInput) => {
    if (!input?.email || !input?.password) throw new Error('请填写邮箱和密码')
    return account.login(input)
  })

  register('account:logout', () => account.logout())

  register('account:groups', () => account.groups())

  register('account:keys', () => account.keys())

  register('account:create-key', async (input: CreateKeyInput): Promise<CreatedKey> => {
    const groupId = Number(input?.groupId)
    if (!Number.isFinite(groupId) || groupId <= 0) throw new Error('请选择分组')
    const created = await account.createKey(groupId, input.name)
    store.setApiKey(created.key)
    agentCache = null
    await refreshRelay()
    return { key: created.key, info: created.info, state: await buildState() }
  })

  register('account:use-key', async (id: number) => {
    const keyId = Number(id)
    if (!Number.isFinite(keyId) || keyId <= 0) throw new Error('Key 不正确')
    const key = await account.keyById(keyId)
    const probe = await probeRelay(store.relayBaseUrl, key)
    if (!probe.ok) throw new Error(probe.error || '这个 Key 不可用')
    store.setApiKey(key)
    agentCache = null
    cachedModels = probe.models
    cachedBalance = probe.balance
    cachedRateMultiplier = probe.rateMultiplier
    cachedMessages = probe.messages
    cachedError = null
    return buildState()
  })
}

