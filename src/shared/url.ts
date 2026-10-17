import { DEFAULT_BASE_URL } from './constants'

export function normalizeBaseUrl(raw: string): string {
  let value = (raw || '').trim()
  if (!value) return DEFAULT_BASE_URL
  if (!/^https?:\/\//i.test(value)) value = `https://${value}`

  let url: URL
  try {
    url = new URL(value)
  } catch {
    throw new Error('服务地址格式不正确')
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') {
    throw new Error('服务地址必须以 http:// 或 https:// 开头')
  }
  url.search = ''
  url.hash = ''
  let path = url.pathname.replace(/\/+$/, '')
  path = path.replace(/\/v1$/i, '')
  url.pathname = path || '/'
  return url.toString().replace(/\/$/, '')
}

export function openaiBaseUrl(baseUrl: string): string {
  return `${normalizeBaseUrl(baseUrl)}/v1`
}

export function anthropicBaseUrl(baseUrl: string): string {
  return normalizeBaseUrl(baseUrl)
}

export function keyPreview(key: string): string {
  if (!key) return ''
  if (key.length <= 10) return '••••••••'
  return `${key.slice(0, 3)}••••${key.slice(-4)}`
}

const NON_CHAT = /(image|audio|realtime|tts|whisper|embedding|moderation|transcribe|speech|auto-review)/i

export function isSelectableModel(id: string): boolean {
  return !NON_CHAT.test(id)
}

function rank(id: string): number {
  const value = id.toLowerCase()
  if (NON_CHAT.test(value)) return 900
  if (value.startsWith('claude-opus')) return 10
  if (value.startsWith('claude-sonnet')) return 11
  if (value.startsWith('claude-haiku')) return 12
  if (value.startsWith('claude')) return 13
  if (value.startsWith('gpt-6.1')) return 20
  if (value.startsWith('gpt-6')) return 21
  if (value.startsWith('gpt-5.6')) return 22
  if (value.startsWith('gpt-5.5')) return 23
  if (value.startsWith('gpt-5.4')) return 24
  if (value.startsWith('gpt-5.3')) return 25
  if (value.startsWith('gpt-5.2')) return 26
  if (value.startsWith('gpt-5.1')) return 27
  if (value.startsWith('gpt-5')) return 28
  if (value.includes('codex')) return 30
  if (value.startsWith('deepseek')) return 40
  if (value.includes('glm')) return 50
  if (value.includes('qwen')) return 60
  if (value.includes('gemini')) return 70
  if (value.includes('grok')) return 80
  return 100
}

export function sortModels<T extends { id: string }>(models: T[]): T[] {
  return [...models].sort((a, b) => {
    const byRank = rank(a.id) - rank(b.id)
    return byRank !== 0 ? byRank : a.id.localeCompare(b.id)
  })
}
