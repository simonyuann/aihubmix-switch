import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { parse } from 'smol-toml'
import { parse as parseJsonc } from 'jsonc-parser'
import { claudeAdapter } from '../src/main/agents/claude'
import { codexAdapter } from '../src/main/agents/codex'
import { piAdapter } from '../src/main/agents/pi'
import { opencodeAdapter } from '../src/main/agents/opencode'
import type { AgentContext } from '../src/main/agents/types'
import { normalizeBaseUrl, openaiBaseUrl, sortModels } from '../src/shared/url'
import { parseModels } from '../src/main/relay'

const BASE = 'https://csmy.fun:23457'
const KEY = 'sk-test-bbbc34bb298eb9aad2c547ce'
const MODELS = [
  { id: 'gpt-5.6-luna', name: 'GPT-5.6 Luna' },
  { id: 'gpt-5.6-sol', name: 'GPT-5.6 Sol' },
  { id: 'gpt-image-1', name: 'GPT Image 1' }
]

let home: string
let ctx: AgentContext

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'aihubmix-test-'))
  ctx = {
    home,
    userDataDir: path.join(home, 'app-data'),
    env: {},
    platform: 'linux',
    baseUrl: BASE,
    apiKey: KEY,
    options: { piApi: 'openai-responses', codexWireApi: 'responses' }
  }
})

afterEach(() => {
  fs.rmSync(home, { recursive: true, force: true })
})

function write(file: string, content: string): void {
  fs.mkdirSync(path.dirname(file), { recursive: true })
  fs.writeFileSync(file, content)
}

function read(file: string): string {
  return fs.readFileSync(file, 'utf8')
}

describe('key change detection', () => {
  it('marks all adapters as not connected when the key changes', async () => {
    claudeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    codexAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    piAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    opencodeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })

    const other = { ...ctx, apiKey: 'sk-different-key-000000' }
    expect((await claudeAdapter.status(other)).connected).toBe(false)
    expect((await codexAdapter.status(other)).connected).toBe(false)
    expect((await piAdapter.status(other)).connected).toBe(false)
    expect((await opencodeAdapter.status(other)).connected).toBe(false)
  })
})

describe('url helpers', () => {
  it('normalizes base urls', () => {
    expect(normalizeBaseUrl('csmy.fun:23457/')).toBe(BASE)
    expect(normalizeBaseUrl(`${BASE}/v1/`)).toBe(BASE)
    expect(normalizeBaseUrl(`${BASE}/`)).toBe(BASE)
    expect(openaiBaseUrl(BASE)).toBe(`${BASE}/v1`)
  })

  it('sorts coding models first and keeps image models last', () => {
    const sorted = sortModels([
      { id: 'gpt-image-1' },
      { id: 'deepseek-chat' },
      { id: 'gpt-5.6-luna' },
      { id: 'claude-sonnet-4.5' }
    ])
    expect(sorted.map((model) => model.id)).toEqual([
      'claude-sonnet-4.5',
      'gpt-5.6-luna',
      'deepseek-chat',
      'gpt-image-1'
    ])
  })

  it('parses OpenAI and Codex model payloads', () => {
    expect(parseModels({ data: [{ id: 'gpt-5.6', owned_by: 'openai' }] })).toEqual([
      { id: 'gpt-5.6', name: 'gpt-5.6', ownedBy: 'openai' }
    ])
    expect(parseModels({ models: [{ slug: 'gpt-5.6', display_name: 'GPT-5.6' }] })).toEqual([
      { id: 'gpt-5.6', name: 'GPT-5.6' }
    ])
  })
})

