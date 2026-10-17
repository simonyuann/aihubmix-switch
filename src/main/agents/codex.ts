import path from 'node:path'
import fs from 'node:fs'
import { parse } from 'smol-toml'
import type { AgentAdapter, AgentContext, ApplyOptions, ApplyResult } from './types'
import { normalizeBaseUrl, openaiBaseUrl } from '../../shared/url'
import { PROVIDER_ID, PROVIDER_NAME } from '../../shared/constants'
import { createBackup, latestBackup, restoreBackup } from './backup'
import {
  ensureDir,
  exists,
  parseJson,
  readText,
  stringifyJson,
  writeTextAtomic
} from './fs-utils'

function configDir(ctx: AgentContext): string {
  const override = ctx.env.CODEX_HOME?.trim()
  return override || path.join(ctx.home, '.codex')
}

function configPath(ctx: AgentContext): string {
  return path.join(configDir(ctx), 'config.toml')
}

function authPath(ctx: AgentContext): string {
  return path.join(configDir(ctx), 'auth.json')
}

function tomlString(value: string): string {
  return JSON.stringify(value)
}

function headerKey(line: string): string | null {
  const trimmed = line.trim()
  if (!trimmed.startsWith('[') || !trimmed.endsWith(']')) return null
  return trimmed
}

function splitTopLevel(lines: string[]): { pre: string[]; headerIndex: number } {
  const headerIndex = lines.findIndex((line) => headerKey(line) !== null)
  return { pre: headerIndex === -1 ? lines : lines.slice(0, headerIndex), headerIndex }
}

function upsertTopLevel(text: string, entries: Array<[string, string]>): string {
  const eol = text.includes('\r\n') ? '\r\n' : '\n'
  const lines = text.split(/\r?\n/)
  const { pre, headerIndex } = splitTopLevel(lines)
  const rest = headerIndex === -1 ? [] : lines.slice(headerIndex)
  const nextPre = [...pre]

  for (const [key, value] of entries) {
    const pattern = new RegExp(`^\\s*${key}\\s*=`)
    const index = nextPre.findIndex((line) => pattern.test(line))
    if (index >= 0) {
      nextPre[index] = `${key} = ${value}`
    } else {
      let insertAt = nextPre.length
      while (insertAt > 0 && nextPre[insertAt - 1]?.trim() === '') insertAt -= 1
      nextPre.splice(insertAt, 0, `${key} = ${value}`)
    }
  }

  const body = rest.length ? ['', ...rest] : []
  const output = [...nextPre, ...body].join(eol)
  return output.endsWith(eol) ? output : `${output}${eol}`
}

function ensureTopLevel(text: string, key: string, value: string): string {
  const lines = text.split(/\r?\n/)
  const { pre } = splitTopLevel(lines)
  const pattern = new RegExp(`^\\s*${key}\\s*=`)
  if (pre.some((line) => pattern.test(line))) return text
  return upsertTopLevel(text, [[key, value]])
}

function removeProviderSection(text: string): string {
  const eol = text.includes('\r\n') ? '\r\n' : '\n'
  const lines = text.split(/\r?\n/)
  const kept: string[] = []
  let skipping = false
  for (const line of lines) {
    const header = headerKey(line)
    if (header) {
      skipping = /^\[model_providers\.csmy(?:\..*)?\]$/.test(header)
      if (!skipping) kept.push(line)
      continue
    }
    if (!skipping) kept.push(line)
  }
  while (kept.length && kept[kept.length - 1]?.trim() === '') kept.pop()
  return `${kept.join(eol)}${eol}`
}

function appendProviderSection(text: string, ctx: AgentContext): string {
  const eol = text.includes('\r\n') ? '\r\n' : '\n'
  const wireApi = ctx.options.codexWireApi === 'chat' ? 'chat' : 'responses'
  const section = [
    `[model_providers.${PROVIDER_ID}]`,
    `name = ${tomlString(PROVIDER_NAME)}`,
    `base_url = ${tomlString(openaiBaseUrl(ctx.baseUrl))}`,
    `wire_api = ${tomlString(wireApi)}`,
    'requires_openai_auth = true',
    ''
  ].join(eol)
  const trimmed = text.replace(/\s+$/, '')
  return `${trimmed}${eol}${eol}${section}`
}

