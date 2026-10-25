// This module is loaded only by Vite dev mode with ?preview=1. No real secrets or writes.
import type {
  ApiKeyInfo,
  AppState,
  RendererApi,
  RelayProbe,
  SessionIndex
} from '../../shared/types'
import { AGENT_ORDER, AGENTS } from '../../shared/constants'

export const demoSessions: SessionIndex = {
  sessions: [
    {
      id: 'demo-codex',
      agent: 'codex',
      sessionId: 'demo-project',
      title: '整理项目的 API 接入流程',
      project: '~/Projects/example',
      modifiedAt: 1792909200000,
      archived: false,
      preview: '已完成配置检查，下一步完善导入体验。'
    },
    {
      id: 'demo-claude',
      agent: 'claude',
      sessionId: 'demo-review',
      title: '检查模型调用与错误提示',
      project: '~/Projects/example',
      modifiedAt: 1792905600000,
      archived: false,
      preview: '检查失败时保留原配置，并给出可操作的提示。'
    }
  ],
  sources: ['codex', 'claude', 'pi', 'grokbuild', 'gemini', 'openclaw'].map((agent) => ({
    agent,
    count: ['codex', 'claude'].includes(agent) ? 1 : 0,
    status: ['codex', 'claude'].includes(agent) ? 'ready' : 'missing',
    note: '未找到本地会话目录'
  })),
  truncated: false
}

