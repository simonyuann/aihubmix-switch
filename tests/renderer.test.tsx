// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import App from '../src/renderer/src/App'
import { AccountModal } from '../src/renderer/src/components/AccountModal'
import { SettingsModal } from '../src/renderer/src/components/SettingsModal'
import type { AppState, GroupInfo, RendererApi } from '../src/shared/types'

const state: AppState = {
  version: '0.1.0',
  secureStorage: true,
  site: {
    name: 'AIHubMix',
    subtitle: '订阅转 API 的 AI 聚合平台',
    apiBaseUrl: 'https://csmy.fun:23457',
    docUrl: 'https://docs.csmy.fun:23457',
    registerUrl: 'https://csmy.fun:23457/register',
    loginUrl: 'https://csmy.fun:23457/login'
  },
  relayBaseUrl: 'https://csmy.fun:23457',
  hasApiKey: true,
  keyPreview: 'sk-••••1234',
  account: { loggedIn: true, email: 'user@example.com', userId: 1 },
  models: [
    { id: 'gpt-5.6-luna', name: 'GPT-5.6 Luna', reasoning: true, contextWindow: 272000 },
    { id: 'gpt-image-1', name: 'GPT Image 1' }
  ],
  balance: 6.44,
  rateMultiplier: 0.3,
  messages: 'unsupported',
  relayError: null,
  agents: [
    {
      id: 'claude',
      installed: false,
      version: null,
      configPath: '/home/user/.claude/settings.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    },
    {
      id: 'codex',
      installed: true,
      version: '0.162.1',
      configPath: '/home/user/.codex/config.toml',
      connected: true,
      appliedModel: 'gpt-5.6-luna',
      appliedEffort: 'high',
      backupAvailable: true
    },
    {
      id: 'pi',
      installed: true,
      version: '0.87.1',
      configPath: '/home/user/.pi/agent/models.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    },
    {
      id: 'opencode',
      installed: true,
      version: '1.19.3',
      configPath: '/home/user/.config/opencode/opencode.json',
      connected: false,
      appliedModel: null,
      backupAvailable: false
    }
  ],
  agentModels: {
    claude: 'gpt-5.6-luna',
    codex: 'gpt-5.6-luna',
    pi: 'gpt-5.6-luna',
    opencode: 'gpt-5.6-luna'
  },
  options: {
    piApi: 'openai-responses',
    codexWireApi: 'responses',
    codexReasoningEffort: 'high'
  }
}

const groups: GroupInfo[] = [
  {
    id: 4,
    name: 'openai',
    description: '按量计费分组',
    platform: 'openai',
    rateMultiplier: 0.3,
    allowMessagesDispatch: false,
    subscriptionType: 'standard'
  }
]

function makeApi(): RendererApi {
  return {
    getState: vi.fn().mockResolvedValue(state),
    refresh: vi.fn().mockResolvedValue(state),
    testRelay: vi.fn(),
    saveKey: vi.fn(),
    clearKey: vi.fn(),
    setAgentModel: vi.fn().mockResolvedValue(state),
    updateOptions: vi.fn().mockResolvedValue(state),
    applyAgent: vi.fn(),
    applyAll: vi.fn(),
    restoreAgent: vi.fn(),
    openExternal: vi.fn(),
    openPath: vi.fn(),
    account: {
      register: vi.fn(),
      login: vi.fn(),
      logout: vi.fn(),
      groups: vi.fn().mockResolvedValue(groups),
      keys: vi.fn().mockResolvedValue([]),
      createKey: vi.fn(),
      useKey: vi.fn()
    }
  }
}

beforeEach(() => {
  ;(window as unknown as { api: RendererApi }).api = makeApi()
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('renderer smoke tests', () => {
  it('renders the key card and agent cards', async () => {
    render(<App />)
    await waitFor(() => expect(screen.getByText('连接 API')).toBeTruthy())
    expect(screen.getByText('Codex CLI')).toBeTruthy()
    expect(screen.getByText('OpenCode')).toBeTruthy()
    expect(screen.getByText('已接入 1')).toBeTruthy()
    const effort = screen.getByLabelText('Codex 思考强度') as HTMLSelectElement
    expect(effort.value).toBe('high')
    expect(screen.getByText('自动（不写入配置）')).toBeTruthy()
    expect(screen.getByText('当前 Key 所属分组未开启 /v1/messages 调度。请在平台后台开启 allow_messages_dispatch，或改用 Anthropic 分组后再接入 Claude Code。')).toBeTruthy()
  })

  it('renders the account modal with groups', async () => {
    render(
      <AccountModal
        open
        onClose={() => undefined}
        state={state}
        onStateChange={() => undefined}
        notify={() => undefined}
      />
    )
    await waitFor(() => expect(screen.getByText('选择分组')).toBeTruthy())
    expect(screen.getByText('openai')).toBeTruthy()
    expect(screen.getByText('申请新 Key')).toBeTruthy()
  })

  it('renders the settings modal', () => {
    render(
      <SettingsModal
        open
        onClose={() => undefined}
        state={state}
        onStateChange={() => undefined}
        notify={() => undefined}
      />
    )
    expect(screen.getByText('接口格式')).toBeTruthy()
    expect(screen.getByText('配置与备份')).toBeTruthy()
  })
})
