import path from 'node:path'
import { applyEdits, modify, parse as parseJsonc, type ParseError } from 'jsonc-parser'
import type { AgentAdapter, AgentContext, ApplyOptions, ApplyResult } from './types'
import { isSelectableModel, normalizeBaseUrl, openaiBaseUrl } from '../../shared/url'
import { PROVIDER_ID, PROVIDER_NAME } from '../../shared/constants'
import { createBackup, latestBackup, restoreBackup, saveBackupMeta } from './backup'
import { exists, readText, writeTextAtomic } from './fs-utils'

interface OpenCodeConfig {
  provider?: Record<string, Record<string, unknown>>
  providers?: Record<string, unknown>
  model?: string
  [key: string]: unknown
}

function configDir(ctx: AgentContext): string {
  const explicit = ctx.env.OPENCODE_CONFIG_DIR?.trim()
  if (explicit) return explicit
  const xdg = ctx.env.XDG_CONFIG_HOME?.trim()
  return xdg ? path.join(xdg, 'opencode') : path.join(ctx.home, '.config', 'opencode')
}

function configPath(ctx: AgentContext): string {
  const dir = configDir(ctx)
  const jsonc = path.join(dir, 'opencode.jsonc')
  if (exists(jsonc)) return jsonc
  return path.join(dir, 'opencode.json')
}

function parseValue(text: string | null, label: string): OpenCodeConfig {
  if (!text || !text.trim()) return {}
  const errors: ParseError[] = []
  const value = parseJsonc(text, errors, { allowTrailingComma: true, disallowComments: false })
  if (errors.length) throw new Error(`${label} 格式不正确，请先手动检查该文件`)
  return (value || {}) as OpenCodeConfig
}

function buildProvider(ctx: AgentContext, input: ApplyOptions): Record<string, unknown> {
  const models: Record<string, Record<string, unknown>> = {}
  for (const model of input.models) {
    if (!isSelectableModel(model.id) && model.id !== input.model) continue
    models[model.id] = {
      name: model.name || model.id,
      tool_call: true,
      ...(model.reasoning ? { reasoning: true } : {}),
      ...(model.input?.includes('image') ? { attachment: true } : {}),
      ...(model.contextWindow && model.maxTokens
        ? { limit: { context: model.contextWindow, output: model.maxTokens } }
        : {})
    }
  }
  if (!models[input.model]) models[input.model] = { name: input.model }
  return {
    npm: '@ai-sdk/openai-compatible',
    name: PROVIDER_NAME,
    options: {
      baseURL: openaiBaseUrl(ctx.baseUrl),
      apiKey: ctx.apiKey
    },
    models
  }
}

function applyEditsSequentially(
  text: string,
  entries: Array<{ path: Array<string | number>; value: unknown }>
): string {
  let next = text.trim() || '{\n}\n'
  for (const entry of entries) {
    const edits = modify(next, entry.path, entry.value, {
      formattingOptions: { insertSpaces: true, tabSize: 2 }
    })
    next = applyEdits(next, edits)
  }
  return next.endsWith('\n') ? next : `${next}\n`
}