export const previewState: AppState = {
  version: '0.3.2',
  secureStorage: true,
  site: {
    name: 'AIHubMix',
    subtitle: '让每一次连接更简单',
    apiBaseUrl: 'https://csmy.fun:23457',
    docUrl: 'https://docs.csmy.fun:23457',
    registerUrl: 'https://csmy.fun:23457/register',
    loginUrl: 'https://csmy.fun:23457/login'
  },
  relayBaseUrl: 'https://csmy.fun:23457',
  hasApiKey: true,
  keyPreview: 'sk-••••7a2f',
  account: { loggedIn: true, email: 'simon@example.com', userId: 1 },
  models: [
    {
      id: 'gpt-5.4',
      name: 'GPT-5.4',
      reasoning: true,
      reasoningLevels: ['low', 'medium', 'high', 'xhigh'],
      contextWindow: 272000
    },
    { id: 'deepseek-chat', name: 'DeepSeek Chat' }
  ],
  balance: null,
  rateMultiplier: 1,
  messages: 'supported',
  relayError: null,
  agents: [
    {
      id: 'codex',
      installed: true,
      version: null,
      configPath: '~/.codex/config.toml',
      connected: true,
      appliedModel: 'gpt-5.4',
      appliedEffort: 'medium',
      backupAvailable: true,
      keyLabel: '日常编程 · sk-••••7a2f',
      keyId: 11
    },
    {
      id: 'claude',
      installed: false,
      version: null,
      configPath: '~/.claude/settings.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    },
    {
      id: 'opencode',
      installed: false,
      version: null,
      configPath: '~/.config/opencode/opencode.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    },
    {
      id: 'pi',
      installed: false,
      version: null,
      configPath: '~/.pi/agent/models.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    }
  ],
  agentModels: { codex: 'gpt-5.4', claude: '', opencode: '', pi: '' },
  options: { piApi: 'openai-responses', codexWireApi: 'responses', codexReasoningEffort: 'medium' },
  lastAgent: 'codex'
}

for (const id of AGENT_ORDER) {
  if (!previewState.agents.some((a) => a.id === id))
    previewState.agents.push({
      id,
      installed: false,
      version: null,
      configPath: AGENTS[id].configHint,
      connected: false,
      appliedModel: null,
      backupAvailable: false
    })
}

export const demoKeys: ApiKeyInfo[] = [
  {
    id: 11,
    name: '日常编程',
    keyMasked: 'sk-••••7a2f',
    groupId: 1,
    groupName: 'OpenAI',
    status: 'active',
    quotaUsed: 1.25,
    quota: 50,
    createdAt: '2026-10-24T10:00:00Z'
  },
  {
    id: 12,
    name: '轻量任务',
    keyMasked: 'sk-••••c918',
    groupId: 2,
    groupName: 'DeepSeek',
    status: 'active',
    quotaUsed: 0.32,
    quota: null,
    createdAt: '2026-10-25T08:00:00Z'
  }
]

export function createPreviewApi(): RendererApi {
  let state = structuredClone(previewState)
  let keys = structuredClone(demoKeys)
  const scenario =
    typeof location !== 'undefined' ? new URLSearchParams(location.search).get('scenario') : ''
  if (scenario === 'guest') {
    state.account = { loggedIn: false }
    keys = []
    state.hasApiKey = false
    state.agents.forEach((a) => {
      a.connected = false
      a.backupAvailable = false
    })
  }
  if (scenario === 'empty') keys = []
  const current = async () => structuredClone(state)
  const probe = (): RelayProbe => ({
    ok: true,
    baseUrl: state.relayBaseUrl,
    models: state.models,
    balance: null,
    rateMultiplier: 1,
    messages: state.messages,
    latencyMs: 126
  })
  let update = {
    phase: 'idle',
    currentVersion: state.version,
    version: null,
    notes: null,
    downloaded: 0,
    total: null,
    canInstall: true,
    format: 'app',
    error: null
  } as import('../../shared/types').UpdateStatus
  return {
    diagnoseAgent: async (agent) => ({
      agent,
      ok: true,
      checkedAt: Date.now() / 1000,
      latencyMs: 240,
      model: 'gpt-5.4',
      protocol: 'Responses',
      checks: [
        { name: '当前配置', status: 'ok', detail: '演示配置与导入记录一致' },
        {
          name: '模型与工具调用',
          status: 'ok',
          detail: '界面演示：未发送网络请求，也未修改本机配置'
        }
      ]
    }),
    codexHistory: {
      change: async (input) => {
        const history = {
          enabled: input.enabled,
          backupAvailable: !!input.migrate,
          recoveryRequired: false,
          directory: '~/.codex',
          error: null
        }
        state.codexHistory = history
        return { files: 0, rows: 0, history }
      },
      recover: async () => ({
        enabled: false,
        backupAvailable: false,
        recoveryRequired: false,
        directory: '~/.codex',
        error: null
      })
    },
    sessions: {
      list: async () => structuredClone(demoSessions),
      read: async (id) => {
        const session = demoSessions.sessions.find((s) => s.id === id)
        if (!session) throw new Error('会话未找到')
        return {
          session: structuredClone(session),
          messages: [
            { role: 'user', text: session.title },
            { role: 'assistant', text: session.preview }
          ],
          resumeCommand: `${session.agent} ${session.agent === 'codex' ? 'resume' : '--resume'} '${session.sessionId}'`,
          shell: '终端',
          truncated: false,
          note: '界面演示数据；未读取或修改真实会话'
        }
      }
    },
    updates: {
      status: async () => ({ ...update }),
      check: async () => {
        update = {
          ...update,
          phase: 'available',
          version: '0.3.3',
          notes: '演示更新说明：界面预览不会检查网络或安装程序。'
        }
        return { ...update }
      },
      download: async () => {
        update = { ...update, phase: 'ready', downloaded: 1024, total: 1024 }
        return { ...update }
      },
      install: async () => {
        throw new Error('这是界面预览，不会安装更新')
      },
      onStatus: async () => () => {}
    },
    getState: current,
    setCliPath: async (agent, path) => {
      const value = state.agents.find((a) => a.id === agent)
      if (value) {
        value.cliPath = path
        value.installed = !!path
      }
      return current()
    },
    refresh: current,
    keys: async () => structuredClone(keys),
    getKey: async () => 'DEMO-KEY-NOT-A-REAL-CREDENTIAL',
    copyKey: async () => undefined,
    prepareKey: async () => probe(),
    saveKey: async () => {
      const info: ApiKeyInfo = { ...demoKeys[0]!, id: -1, name: '本机 API Key', local: true }
      keys.push(info)
      return { probe: probe(), state: await current(), info }
    },
    deleteKey: async (id) => {
      keys = keys.filter((k) => k.id !== id)
      return { state: await current() }
    },
    removeLocalKey: async (id) => {
      keys = keys.filter((k) => k.id !== id)
      return current()
    },
    importAgent: async (input) => {
      const agent = state.agents.find((a) => a.id === input.agent)!
      agent.connected = true
      agent.backupAvailable = true
      agent.appliedModel = input.model
      agent.keyLabel = input.keyLabel
      return current()
    },
    restoreAgent: async (id) => {
      const agent = state.agents.find((a) => a.id === id)!
      agent.connected = false
      agent.backupAvailable = false
      return current()
    },
    updateOptions: async (patch) => {
      state.options = { ...state.options, ...patch }
      return current()
    },
    openExternal: async () => undefined,
    openPath: async () => undefined,
    account: {
      login: async (input) => {
        state.account = { loggedIn: true, email: input.email }
        return current()
      },
      register: async (input) => {
        state.account = { loggedIn: true, email: input.email }
        return current()
      },
      logout: async () => {
        state.account = { loggedIn: false }
        return current()
      },
      groups: async () => [
        {
          id: 1,
          name: 'OpenAI',
          description: '适合日常 AI 编程与推理任务',
          platform: 'openai',
          rateMultiplier: 1,
          allowMessagesDispatch: true,
          subscriptionType: 'standard'
        }
      ],
      createKey: async (input) => {
        const info = { ...demoKeys[0]!, id: 13, name: input.name || '我的 API Key' }
        keys.push(info)
        return { info, state: await current() }
      }
    }
  }
}
export const previewApi = createPreviewApi()
