// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import App from '../src/renderer/src/App'
import { previewState, demoKeys, demoSessions } from '../src/renderer/src/preview'
import type { AppState, RendererApi, UpdateStatus } from '../src/shared/types'

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined)
}))
let state: AppState
let api: RendererApi
beforeEach(() => {
  state = structuredClone(previewState)
  api = {
    sessions: {
      list: vi.fn(async () => structuredClone(demoSessions)),
      read: vi.fn(async (id) => ({
        session: demoSessions.sessions.find((s) => s.id === id)!,
        messages: [{ role: 'user', text: '读取正文测试' }],
        resumeCommand: "codex resume 'demo-project'",
        shell: '终端',
        truncated: false,
        note: '本机只读'
      }))
    },
    updates: {
      status: vi.fn(async () => ({
        phase: 'idle' as const,
        currentVersion: '0.2.2',
        version: null,
        notes: null,
        downloaded: 0,
        total: null,
        canInstall: true,
        format: 'app',
        error: null
      })),
      check: vi.fn(async () => ({
        phase: 'current' as const,
        currentVersion: '0.2.2',
        version: null,
        notes: null,
        downloaded: 0,
        total: null,
        canInstall: true,
        format: 'app',
        error: null
      })),
      download: vi.fn(),
      install: vi.fn(),
      onStatus: vi.fn(async () => () => {})
    },
    getState: vi.fn(async () => state),
    setCliPath: vi.fn(async () => state),
    refresh: vi.fn(async () => state),
    keys: vi.fn(async () => structuredClone(demoKeys)),
    getKey: vi.fn(async () => 'test-key-not-a-real-secret'),
    copyKey: vi.fn(async () => undefined),
    prepareKey: vi.fn(async () => ({
      ok: true,
      baseUrl: state.relayBaseUrl,
      models: state.models,
      balance: null,
      rateMultiplier: 1,
      messages: state.messages,
      latencyMs: 50
    })),
    saveKey: vi.fn(),
    deleteKey: vi.fn(async () => ({ state })),
    removeLocalKey: vi.fn(),
    importAgent: vi.fn(async () => state),
    restoreAgent: vi.fn(async () => state),
    updateOptions: vi.fn(async () => state),
    openExternal: vi.fn(),
    openPath: vi.fn(),
    account: {
      register: vi.fn(async () => state),
      login: vi.fn(async () => state),
      logout: vi.fn(async () => state),
      groups: vi.fn(async () => [
        {
          id: 1,
          name: 'OpenAI',
          description: '编程模型服务',
          platform: 'openai',
          rateMultiplier: 1,
          allowMessagesDispatch: true,
          subscriptionType: 'standard'
        }
      ]),
      createKey: vi.fn(async () => ({
        info: demoKeys[0]!,
        state
      }))
    }
  }
  window.api = api
})
afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})
async function ready() {
  render(<App />)
  await screen.findByText('日常编程')
}

