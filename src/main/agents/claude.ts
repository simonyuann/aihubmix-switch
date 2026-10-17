import path from 'node:path'
import type { AgentAdapter, AgentContext, ApplyOptions, ApplyResult } from './types'
import { anthropicBaseUrl, normalizeBaseUrl } from '../../shared/url'
import { createBackup, latestBackup, restoreBackup, saveBackupMeta } from './backup'
import { exists, parseJson, readText, stringifyJson, writeTextAtomic } from './fs-utils'

interface Settings {
  env?: Record<string, unknown>
  [key: string]: unknown
}

const ENV_KEYS = [
  'ANTHROPIC_BASE_URL',
  'ANTHROPIC_AUTH_TOKEN',
  'ANTHROPIC_API_KEY',
  'ANTHROPIC_MODEL',
  'ANTHROPIC_DEFAULT_SONNET_MODEL',
  'ANTHROPIC_DEFAULT_OPUS_MODEL',
  'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  'ANTHROPIC_SMALL_FAST_MODEL'
] as const

function configDir(ctx: AgentContext): string {
  const override = ctx.env.CLAUDE_CONFIG_DIR?.trim()
  return override || path.join(ctx.home, '.claude')
}

function configPath(ctx: AgentContext): string {
  return path.join(configDir(ctx), 'settings.json')
}

function readSettings(file: string): Settings {
  return parseJson<Settings>(readText(file), 'Claude Code settings.json')
}

export const claudeAdapter: AgentAdapter = {
  id: 'claude',
  configPath,

  async status(ctx) {
    const file = configPath(ctx)
    try {
      const settings = readSettings(file)
      const env = (settings.env || {}) as Record<string, unknown>
      const base = typeof env.ANTHROPIC_BASE_URL === 'string' ? env.ANTHROPIC_BASE_URL : ''
      const token =
        (typeof env.ANTHROPIC_AUTH_TOKEN === 'string' && env.ANTHROPIC_AUTH_TOKEN) ||
        (typeof env.ANTHROPIC_API_KEY === 'string' && env.ANTHROPIC_API_KEY) ||
        ''
      let baseMatches = false
      if (base) {
        try {
          baseMatches = normalizeBaseUrl(base) === normalizeBaseUrl(ctx.baseUrl)
        } catch {
          baseMatches = false
        }
      }
      return {
        configPath: file,
        connected: Boolean(token) && baseMatches && token === ctx.apiKey,
        appliedModel: typeof env.ANTHROPIC_MODEL === 'string' ? env.ANTHROPIC_MODEL : null
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
    const settings = readSettings(file)
    const currentEnv = { ...((settings.env || {}) as Record<string, unknown>) }

    const baseUrl = anthropicBaseUrl(ctx.baseUrl)
    const written: Record<string, string> = {
      ANTHROPIC_BASE_URL: baseUrl,
      ANTHROPIC_AUTH_TOKEN: ctx.apiKey,
      ANTHROPIC_API_KEY: ctx.apiKey,
      ANTHROPIC_MODEL: input.model,
      ANTHROPIC_DEFAULT_SONNET_MODEL: input.model,
      ANTHROPIC_DEFAULT_OPUS_MODEL: input.model,
      ANTHROPIC_DEFAULT_HAIKU_MODEL: input.model,
      ANTHROPIC_SMALL_FAST_MODEL: input.model
    }
    const previous: Record<string, string | null> = {}
    for (const key of ENV_KEYS) {
      previous[key] =
        typeof currentEnv[key] === 'string' && currentEnv[key] ? (currentEnv[key] as string) : null
    }

    const latest = latestBackup(ctx, 'claude')
    const currentBase =
      typeof currentEnv.ANTHROPIC_BASE_URL === 'string' ? currentEnv.ANTHROPIC_BASE_URL : ''
    let alreadyConnected = false
    try {
      alreadyConnected =
        Boolean(currentBase) &&
        normalizeBaseUrl(currentBase) === normalizeBaseUrl(ctx.baseUrl) &&
        (currentEnv.ANTHROPIC_AUTH_TOKEN === ctx.apiKey ||
          currentEnv.ANTHROPIC_API_KEY === ctx.apiKey)
    } catch {
      alreadyConnected = false
    }

    let backup
    if (alreadyConnected && latest && latest.previous?.previous) {
      latest.previous = { previous: latest.previous.previous, written }
      saveBackupMeta(ctx, latest)
      backup = latest
    } else {
      backup = createBackup(ctx, 'claude', [file], { previous, written })
    }
    try {
      settings.env = { ...currentEnv, ...written }
      writeTextAtomic(file, stringifyJson(settings))
    } catch (error) {
      restoreBackup(ctx, backup)
      throw error
    }

    return {
      configPath: file,
      backupId: backup.id,
      message: exists(file) ? 'Claude Code 配置已更新，重启 Claude Code 后生效' : 'Claude Code 配置已创建，启动后生效'
    }
  },

  restore(ctx): string {
    const file = configPath(ctx)
    const meta = latestBackup(ctx, 'claude')
    if (!meta) {
      return '没有找到 Claude Code 的备份，请手动检查配置'
    }
    const previous = (meta.previous?.previous || {}) as Record<string, string | null>
    const written = (meta.previous?.written || {}) as Record<string, string>
    try {
      const settings = readSettings(file)
      const env = { ...((settings.env || {}) as Record<string, unknown>) }
      let changed = false
      for (const key of ENV_KEYS) {
        if (env[key] !== written[key]) continue
        const before = previous[key]
        if (before) {
          env[key] = before
        } else {
          delete env[key]
        }
        changed = true
      }
      if (changed) {
        settings.env = env
        writeTextAtomic(file, stringifyJson(settings))
      }
      return 'Claude Code 已恢复接入前的配置'
    } catch (error) {
      restoreBackup(ctx, meta)
      return `Claude Code 已从备份恢复（${error instanceof Error ? error.message : '配置解析失败'}）`
    }
  }
}