function readAuth(file: string): Record<string, unknown> {
  return parseJson<Record<string, unknown>>(readText(file), 'Codex auth.json')
}

export const codexAdapter: AgentAdapter = {
  id: 'codex',
  configPath,

  async status(ctx) {
    const file = configPath(ctx)
    try {
      const text = readText(file)
      if (!text) {
        return { configPath: file, connected: false, appliedModel: null }
      }
      const doc = parse(text) as Record<string, unknown>
      const providers = (doc.model_providers || {}) as Record<string, Record<string, unknown>>
      const provider = providers[PROVIDER_ID]
      const baseUrl = provider && typeof provider.base_url === 'string' ? provider.base_url : ''
      let baseMatches = false
      if (baseUrl) {
        try {
          baseMatches = normalizeBaseUrl(baseUrl) === normalizeBaseUrl(ctx.baseUrl)
        } catch {
          baseMatches = false
        }
      }
      const auth = readAuth(authPath(ctx))
      const hasKey = typeof auth.OPENAI_API_KEY === 'string' && auth.OPENAI_API_KEY.length > 0
      const providerId = typeof doc.model_provider === 'string' ? doc.model_provider : ''
      return {
        configPath: file,
        connected: providerId === PROVIDER_ID && Boolean(provider) && baseMatches && hasKey,
        appliedModel: typeof doc.model === 'string' ? doc.model : null
      }
    } catch (error) {
      return {
        configPath: file,
        connected: false,
        appliedModel: null,
        error: error instanceof Error ? error.message : String(error)
      }
    }
  },

  apply(ctx, input: ApplyOptions): ApplyResult {
    const file = configPath(ctx)
    const auth = authPath(ctx)
    ensureDir(path.dirname(file))
    const current = readText(file) || ''
    let alreadyConnected = false
    if (current.trim()) {
      try {
        const doc = parse(current) as Record<string, any>
        const provider = doc.model_providers?.csmy
        alreadyConnected =
          doc.model_provider === PROVIDER_ID &&
          Boolean(provider) &&
          typeof provider.base_url === 'string' &&
          normalizeBaseUrl(provider.base_url) === normalizeBaseUrl(ctx.baseUrl)
      } catch {
        alreadyConnected = false
      }
    }
    const latest = latestBackup(ctx, 'codex')
    const backup = alreadyConnected && latest ? latest : createBackup(ctx, 'codex', [file, auth])

    try {
      let next = removeProviderSection(current)
      next = upsertTopLevel(next, [
        ['model_provider', tomlString(PROVIDER_ID)],
        ['model', tomlString(input.model)]
      ])
      next = ensureTopLevel(next, 'model_reasoning_effort', tomlString('medium'))
      next = appendProviderSection(next, ctx)
      parse(next)

      writeTextAtomic(file, next)
      writeTextAtomic(auth, stringifyJson({ OPENAI_API_KEY: ctx.apiKey }))
    } catch (error) {
      restoreBackup(ctx, backup)
      throw new Error(
        `Codex 配置写入失败：${error instanceof Error ? error.message : String(error)}`
      )
    }

    return {
      configPath: file,
      backupId: backup.id,
      message: 'Codex 配置已更新（已写入 config.toml 与 auth.json）'
    }
  },

  restore(ctx): string {
    const file = configPath(ctx)
    const auth = authPath(ctx)
    const meta = latestBackup(ctx, 'codex')
    if (meta) {
      restoreBackup(ctx, meta)
      return 'Codex 已恢复接入前的 config.toml 与 auth.json'
    }

    const text = readText(file)
    if (text) {
      const next = removeProviderSection(text)
      writeTextAtomic(file, next)
    }
    if (exists(auth)) {
      const data = readAuth(auth)
      if (data.OPENAI_API_KEY) {
        delete data.OPENAI_API_KEY
        writeTextAtomic(auth, stringifyJson(data))
      }
    }
    return '未找到备份，已移除本工具写入的 Codex Provider'
  }
}