describe('API-first workspace', () => {
  it('offers all thirteen native agent families and imports MiniMax without fetching a key into the UI', async () => {
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    const selector = within(dialog).getByLabelText('选择智能体')
    expect(within(selector).getAllByRole('option')).toHaveLength(14)
    for (const name of [
      'Gemini CLI',
      'Claude Desktop',
      'Hermes Agent',
      'MiniMax Code',
      'Oh My Pi',
      'Antigravity CLI'
    ]) {
      expect(within(selector).getByRole('option', { name: new RegExp(name) })).toBeTruthy()
    }
    fireEvent.change(selector, { target: { value: 'mcode' } })
    await within(dialog).findByText(/默认模型由原工具管理/)
    fireEvent.change(within(dialog).getByLabelText('使用模型'), { target: { value: 'gpt-5.4' } })
    fireEvent.click(within(dialog).getByRole('button', { name: '导入到 MiniMax Code' }))
    await waitFor(() =>
      expect(api.importAgent).toHaveBeenCalledWith(
        expect.objectContaining({ agent: 'mcode', model: 'gpt-5.4', keyId: 11 })
      )
    )
    expect(api.getKey).not.toHaveBeenCalled()
    expect(api.copyKey).not.toHaveBeenCalled()
    await within(dialog).findByText('已成功导入')
    expect(within(dialog).getByText(/导入后在 MCode/)).toBeTruthy()
  })
  it('loads sessions only when opened and reads only the selected conversation', async () => {
    await ready()
    expect(api.sessions.list).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '会话' }))
    await screen.findByText('整理项目的 API 接入流程')
    expect(api.sessions.read).not.toHaveBeenCalled()
    fireEvent.change(screen.getByLabelText('搜索会话'), { target: { value: '错误提示' } })
    expect(screen.queryByText('整理项目的 API 接入流程')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /检查模型调用与错误提示/ }))
    await screen.findByText('读取正文测试')
    expect(api.sessions.read).toHaveBeenCalledWith('demo-claude')
    expect(api.importAgent).not.toHaveBeenCalled()
    expect(api.getKey).not.toHaveBeenCalled()
  })
  it('shows a session read error without keeping stale conversation content', async () => {
    vi.mocked(api.sessions.read).mockRejectedValue(new Error('会话已移动或删除'))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '会话' }))
    await screen.findByText('整理项目的 API 接入流程')
    fireEvent.click(screen.getByRole('button', { name: /整理项目的 API 接入流程/ }))
    await screen.findByRole('alert')
    expect(screen.getByText('会话已移动或删除')).toBeTruthy()
    expect(screen.queryByRole('button', { name: '复制继续命令' })).toBeNull()
  })
  it('searches native adapters and keeps guided setup separate from automatic writes', async () => {
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    fireEvent.change(within(dialog).getByLabelText('搜索智能体'), { target: { value: 'Kimi' } })
    expect(within(dialog).getByRole('option', { name: /Kimi Code/ })).toBeTruthy()
    fireEvent.change(within(dialog).getByLabelText('选择智能体'), { target: { value: 'kimi' } })
    await within(dialog).findByRole('button', { name: '导入到 Kimi Code' })
    expect(api.importAgent).not.toHaveBeenCalled()
    fireEvent.change(within(dialog).getByLabelText('搜索智能体'), { target: { value: 'Cline' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cline' }))
    await within(dialog).findByText('手动配置')
    expect(within(dialog).queryByRole('button', { name: '选择智能体后导入' })).toBeNull()
    expect(api.importAgent).not.toHaveBeenCalled()
    expect(api.getKey).not.toHaveBeenCalled()
    expect(api.copyKey).not.toHaveBeenCalled()
  })
  it('shows keys without listing all agents and never fetches a secret on startup', async () => {
    await ready()
    expect(screen.getByText('轻量任务')).toBeTruthy()
    expect(screen.queryByText('Claude Code')).toBeNull()
    expect(screen.queryByText('OpenCode')).toBeNull()
    expect(api.getKey).not.toHaveBeenCalled()
    expect(api.importAgent).not.toHaveBeenCalled()
  })
  it('opens an import panel and only writes after explicit submission', async () => {
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    await within(dialog).findByRole('button', { name: '导入到 Codex CLI' })
    await waitFor(() =>
      expect((within(dialog).getByLabelText('使用模型') as HTMLSelectElement).value).toBe('gpt-5.4')
    )
    expect(api.prepareKey).toHaveBeenCalledWith(11)
    expect(api.importAgent).not.toHaveBeenCalled()
    fireEvent.click(within(dialog).getByRole('button', { name: '导入到 Codex CLI' }))
    await within(dialog).findByText('已成功导入')
    expect(api.importAgent).toHaveBeenCalledWith(
      expect.objectContaining({ keyId: 11, agent: 'codex', model: 'gpt-5.4', effort: 'medium' })
    )
  })
  it('lets a non-Anthropic key use Claude through local compatibility', async () => {
    state.messages = 'unsupported'
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    fireEvent.change(within(dialog).getByLabelText('选择智能体'), { target: { value: 'claude' } })
    await within(dialog).findByText('通过本地兼容连接使用 Claude Code。')
    fireEvent.change(within(dialog).getByLabelText('使用模型'), { target: { value: 'gpt-5.4' } })
    expect(
      (within(dialog).getByRole('button', { name: '导入到 Claude Code' }) as HTMLButtonElement)
        .disabled
    ).toBe(false)
    fireEvent.click(within(dialog).getByRole('button', { name: '导入到 Claude Code' }))
    await waitFor(() =>
      expect(api.importAgent).toHaveBeenCalledWith(
        expect.objectContaining({ agent: 'claude', keyId: 11 })
      )
    )
  })
  it('does not retrieve full keys until the user asks and hides them when closed', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '查看 日常编程' }))
    const dialog = await screen.findByRole('dialog')
    expect(api.getKey).not.toHaveBeenCalled()
    fireEvent.click(within(dialog).getByRole('button', { name: '查看完整 Key' }))
    await within(dialog).findByText('test-key-not-a-real-secret')
    fireEvent.click(within(dialog).getByRole('button', { name: '关闭' }))
    expect(screen.queryByText('test-key-not-a-real-secret')).toBeNull()
  })
  it('creates once and continues directly to import even when storage returns a warning', async () => {
    vi.mocked(api.account.createKey).mockResolvedValue({
      info: demoKeys[0]!,
      state,
      warning: '系统存储暂不可用'
    })
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '创建 API Key' }))
    const dialog = await screen.findByRole('dialog')
    await waitFor(() =>
      expect((within(dialog).getByLabelText('模型服务') as HTMLSelectElement).value).toBe('1')
    )
    fireEvent.click(within(dialog).getByRole('button', { name: '创建 API Key' }))
    await screen.findByText('API Key 已创建')
    expect(api.account.createKey).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByRole('button', { name: '导入到智能体' }))
    await screen.findByRole('button', { name: '导入到 Codex CLI' })
  })
  it('shows only previously configured agents in the connections view', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '已接入 1' }))
    expect(screen.getByText('Codex CLI')).toBeTruthy()
    expect(screen.queryByText('Claude Code')).toBeNull()
    expect(screen.queryByText('Pi')).toBeNull()
  })
  it('shows a recharge placeholder without payment inputs', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: /充值/ }))
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('充值功能正在准备中')).toBeTruthy()
    expect(within(dialog).queryByRole('textbox')).toBeNull()
    expect(within(dialog).queryByRole('button', { name: '支付' })).toBeNull()
  })
  it('lets guests add an existing key without signing up', async () => {
    state.account = { loggedIn: false }
    vi.mocked(api.keys).mockResolvedValue([])
    render(<App />)
    await screen.findByText('从你的第一个 API Key 开始')
    fireEvent.click(screen.getByRole('button', { name: '我已有 API Key，直接添加' }))
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByLabelText('API Key')).toBeTruthy()
    expect(api.account.login).not.toHaveBeenCalled()
  })
  it('deletes only after explicit confirmation and leaves agent configuration alone', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '删除 日常编程' }))
    let dialog = await screen.findByRole('dialog')
    expect(api.deleteKey).not.toHaveBeenCalled()
    expect(within(dialog).getByText(/Codex CLI/)).toBeTruthy()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
    expect(api.deleteKey).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '删除 日常编程' }))
    dialog = await screen.findByRole('dialog')
    vi.mocked(api.keys).mockResolvedValue(demoKeys.filter((key) => key.id !== 11))
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }))
    await waitFor(() => expect(api.deleteKey).toHaveBeenCalledWith(11))
    await waitFor(() => expect(screen.queryByRole('button', { name: '删除 日常编程' })).toBeNull())
    expect(api.restoreAgent).not.toHaveBeenCalled()
    expect(api.importAgent).not.toHaveBeenCalled()
  })
  it('keeps the key and the confirmation dialog when platform deletion fails', async () => {
    vi.mocked(api.deleteKey).mockRejectedValue(new Error('服务暂时不可用'))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '删除 日常编程' }))
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }))
    await within(dialog).findByText('服务暂时不可用')
    expect(screen.getByRole('button', { name: '删除 日常编程' })).toBeTruthy()
    expect(api.deleteKey).toHaveBeenCalledTimes(1)
  })
})

