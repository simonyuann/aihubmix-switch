import path from 'node:path'
import { applyEdits, modify, parse as parseJsonc, type ParseError } from 'jsonc-parser'
import type { AgentAdapter, AgentContext, ApplyOptions, ApplyResult } from './types'
import { normalizeBaseUrl, openaiBaseUrl, isSelectableModel } from '../../shared/url'
import { PROVIDER_ID, PROVIDER_NAME } from '../../shared/constants'
import { createBackup, latestBackup, restoreBackup, saveBackupMeta } from './backup'
import { parseJson, readText, stringifyJson, writeTextAtomic } from './fs-utils'

interface PiModels {
  providers?: Record<string, Record<string, unknown>>
  [key: string]: unknown
}

interface PiSettings {
  defaultProvider?: string
  defaultModel?: string
  [key: string]: unknown
}

function agentDir(ctx: AgentContext): string {
  const override = ctx.env.PI_CODING_AGENT_DIR?.trim()
  return override || path.join(ctx.home, '.pi', 'agent')
}

function modelsPath(ctx: AgentContext): string {
  return path.join(agentDir(ctx), 'models.json')
}

function settingsPath(ctx: AgentContext): string {
  return path.join(agentDir(ctx), 'settings.json')
}

function parseJsoncValue<T>(text: string | null, label: string): T {
  if (!text || !text.trim()) return {} as T
  const errors: ParseError[] = []
  const value = parseJsonc(text, errors, { allowTrailingComma: true, disallowComments: false })
  if (errors.length) throw new Error(`${label} 格式不正确，请先手动检查该文件`)
  return (value || {}) as T
}

function setJsoncValues(text: string, entries: Array<[string, unknown]>): string {
  let next = text.trim() || '{\n}\n'
  for (const [key, value] of entries) {
    const edits = modify(next, [key], value, {
      formattingOptions: { insertSpaces: true, tabSize: 2 }
    })
    next = applyEdits(next, edits)
  }
  return next.endsWith('\n') ? next : `${next}\n`
}

function buildProvider(ctx: AgentContext, input: ApplyOptions): Record<string, unknown> {
  const seen = new Set<string>()
  const models: Array<{
    id: string
    name: string
    reasoning?: boolean
    contextWindow?: number
    maxTokens?: number
    input?: string[]
  }> = []
  for (const model of input.models) {
    if (!isSelectableModel(model.id) && model.id !== input.model) continue
    if (seen.has(model.id)) continue
    seen.add(model.id)
    models.push({
      id: model.id,
      name: model.name || model.id,
      ...(model.reasoning ? { reasoning: true } : {}),
      ...(model.contextWindow ? { contextWindow: model.contextWindow } : {}),
      ...(model.maxTokens ? { maxTokens: model.maxTokens } : {}),
      ...(model.input?.length ? { input: model.input } : {})
    })
  }
  if (!seen.has(input.model)) {
    models.unshift({ id: input.model, name: input.model })
  }
  return {
    name: PROVIDER_NAME,
    baseUrl: openaiBaseUrl(ctx.baseUrl),
    api: ctx.options.piApi,
    apiKey: ctx.apiKey,
    models
  }
}