export const opencodeAdapter: AgentAdapter = {
  id: 'opencode',
  configPath,

  async status(ctx) {
    const file = configPath(ctx)
    try {
      const value = parseValue(readText(file), 'OpenCode 配置')
      const provider = value.provider?.[PROVIDER_ID]
      const options = (provider?.options || {}) as Record<string, unknown>
      const baseUrl = typeof options.baseURL === 'string' ? options.baseURL : ''
      let baseMatches = false
      if (baseUrl) {
        try {
          baseMatches = normalizeBaseUrl(baseUrl) === normalizeBaseUrl(ctx.baseUrl)
        } catch {
          baseMatches = false
        }
      }
      const selected = typeof value.model === 'string' ? value.model : ''
      return {
        configPath: file,
        connected: Boolean(provider) && baseMatches && options.apiKey === ctx.apiKey,
        appliedModel: selected.startsWith(`${PROVIDER_ID}/`)
          ? selected.slice(PROVIDER_ID.length + 1)
          : null
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
    const currentText = readText(file) || ''
    const current = parseValue(currentText, 'OpenCode 配置')
    const previousProvider = current.provider?.[PROVIDER_ID] ?? null
    const previousModel = typeof current.model === 'string' ? current.model : null
    const provider = buildProvider(ctx, input)
    const writtenModel = `${PROVIDER_ID}/${input.model}`

    const latest = latestBackup(ctx, 'opencode')
    const currentProvider = current.provider?.[PROVIDER_ID]
    const currentOptions = (currentProvider?.options || {}) as Record<string, unknown>
    let alreadyConnected = false
    if (typeof currentOptions.baseURL === 'string') {
      try {
        alreadyConnected =
          normalizeBaseUrl(currentOptions.baseURL) === normalizeBaseUrl(ctx.baseUrl) &&
          currentOptions.apiKey === ctx.apiKey
      } catch {
        alreadyConnected = false
      }
    }

    let backup
    if (alreadyConnected && latest && latest.previous?.writtenProvider) {
      latest.previous = {
        ...latest.previous,
        writtenProvider: provider,
        writtenModel
      }
      saveBackupMeta(ctx, latest)
      backup = latest
    } else {
      backup = createBackup(ctx, 'opencode', [file], {
        previousProvider,
        previousModel,
        writtenProvider: provider,
        writtenModel
      })
    }

    try {
      let next = currentText
      if (!next.trim()) {
        next = '{\n  "$schema": "https://opencode.ai/config.json"\n}\n'
      }
      next = applyEditsSequentially(next, [
        { path: ['provider', PROVIDER_ID], value: provider },
        { path: ['model'], value: writtenModel }
      ])
      parseValue(next, 'OpenCode 配置')
      writeTextAtomic(file, next)
    } catch (error) {
      restoreBackup(ctx, backup)
      throw new Error(
        `OpenCode 配置写入失败：${error instanceof Error ? error.message : String(error)}`
      )
    }

    return {
      configPath: file,
      backupId: backup.id,
      message: 'OpenCode 配置已更新，重启 OpenCode 后生效'
    }
  },

  restore(ctx): string {
    const file = configPath(ctx)
    const meta = latestBackup(ctx, 'opencode')
    if (!meta) {
      return '没有找到 OpenCode 的备份，请手动检查配置'
    }

    const previousProvider = meta.previous?.previousProvider as
      | Record<string, unknown>
      | null
      | undefined
    const previousModel = meta.previous?.previousModel as string | null | undefined
    const writtenProvider = (meta.previous?.writtenProvider || {}) as Record<string, unknown>
    const writtenModel = meta.previous?.writtenModel as string | undefined

    try {
      const text = readText(file) || ''
      const value = parseValue(text, 'OpenCode 配置')
      const provider = value.provider?.[PROVIDER_ID]
      const options = (provider?.options || {}) as Record<string, unknown>
      const sameProvider =
        provider &&
        options.baseURL ===
          ((writtenProvider.options as Record<string, unknown> | undefined)?.baseURL as string)

      const entries: Array<{ path: Array<string | number>; value: unknown }> = []
      if (sameProvider) {
        entries.push({
          path: ['provider', PROVIDER_ID],
          value: previousProvider ?? undefined
        })
      }
      if (writtenModel && value.model === writtenModel) {
        entries.push({ path: ['model'], value: previousModel ?? undefined })
      }
      if (entries.length) {
        const next = applyEditsSequentially(text, entries)
        writeTextAtomic(file, next)
      }
      return 'OpenCode 已恢复接入前的配置'
    } catch (error) {
      restoreBackup(ctx, meta)
      return `OpenCode 已从备份恢复（${
        error instanceof Error ? error.message : '配置解析失败'
      }）`
    }
  }
}
