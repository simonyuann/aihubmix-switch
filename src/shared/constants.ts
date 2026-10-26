import type { AgentId, AgentMeta } from './types'

export const DEFAULT_BASE_URL = 'https://csmy.fun:23457'
export const DEFAULT_DOC_URL = 'https://docs.csmy.fun:23457'
export const APP_NAME = 'AIHubMix 接入助手'
export const PROVIDER_ID = 'csmy'
export const PROVIDER_NAME = 'AIHubMix'

export const AGENT_ORDER: AgentId[] = [
  'claude',
  'codex',
  'pi',
  'opencode',
  'grokbuild',
  'kimi',
  'openclaw',
  'gemini',
  'claude-desktop',
  'hermes',
  'mcode',
  'omp',
  'agy'
]

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
  gemini: {
    id: 'gemini',
    name: 'Gemini CLI',
    subtitle: 'Google 原生 Gemini 协议',
    color: '#4285f4',
    configHint: '~/.gemini/settings.json + .env',
    installCommand: 'npm install -g @google/gemini-cli',
    setupNote:
      '需要当前 Key 支持 Gemini 原生接口（/v1beta）。导入前验证 Gemini 流式工具调用；普通 OpenAI 兼容接口不能直接替代。项目内 .env 或 shell 环境可能覆盖全局配置。'
  },
  'claude-desktop': {
    id: 'claude-desktop',
    name: 'Claude Desktop',
    subtitle: '桌面版第三方网关',
    color: '#c87e61',
    desktop: true,
    configHint: 'Claude-3p/configLibrary/',
    installCommand: 'https://claude.com/download',
    setupNote:
      '写入桌面版第三方网关配置，需要支持该功能的新版本。会自动选择可用的 Claude 模型验证 Messages 接口。完全退出后重新打开桌面应用。Linux 依赖所用桌面移植版本的支持。'
  },
  hermes: {
    id: 'hermes',
    name: 'Hermes Agent',
    subtitle: '自定义 OpenAI 兼容供应商',
    color: '#936f3c',
    configHint: '~/.hermes/config.yaml',
    installCommand: 'https://hermes-agent.nousresearch.com/docs/getting-started/installation/',
    setupNote:
      '添加独立供应商并设为默认模型；保留原供应商和其他设置。Windows 按 HERMES_HOME 或本机 LocalAppData 目录配置。'
  },
  mcode: {
    id: 'mcode',
    name: 'MiniMax Code',
    subtitle: 'MCode 终端与桌面共享供应商',
    color: '#cf5473',
    configHint: '~/.minimax/config.yaml',
    installCommand: 'https://github.com/MiniMax-AI',
    setupNote:
      '添加 AIHubMix 自定义供应商，使用 OpenAI Completions 接口。导入后在 MCode 的模型选择器中选择 csmy 下的模型；默认模型由原工具管理。'
  },
  omp: {
    id: 'omp',
    name: 'Oh My Pi',
    subtitle: '独立供应商目录与命名 Profile',
    color: '#7351a3',
    configHint: '~/.omp/agent/models.yml',
    installCommand: 'npm install -g @oh-my-pi/pi-coding-agent',
    setupNote:
      '添加自定义供应商，保留 Oh My Pi 的登录和默认模型。导入后在原工具的 /model 中选择 csmy 下的模型。支持当前 OMP_PROFILE / PI_PROFILE。'
  },
  agy: {
    id: 'agy',
    name: 'Antigravity CLI',
    subtitle: '专用启动环境 · Gemini 协议',
    color: '#397abe',
    configHint: '本应用数据目录 / agent-profiles/agy',
    installCommand: 'https://www.agy.dev/docs/cli/',
    setupNote:
      '适用于 agy CLI，需要 Gemini 原生接口。会自动选择兼容模型完成配置；启动后可使用 /model 切换。导入完成后复制专用启动命令运行；普通 agy 启动方式仍使用原账号。不会修改 Antigravity IDE 账号。'
  },
  grokbuild: {
    id: 'grokbuild',
    name: 'Grok Build',
    subtitle: 'xAI 编码智能体',
    color: '#24272a',
    configHint: '~/.grok/config.toml',
    installCommand: 'npm install -g @xai-official/grok'
  },
  kimi: {
    id: 'kimi',
    name: 'Kimi Code',
    subtitle: '支持自定义模型供应商',
    color: '#3d63df',
    configHint: '~/.kimi-code/config.toml',
    installCommand: 'npm install -g @moonshot-ai/kimi-code'
  },
  openclaw: {
    id: 'openclaw',
    name: 'OpenClaw',
    subtitle: '个人智能体与多渠道助手',
    color: '#de6458',
    configHint: '~/.openclaw/openclaw.json',
    installCommand: 'npm install -g openclaw'
  },
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
    color: '#283f43',
    configHint: '~/.codex/config.toml',
    installCommand: 'npm install -g @openai/codex'
  },
  pi: {
    id: 'pi',
    name: 'Pi',
    subtitle: '轻量终端编码 Agent',
    color: '#0d9488',
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
