import type { MessageCapability, RelayModel, RelayProbe, SiteInfo } from '../shared/types'
import { DEFAULT_DOC_URL } from '../shared/constants'
import { normalizeBaseUrl, openaiBaseUrl, sortModels, isSelectableModel } from '../shared/url'

export interface HttpResult {
  status: number
  ok: boolean
  data: unknown
  text: string
}

const TIMEOUT_MS = 20000

export async function httpJson(
  url: string,
  init: RequestInit = {},
  timeoutMs = TIMEOUT_MS
): Promise<HttpResult> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetch(url, {
      ...init,
      signal: controller.signal,
      headers: {
        Accept: 'application/json',
        ...(init.headers || {})
      }
    })
    const text = await response.text()
    let data: unknown = null
    if (text) {
      try {
        data = JSON.parse(text)
      } catch {
        data = null
      }
    }
    return { status: response.status, ok: response.ok, data, text }
  } finally {
    clearTimeout(timer)
  }
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    if (error.name === 'AbortError') return '连接超时，请检查网络或服务地址'
    if (/ENOTFOUND|EAI_AGAIN/.test(error.message)) return '域名解析失败，请检查服务地址'
    if (/ECONNREFUSED/.test(error.message)) return '连接被拒绝，请确认中转站是否正常运行'
    if (/CERT|SSL|TLS/i.test(error.message)) return 'HTTPS 证书校验失败'
    return error.message
  }
  return String(error)
}

function asNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

export function parseModels(payload: unknown): RelayModel[] {
  const root = payload as Record<string, unknown> | null
  const raw =
    (Array.isArray(root?.data) && root?.data) ||
    (Array.isArray(root?.models) && root?.models) ||
    (Array.isArray(payload) && payload) ||
    []

  const seen = new Set<string>()
  const models: RelayModel[] = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const record = item as Record<string, unknown>
    const id = String(record.id ?? record.slug ?? record.name ?? '').trim()
    if (!id || seen.has(id)) continue
    seen.add(id)
    const display = record.display_name ?? record.displayName ?? record.name
    models.push({
      id,
      name: typeof display === 'string' && display.trim() ? display.trim() : id,
      ownedBy: typeof record.owned_by === 'string' ? record.owned_by : undefined
    })
  }
  return sortModels(models)
}

export function mergeManifest(models: RelayModel[], payload: unknown): RelayModel[] {
  const root = payload as Record<string, unknown> | null
  const raw =
    (Array.isArray(root?.models) && root?.models) ||
    (Array.isArray(root?.data) && root?.data) ||
    []
  const bySlug = new Map<string, Record<string, unknown>>()
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const record = item as Record<string, unknown>
    const slug = String(record.slug ?? record.id ?? '').trim()
    if (slug) bySlug.set(slug, record)
  }

  return models.map((model) => {
    const record = bySlug.get(model.id)
    if (!record) return model
    const contextWindow =
      typeof record.context_window === 'number' && record.context_window > 0
        ? record.context_window
        : undefined
    const levels = Array.isArray(record.supported_reasoning_levels)
      ? record.supported_reasoning_levels
      : []
    const reasoning = levels.some((level) => {
      const effort = (level as Record<string, unknown> | null)?.effort
      return typeof effort === 'string' && effort !== 'none'
    })
    const input = Array.isArray(record.input_modalities)
      ? record.input_modalities.filter((value): value is string => typeof value === 'string')
      : undefined
    const display = typeof record.display_name === 'string' ? record.display_name.trim() : ''
    return {
      ...model,
      name: display || model.name,
      contextWindow,
      maxTokens: contextWindow ? Math.min(128000, Math.floor(contextWindow / 2)) : undefined,
      reasoning,
      input
    }
  })
}

function pickProbeModel(models: RelayModel[], preferClaude: boolean): string | null {
  const selectable = models.filter((model) => isSelectableModel(model.id))
  if (!selectable.length) return null
  if (preferClaude) {
    const claude = selectable.find((model) => model.id.toLowerCase().startsWith('claude'))
    if (claude) return claude.id
  }
  return selectable[0]?.id ?? null
}