export const piAdapter: AgentAdapter = {
  id: 'pi',
  configPath: modelsPath,

  async status(ctx) {
    const modelsFile = modelsPath(ctx)
    const settingsFile = settingsPath(ctx)
    try {
      const models = parseJson<PiModels>(readText(modelsFile), 'Pi models.json')
      const settings = parseJsoncValue<PiSettings>(readText(settingsFile), 'Pi settings.json')
      const provider = models.providers?.[PROVIDER_ID]
      const baseUrl = provider && typeof provider.baseUrl === 'string' ? provider.baseUrl : ''
      let baseMatches = false
      if (baseUrl) {
        try {
          baseMatches = normalizeBaseUrl(baseUrl) === normalizeBaseUrl(ctx.baseUrl)
        } catch {
          baseMatches = false
        }
      }
      return {
        configPath: modelsFile,
        connected: Boolean(provider) && baseMatches && provider?.apiKey === ctx.apiKey,
        appliedModel:
          settings.defaultProvider === PROVIDER_ID && typeof settings.defaultModel === 'string'
            ? settings.defaultModel
            : null
      }
    } catch (error) {
      return {
        configPath: modelsFile,
        connected: false,
        appliedModel: null,
        error: error instanceof Error ? error.message : String(error)
      }
    }
  },

  apply(ctx, input: ApplyOptions): ApplyResult {
    const modelsFile = modelsPath(ctx)
    const settingsFile = settingsPath(ctx)
    const models = parseJson<PiModels>(readText(modelsFile), 'Pi models.json')
    if (!models.providers || typeof models.providers !== 'object') models.providers = {}

    const previousProvider = models.providers[PROVIDER_ID] ?? null
    const settingsText = readText(settingsFile) || ''
    const settings = parseJsoncValue<PiSettings>(settingsText, 'Pi settings.json')
    const previousDefaults = {
      defaultProvider: settings.defaultProvider ?? null,
      defaultModel: settings.defaultModel ?? null
    }

    const provider = buildProvider(ctx, input)
    const writtenDefaults = { defaultProvider: PROVIDER_ID, defaultModel: input.model }

    const latest = latestBackup(ctx, 'pi')
    const currentProvider = models.providers[PROVIDER_ID]
    let alreadyConnected = false
    if (currentProvider && typeof currentProvider.baseUrl === 'string') {
      try {
        alreadyConnected =
          normalizeBaseUrl(currentProvider.baseUrl) === normalizeBaseUrl(ctx.baseUrl) &&
          currentProvider.apiKey === ctx.apiKey
      } catch {
        alreadyConnected = false
      }
    }

    let backup
    if (alreadyConnected && latest && latest.previous?.previousProvider !== undefined) {
      latest.previous = {
        ...latest.previous,
        writtenProvider: provider,
        writtenDefaults
      }
      saveBackupMeta(ctx, latest)
      backup = latest
    } else {
      backup = createBackup(ctx, 'pi', [modelsFile, settingsFile], {
        previousProvider,
        previousDefaults,
        writtenProvider: provider,
        writtenDefaults
      })
    }

    try {
      models.providers[PROVIDER_ID] = provider
      writeTextAtomic(modelsFile, stringifyJson(models))

      let nextSettings = settingsText
      nextSettings = setJsoncValues(nextSettings, [
        ['defaultProvider', PROVIDER_ID],
        ['defaultModel', input.model]
      ])
      writeTextAtomic(settingsFile, nextSettings)
    } catch (error) {
      restoreBackup(ctx, backup)
      throw new Error(`Pi 配置写入失败：${error instanceof Error ? error.message : String(error)}`)
    }

    return {
      configPath: modelsFile,
      backupId: backup.id,
      message: 'Pi 配置已更新（models.json + settings.json），重启 Pi 后生效'
    }
  },

  restore(ctx): string {
    const modelsFile = modelsPath(ctx)
    const settingsFile = settingsPath(ctx)
    const meta = latestBackup(ctx, 'pi')
    if (!meta) {
      return '没有找到 Pi 的备份，请手动检查配置'
    }

    const previousProvider = meta.previous?.previousProvider as
      | Record<string, unknown>
      | null
      | undefined
    const previousDefaults = (meta.previous?.previousDefaults || {}) as Record<string, unknown>
    const writtenProvider = (meta.previous?.writtenProvider || {}) as Record<string, unknown>
    const writtenDefaults = (meta.previous?.writtenDefaults || {}) as Record<string, unknown>

    try {
      const models = parseJson<PiModels>(readText(modelsFile), 'Pi models.json')
      const current = models.providers?.[PROVIDER_ID]
      const sameProvider =
        current &&
        writtenProvider &&
        current.baseUrl === writtenProvider.baseUrl &&
        current.apiKey === writtenProvider.apiKey
      if (models.providers && sameProvider) {
        if (previousProvider) {
          models.providers[PROVIDER_ID] = previousProvider
        } else {
          delete models.providers[PROVIDER_ID]
        }
        writeTextAtomic(modelsFile, stringifyJson(models))
      }

      const settingsText = readText(settingsFile) || ''
      const settings = parseJsoncValue<PiSettings>(settingsText, 'Pi settings.json')
      if (
        settings.defaultProvider === writtenDefaults.defaultProvider &&
        settings.defaultModel === writtenDefaults.defaultModel
      ) {
        const providerValue =
          typeof previousDefaults.defaultProvider === 'string'
            ? previousDefaults.defaultProvider
            : undefined
        const modelValue =
          typeof previousDefaults.defaultModel === 'string' ? previousDefaults.defaultModel : undefined
        let next = setJsoncValues(settingsText, [
          ['defaultProvider', providerValue],
          ['defaultModel', modelValue]
        ])
        writeTextAtomic(settingsFile, next)
      }

      return 'Pi 已恢复接入前的配置'
    } catch (error) {
      restoreBackup(ctx, meta)
      return `Pi 已从备份恢复（${error instanceof Error ? error.message : '配置解析失败'}）`
    }
  }
}
