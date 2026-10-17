import type { AgentId, AgentMeta } from './types'

export const DEFAULT_BASE_URL = 'https://csmy.fun:23457'
export const DEFAULT_DOC_URL = 'https://docs.csmy.fun:23457'
export const APP_NAME = 'AIHubMix 接入助手'
export const PROVIDER_ID = 'csmy'
export const PROVIDER_NAME = 'AIHubMix'

export const AGENT_ORDER: AgentId[] = ['claude', 'codex', 'pi', 'opencode']

export const DEFAULT_CODEX_EFFORT = 'medium'

export const CODEX_EFFORTS = [
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
  'ultra'
] as const

export const CODEX_EFFORT_LABELS: Record<string, string> = {
  auto: '自动（不写入配置）',
  none: '关闭',
  minimal: '极低',
  low: '低',
  medium: '中',
  high: '高',
  xhigh: '极高',
  max: '最大',
  ultra: '极限'
}

export const AGENTS: Record<AgentId, AgentMeta> = {
  claude: {
    id: 'claude',
    name: 'Claude Code',
    subtitle: 'Anthropic 官方 CLI',
    color: '#d97757',
    configHint: '~/.claude/settings.json',
    installCommand: 'npm install -g @anthropic-ai/claude-code'
  },
  codex: {
    id: 'codex',
    name: 'Codex CLI',
    subtitle: 'OpenAI Codex 编码代理',
    color: '#e8ecf8',
    configHint: '~/.codex/config.toml',
    installCommand: 'npm install -g @openai/codex'
  },
  pi: {
    id: 'pi',
    name: 'Pi',
    subtitle: '轻量终端编码 Agent',
    color: '#5eead4',
    configHint: '~/.pi/agent/models.json',
    installCommand: 'npm install -g @earendil-works/pi-coding-agent'
  },
  opencode: {
    id: 'opencode',
    name: 'OpenCode',
    subtitle: '开源终端编码 Agent',
    color: '#60a5fa',
    configHint: '~/.config/opencode/opencode.json',
    installCommand: 'npm install -g opencode-ai'
  }
}