describe('claude adapter', () => {
  it('merges settings, reports connected and restores previous values', async () => {
    const file = path.join(home, '.claude', 'settings.json')
    write(
      file,
      JSON.stringify({
        theme: 'dark',
        env: { KEEP: '1', ANTHROPIC_AUTH_TOKEN: 'old-key' }
      })
    )

    const result = claudeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    expect(result.configPath).toBe(file)

    const saved = JSON.parse(read(file))
    expect(saved.theme).toBe('dark')
    expect(saved.env.KEEP).toBe('1')
    expect(saved.env.ANTHROPIC_BASE_URL).toBe(BASE)
    expect(saved.env.ANTHROPIC_AUTH_TOKEN).toBe(KEY)
    expect(saved.env.ANTHROPIC_API_KEY).toBe(KEY)
    expect(saved.env.ANTHROPIC_MODEL).toBe('gpt-5.6-luna')
    expect(saved.env.ANTHROPIC_DEFAULT_SONNET_MODEL).toBe('gpt-5.6-luna')

    const status = await claudeAdapter.status(ctx)
    expect(status.connected).toBe(true)
    expect(status.appliedModel).toBe('gpt-5.6-luna')

    claudeAdapter.restore(ctx)
    const restored = JSON.parse(read(file))
    expect(restored.theme).toBe('dark')
    expect(restored.env.KEEP).toBe('1')
    expect(restored.env.ANTHROPIC_AUTH_TOKEN).toBe('old-key')
    expect(restored.env.ANTHROPIC_BASE_URL).toBeUndefined()
    expect(restored.env.ANTHROPIC_MODEL).toBeUndefined()
  })

  it('keeps the original restore point after applying twice', () => {
    const file = path.join(home, '.claude', 'settings.json')
    write(file, JSON.stringify({ env: { ANTHROPIC_AUTH_TOKEN: 'old-key' } }))
    claudeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    claudeAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    claudeAdapter.restore(ctx)
    const restored = JSON.parse(read(file))
    expect(restored.env.ANTHROPIC_AUTH_TOKEN).toBe('old-key')
    expect(restored.env.ANTHROPIC_MODEL).toBeUndefined()
  })

  it('reports not connected for another base url', async () => {
    write(
      path.join(home, '.claude', 'settings.json'),
      JSON.stringify({ env: { ANTHROPIC_BASE_URL: 'https://other.example', ANTHROPIC_AUTH_TOKEN: 'x' } })
    )
    const status = await claudeAdapter.status(ctx)
    expect(status.connected).toBe(false)
  })
})

describe('codex adapter', () => {
  it('preserves comments and unrelated sections', async () => {
    const file = path.join(home, '.codex', 'config.toml')
    const auth = path.join(home, '.codex', 'auth.json')
    const original = `# my codex config
model = "old-model"
approval_policy = "on-request"

[projects."/home/simon"]
trust_level = "trusted"
`
    write(file, original)
    write(auth, JSON.stringify({ OPENAI_API_KEY: 'old-key', other: 1 }))

    codexAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })

    const text = read(file)
    expect(text).toContain('# my codex config')
    expect(text).toContain('[projects."/home/simon"]')
    expect(text).toContain('model_provider = "csmy"')
    expect(text).toContain('model = "gpt-5.6-luna"')
    expect(text).toContain('model_reasoning_effort = "medium"')
    expect(text).toContain('[model_providers.csmy]')
    expect(text).toContain(`base_url = "${BASE}/v1"`)

    const doc = parse(text) as Record<string, any>
    expect(doc.model_provider).toBe('csmy')
    expect(doc.model).toBe('gpt-5.6-luna')
    expect(doc.model_providers.csmy.wire_api).toBe('responses')
    expect(doc.projects['/home/simon'].trust_level).toBe('trusted')

    expect(JSON.parse(read(auth))).toEqual({ OPENAI_API_KEY: KEY })

    const status = await codexAdapter.status(ctx)
    expect(status.connected).toBe(true)
    expect(status.appliedModel).toBe('gpt-5.6-luna')

    codexAdapter.restore(ctx)
    expect(read(file)).toBe(original)
    expect(JSON.parse(read(auth))).toEqual({ OPENAI_API_KEY: 'old-key', other: 1 })
  })

  it('keeps the original restore point after applying twice', () => {
    const file = path.join(home, '.codex', 'config.toml')
    const auth = path.join(home, '.codex', 'auth.json')
    const original = 'model = "old-model"\n'
    write(file, original)
    write(auth, JSON.stringify({ OPENAI_API_KEY: 'old-key' }))
    codexAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    codexAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    codexAdapter.restore(ctx)
    expect(read(file)).toBe(original)
    expect(JSON.parse(read(auth))).toEqual({ OPENAI_API_KEY: 'old-key' })
  })

  it('replaces an existing csmy provider section without duplicating it', () => {
    const file = path.join(home, '.codex', 'config.toml')
    write(
      file,
      `model = "a"\n\n[model_providers.csmy]\nname = "old"\nbase_url = "http://old"\n\n[model_providers.csmy.env]\nX = "1"\n\n[other]\nkeep = true\n`
    )
    codexAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    const text = read(file)
    expect(text.match(/\[model_providers\.csmy\]/g)).toHaveLength(1)
    expect(text).not.toContain('name = "old"')
    expect(text).toContain('[other]')
    expect(text).toContain('keep = true')
  })
})