describe('installation and verification regressions', () => {
  it('rescans on opening import and uses a manual CLI path', async () => {
    state.agents.find((a) => a.id === 'codex')!.installed = false
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    fireEvent.change(within(dialog).getByLabelText('选择智能体'), { target: { value: 'codex' } })
    await within(dialog).findByText('尚未定位到 Codex CLI')
    expect(api.getState).toHaveBeenCalledTimes(2)
    fireEvent.click(within(dialog).getByText('高级设置'))
    fireEvent.change(within(dialog).getByLabelText('CLI 文件路径（可选）'), {
      target: { value: '/home/test/.opencode/bin/codex' }
    })
    fireEvent.click(within(dialog).getByRole('button', { name: '使用此路径' }))
    await waitFor(() =>
      expect(api.setCliPath).toHaveBeenCalledWith('codex', '/home/test/.opencode/bin/codex')
    )
  })
  it('never claims successful import when model validation returns 503', async () => {
    vi.mocked(api.importAgent).mockRejectedValue(
      new Error('模型验证失败，原配置未修改。HTTP 503；请求 ID：test-503')
    )
    await ready()
    fireEvent.click(screen.getAllByRole('button', { name: '导入' })[0]!)
    const dialog = await screen.findByRole('dialog')
    const submit = await within(dialog).findByRole('button', { name: '导入到 Codex CLI' })
    await waitFor(() => expect((submit as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(submit)
    await within(dialog).findByRole('alert')
    expect(within(dialog).getByText(/请求 ID：test-503/)).toBeTruthy()
    expect(within(dialog).queryByText('已成功导入')).toBeNull()
    expect((submit as HTMLButtonElement).disabled).toBe(false)
  })
})

const updateFixture = (changes: Partial<UpdateStatus> = {}): UpdateStatus => ({
  phase: 'available',
  currentVersion: '0.2.2',
  version: '0.2.3',
  notes: '修复与改进',
  downloaded: 0,
  total: null,
  canInstall: true,
  format: 'app',
  error: null,
  ...changes
})
describe('signed application updates', () => {
  it('notifies without downloading or interrupting the API workspace', async () => {
    vi.mocked(api.updates.status).mockResolvedValue(updateFixture())
    await ready()
    await screen.findByText('新版本 v0.2.3 已发布')
    expect(api.updates.download).not.toHaveBeenCalled()
    expect(api.updates.install).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '查看更新' }))
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByRole('region', { name: '软件更新' })).toBeTruthy()
  })
  it('shows progress and requires confirmation before installing a verified update', async () => {
    let callback: ((status: UpdateStatus) => void) | undefined
    let finish: ((status: UpdateStatus) => void) | undefined
    vi.mocked(api.updates.status).mockResolvedValue(updateFixture())
    vi.mocked(api.updates.onStatus).mockImplementation(async (listener) => {
      callback = listener
      return () => {}
    })
    vi.mocked(api.updates.download).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        })
    )
    vi.mocked(api.updates.install).mockResolvedValue(updateFixture({ phase: 'installing' }))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '设置' }))
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(await within(dialog).findByRole('button', { name: '下载更新' }))
    act(() => callback?.(updateFixture({ phase: 'downloading', downloaded: 50, total: 100 })))
    expect((within(dialog).getByRole('progressbar') as HTMLProgressElement).value).toBe(50)
    expect(api.updates.install).not.toHaveBeenCalled()
    await act(async () => finish?.(updateFixture({ phase: 'ready', downloaded: 100, total: 100 })))
    const install = within(dialog).getByRole('button', {
      name: '安装并重启'
    }) as HTMLButtonElement
    expect(install.disabled).toBe(true)
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /我已准备好重启应用/ }))
    fireEvent.click(install)
    fireEvent.click(install)
    await waitFor(() => expect(api.updates.install).toHaveBeenCalledExactlyOnceWith(true))
    expect(api.importAgent).not.toHaveBeenCalled()
  })
  it('reports a failed check and keeps existing keys available', async () => {
    vi.mocked(api.updates.check).mockRejectedValue(new Error('更新网络连接失败'))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '设置' }))
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(within(dialog).getByRole('button', { name: '检查更新' }))
    await within(dialog).findByText('更新网络连接失败')
    expect(screen.getByText('日常编程')).toBeTruthy()
    expect(api.updates.install).not.toHaveBeenCalled()
  })
  it('portable builds offer the download page and never run an installer', async () => {
    vi.mocked(api.updates.status).mockResolvedValue(
      updateFixture({
        canInstall: false,
        format: 'portable',
        downloadUrl:
          'https://github.com/simonyuann/aihubmix-switch/releases/download/v0.2.3/AIHubMix-Switch_0.2.3_windows-x64-portable.exe'
      })
    )
    await ready()
    fireEvent.click(screen.getByRole('button', { name: '设置' }))
    const dialog = await screen.findByRole('dialog')
    fireEvent.click(await within(dialog).findByRole('button', { name: '下载覆盖版本' }))
    await waitFor(() =>
      expect(api.openExternal).toHaveBeenCalledWith(
        'https://github.com/simonyuann/aihubmix-switch/releases/download/v0.2.3/AIHubMix-Switch_0.2.3_windows-x64-portable.exe'
      )
    )
    expect(api.updates.download).not.toHaveBeenCalled()
    expect(api.updates.install).not.toHaveBeenCalled()
  })
})