async function probeMessages(
  baseUrl: string,
  apiKey: string,
  models: RelayModel[]
): Promise<MessageCapability> {
  const model = pickProbeModel(models, true)
  if (!model) return 'unknown'
  try {
    const result = await httpJson(
      `${baseUrl}/v1/messages/count_tokens`,
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${apiKey}`,
          'Content-Type': 'application/json',
          'anthropic-version': '2023-06-01'
        },
        body: JSON.stringify({
          model,
          messages: [{ role: 'user', content: 'hi' }]
        })
      },
      12000
    )
    if (result.ok) return 'supported'
    const text = result.text.toLowerCase()
    if (result.status === 403 && text.includes('does not allow')) return 'unsupported'
    if (result.status === 403 && (text.includes('permission') || text.includes('group'))) {
      return 'unsupported'
    }
    if (result.status === 404) return 'unsupported'
    return 'unknown'
  } catch {
    return 'unknown'
  }
}

export async function probeRelay(rawBaseUrl: string, apiKey: string): Promise<RelayProbe> {
  const baseUrl = normalizeBaseUrl(rawBaseUrl)
  const key = apiKey.trim()
  const startedAt = Date.now()

  if (!key) {
    return {
      ok: false,
      baseUrl,
      models: [],
      balance: null,
      rateMultiplier: null,
      messages: 'unknown',
      latencyMs: 0,
      error: '请先填写 API Key'
    }
  }

  const headers = { Authorization: `Bearer ${key}` }
  const modelsPromise = httpJson(`${baseUrl}/v1/models`, { headers })
  const manifestPromise = httpJson(
    `${baseUrl}/v1/models?client_version=0.99.0`,
    { headers },
    12000
  ).catch(() => null)
  const usagePromise = httpJson(`${baseUrl}/v1/usage`, { headers }).catch(() => null)
  const billingPromise = httpJson(`${baseUrl}/v1/sub2api/billing`, { headers }).catch(() => null)

  let modelsResult: HttpResult
  try {
    modelsResult = await modelsPromise
  } catch (error) {
    return {
      ok: false,
      baseUrl,
      models: [],
      balance: null,
      rateMultiplier: null,
      messages: 'unknown',
      latencyMs: Date.now() - startedAt,
      error: errorMessage(error)
    }
  }

  const latencyMs = Date.now() - startedAt
  if (modelsResult.status === 401 || modelsResult.status === 403) {
    return {
      ok: false,
      baseUrl,
      models: [],
      balance: null,
      rateMultiplier: null,
      messages: 'unknown',
      latencyMs,
      error: 'API Key 无效、已禁用，或该 Key 没有访问权限'
    }
  }
  if (!modelsResult.ok) {
    return {
      ok: false,
      baseUrl,
      models: [],
      balance: null,
      rateMultiplier: null,
      messages: 'unknown',
      latencyMs,
      error: `服务返回 ${modelsResult.status}，请确认服务地址是否正确`
    }
  }

  const baseModels = parseModels(modelsResult.data)
  if (!baseModels.length) {
    return {
      ok: false,
      baseUrl,
      models: [],
      balance: null,
      rateMultiplier: null,
      messages: 'unknown',
      latencyMs,
      error: '连接成功，但没有获取到任何模型，请检查 Key 所属分组'
    }
  }

  const [usageResult, billingResult, manifestResult, messages] = await Promise.all([
    usagePromise,
    billingPromise,
    manifestPromise,
    probeMessages(baseUrl, key, baseModels)
  ])
  const models = mergeManifest(baseModels, manifestResult?.data)

  const usage = usageResult?.data as Record<string, unknown> | null
  const quota = usage?.quota as Record<string, unknown> | undefined
  const balance =
    asNumber(usage?.balance) ?? asNumber(usage?.remaining) ?? asNumber(quota?.remaining) ?? null

  const billing = billingResult?.data as Record<string, unknown> | null
  const rateMultiplier =
    asNumber(billing?.group_rate_multiplier) ??
    asNumber(billing?.resolved_rate_multiplier) ??
    asNumber(billing?.effective_rate_multiplier) ??
    null

  return {
    ok: true,
    baseUrl,
    models,
    balance,
    rateMultiplier,
    messages,
    latencyMs
  }
}

export async function fetchSiteInfo(rawBaseUrl: string): Promise<SiteInfo> {
  const baseUrl = normalizeBaseUrl(rawBaseUrl)
  const fallback: SiteInfo = {
    name: 'AIHubMix',
    subtitle: '订阅转 API 的 AI 聚合平台',
    apiBaseUrl: baseUrl,
    docUrl: DEFAULT_DOC_URL,
    registerUrl: `${baseUrl}/register`,
    loginUrl: `${baseUrl}/login`
  }

  try {
    const result = await httpJson(`${baseUrl}/api/v1/settings/public`, {}, 12000)
    if (!result.ok) return fallback
    const data = result.data as Record<string, unknown> | null
    const name = typeof data?.site_name === 'string' ? data.site_name.trim() : ''
    const subtitle = typeof data?.site_subtitle === 'string' ? data.site_subtitle.trim() : ''
    const docUrl = typeof data?.doc_url === 'string' ? data.doc_url.trim() : ''
    const apiBaseUrl =
      typeof data?.api_base_url === 'string' ? data.api_base_url.trim() : ''
    return {
      name: name || fallback.name,
      subtitle: subtitle || fallback.subtitle,
      apiBaseUrl: apiBaseUrl || baseUrl,
      docUrl: docUrl || fallback.docUrl,
      registerUrl: `${baseUrl}/register`,
      loginUrl: `${baseUrl}/login`
    }
  } catch {
    return fallback
  }
}

export function apiV1(rawBaseUrl: string): string {
  return `${normalizeBaseUrl(rawBaseUrl)}/api/v1`
}

export function gatewayBase(rawBaseUrl: string): string {
  return openaiBaseUrl(rawBaseUrl)
}