describe('pi adapter', () => {
  it('merges provider and defaults, then restores them', async () => {
    const modelsFile = path.join(home, '.pi', 'agent', 'models.json')
    const settingsFile = path.join(home, '.pi', 'agent', 'settings.json')
    write(
      modelsFile,
      JSON.stringify({ providers: { other: { baseUrl: 'https://x/v1', api: 'openai-completions', models: [] } } })
    )
    write(
      settingsFile,
      JSON.stringify({ theme: 'dark', defaultProvider: 'other', defaultModel: 'x' })
    )

    piAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })

    const models = JSON.parse(read(modelsFile))
    expect(models.providers.other).toBeTruthy()
    expect(models.providers.csmy.baseUrl).toBe(`${BASE}/v1`)
    expect(models.providers.csmy.api).toBe('openai-responses')
    expect(models.providers.csmy.apiKey).toBe(KEY)
    expect(models.providers.csmy.models.some((m: any) => m.id === 'gpt-5.6-luna')).toBe(true)
    expect(models.providers.csmy.models.some((m: any) => m.id === 'gpt-image-1')).toBe(false)

    const settings = JSON.parse(read(settingsFile))
    expect(settings.theme).toBe('dark')
    expect(settings.defaultProvider).toBe('csmy')
    expect(settings.defaultModel).toBe('gpt-5.6-luna')

    const status = await piAdapter.status(ctx)
    expect(status.connected).toBe(true)
    expect(status.appliedModel).toBe('gpt-5.6-luna')

    piAdapter.restore(ctx)
    const restoredModels = JSON.parse(read(modelsFile))
    expect(restoredModels.providers.csmy).toBeUndefined()
    expect(restoredModels.providers.other).toBeTruthy()
    const restoredSettings = JSON.parse(read(settingsFile))
    expect(restoredSettings.theme).toBe('dark')
    expect(restoredSettings.defaultProvider).toBe('other')
    expect(restoredSettings.defaultModel).toBe('x')
  })

  it('keeps the original restore point after applying twice', () => {
    const modelsFile = path.join(home, '.pi', 'agent', 'models.json')
    const settingsFile = path.join(home, '.pi', 'agent', 'settings.json')
    write(modelsFile, JSON.stringify({ providers: { other: { baseUrl: 'https://x/v1' } } }))
    write(settingsFile, JSON.stringify({ defaultProvider: 'other', defaultModel: 'x' }))
    piAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    piAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    piAdapter.restore(ctx)
    const models = JSON.parse(read(modelsFile))
    expect(models.providers.csmy).toBeUndefined()
    expect(models.providers.other).toBeTruthy()
    const settings = JSON.parse(read(settingsFile))
    expect(settings.defaultProvider).toBe('other')
    expect(settings.defaultModel).toBe('x')
  })
})

describe('opencode adapter', () => {
  it('preserves jsonc comments and restores the previous provider/model', async () => {
    const file = path.join(home, '.config', 'opencode', 'opencode.jsonc')
    write(
      file,
      `{
  // keep this comment
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "other": { "npm": "@ai-sdk/openai-compatible", "options": { "baseURL": "https://x/v1" } }
  },
  "model": "other/x"
}
`
    )

    opencodeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })

    const text = read(file)
    expect(text).toContain('// keep this comment')
    const value = parseJsonc(text)
    expect(value.provider.other).toBeTruthy()
    expect(value.provider.csmy.npm).toBe('@ai-sdk/openai-compatible')
    expect(value.provider.csmy.options.baseURL).toBe(`${BASE}/v1`)
    expect(value.provider.csmy.options.apiKey).toBe(KEY)
    expect(value.model).toBe('csmy/gpt-5.6-luna')

    const status = await opencodeAdapter.status(ctx)
    expect(status.connected).toBe(true)
    expect(status.appliedModel).toBe('gpt-5.6-luna')

    opencodeAdapter.restore(ctx)
    const restoredText = read(file)
    expect(restoredText).toContain('// keep this comment')
    const restored = parseJsonc(restoredText)
    expect(restored.provider.csmy).toBeUndefined()
    expect(restored.provider.other).toBeTruthy()
    expect(restored.model).toBe('other/x')
  })

  it('keeps the original restore point after applying twice', () => {
    const file = path.join(home, '.config', 'opencode', 'opencode.json')
    write(
      file,
      JSON.stringify({
        provider: { other: { npm: '@ai-sdk/openai-compatible', options: { baseURL: 'https://x/v1' } } },
        model: 'other/x'
      })
    )
    opencodeAdapter.apply(ctx, { model: 'gpt-5.6-luna', models: MODELS })
    opencodeAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    opencodeAdapter.restore(ctx)
    const value = JSON.parse(read(file))
    expect(value.provider.csmy).toBeUndefined()
    expect(value.provider.other).toBeTruthy()
    expect(value.model).toBe('other/x')
  })

  it('creates a config when none exists', async () => {
    const result = opencodeAdapter.apply(ctx, { model: 'gpt-5.6-sol', models: MODELS })
    const value = JSON.parse(read(result.configPath))
    expect(value.$schema).toBe('https://opencode.ai/config.json')
    expect(value.model).toBe('csmy/gpt-5.6-sol')
  })
})
