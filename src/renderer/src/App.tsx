import { useEffect, useRef, useState } from 'react'
import type { AgentId, AppState, RelayProbe } from '../../shared/types'
import { AGENTS, AGENT_ORDER, CODEX_EFFORT_LABELS } from '../../shared/constants'
import { KeyCard } from './components/KeyCard'
import { AgentCard } from './components/AgentCard'
import { AccountModal } from './components/AccountModal'
import { SettingsModal } from './components/SettingsModal'
import { Toast, type ToastKind, type ToastState } from './components/Toast'
import { FolderIcon, RefreshIcon, SettingsIcon, SparkIcon, UserIcon } from './components/Icons'

export default function App() {
  const [state, setState] = useState<AppState | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)
  const [keyInput, setKeyInput] = useState('')
  const [showKey, setShowKey] = useState(false)
  const [probe, setProbe] = useState<RelayProbe | null>(null)
  const [accountOpen, setAccountOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [toast, setToast] = useState<ToastState | null>(null)
  const toastId = useRef(0)

  const notify = (message: string, kind: ToastKind = 'success') => {
    toastId.current += 1
    setToast({ id: toastId.current, kind, message })
  }

  useEffect(() => {
    if (!toast) return
    const timer = setTimeout(() => setToast(null), toast.kind === 'error' ? 6500 : 3800)
    return () => clearTimeout(timer)
  }, [toast])

  useEffect(() => {
    let cancelled = false
    const bootstrap = async () => {
      try {
        const initial = await window.api.getState()
        if (cancelled) return
        setState(initial)
        setLoading(false)
        const refreshed = await window.api.refresh()
        if (!cancelled) setState(refreshed)
      } catch (error) {
        if (cancelled) return
        setLoading(false)
        notify(error instanceof Error ? error.message : String(error), 'error')
      }
    }
    void bootstrap()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  async function reload() {
    const next = await window.api.getState()
    setState(next)
    return next
  }

  async function handleTest() {
    if (!state) return
    setBusy('test')
    setProbe(null)
    try {
      if (keyInput.trim()) {
        const result = await window.api.testRelay({
          baseUrl: state.relayBaseUrl,
          apiKey: keyInput.trim()
        })
        setProbe(result)
        if (result.ok) {
          notify(
            result.messages === 'unsupported'
              ? `连接成功，共 ${result.models.length} 个模型；当前分组未开启 Claude Code`
              : `连接成功，共 ${result.models.length} 个模型`,
            'success'
          )
        } else {
          notify(result.error || '连接失败', 'error')
        }
      } else {
        const next = await window.api.refresh()
        setState(next)
        if (next.relayError) notify(next.relayError, 'error')
        else notify(`连接正常，共 ${next.models.length} 个模型`)
      }
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleSave() {
    if (!state) return
    setBusy('save')
    try {
      const result = await window.api.saveKey({
        baseUrl: state.relayBaseUrl,
        apiKey: keyInput.trim()
      })
      setState(result.state)
      setProbe(result.probe)
      setKeyInput('')
      notify(
        result.probe.messages === 'unsupported'
          ? 'API Key 已保存；当前分组未开启 Claude Code 调度'
          : 'API Key 已保存'
      )
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleRefresh() {
    setBusy('refresh')
    try {
      const next = await window.api.refresh(true)
      setState(next)
      if (next.relayError) notify(next.relayError, 'error')
      else notify('状态已刷新')
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleClear() {
    if (!window.confirm('确定清除本机保存的 API Key？已写入各 Agent 的配置不会自动还原。')) return
    setBusy('clear')
    try {
      const next = await window.api.clearKey()
      setState(next)
      setProbe(null)
      notify('已清除本机保存的 API Key')
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleApply(agent: AgentId) {
    if (!state) return
    const model = state.agentModels[agent]
    if (!model) {
      notify('请先保存 API Key 并选择模型', 'error')
      return
    }
    setBusy(`apply:${agent}`)
    try {
      const result = await window.api.applyAgent({ agent, model })
      await reload()
      notify(result.message)
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleRestore(agent: AgentId) {
    const meta = AGENTS[agent]
    if (!window.confirm(`确定还原 ${meta.name} 的配置？将恢复到最近一次接入前的状态。`)) return
    setBusy(`restore:${agent}`)
    try {
      const result = await window.api.restoreAgent(agent)
      await reload()
      notify(result.message)
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleApplyAll() {
    if (!state?.hasApiKey) {
      notify('请先保存 API Key', 'error')
      return
    }
    setBusy('apply-all')
    try {
      const result = await window.api.applyAll()
      await reload()
      if (result.errors.length) {
        notify(
          `已接入 ${result.results.length} 个，失败 ${result.errors.length} 个：${result.errors[0]?.message}`,
          'error'
        )
      } else {
        notify(`已接入 ${result.results.length} 个 Agent`)
      }
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleRestoreAll() {
    if (!window.confirm('确定还原全部 Agent 的配置？')) return
    setBusy('restore-all')
    try {
      for (const agent of AGENT_ORDER) {
        await window.api.restoreAgent(agent)
      }
      await reload()
      notify('全部 Agent 已还原')
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function handleCodexEffortChange(effort: string) {
    if (!state) return
    try {
      const next = await window.api.updateOptions({ codexReasoningEffort: effort })
      setState(next)
      const label = CODEX_EFFORT_LABELS[effort] || effort
      const codex = next.agents.find((agent) => agent.id === 'codex')
      if (codex?.connected && next.agentModels.codex) {
        setBusy('apply:codex')
        try {
          await window.api.applyAgent({ agent: 'codex', model: next.agentModels.codex })
          await reload()
          notify(`Codex 思考强度已更新：${label}`)
        } catch (error) {
          notify(error instanceof Error ? error.message : String(error), 'error')
        } finally {
          setBusy(null)
        }
      } else {
        notify(`已保存思考强度：${label}，点击「接入」后生效`)
      }
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    }
  }

  async function handleModelChange(agent: AgentId, model: string) {
    if (!state) return
    setState({
      ...state,
      agentModels: { ...state.agentModels, [agent]: model }
    })
    try {
      const next = await window.api.setAgentModel(agent, model)
      setState(next)
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
      await reload()
    }
  }

  async function copyInstall(agent: AgentId) {
    try {
      await navigator.clipboard.writeText(AGENTS[agent].installCommand)
      notify('安装命令已复制')
    } catch {
      notify('复制失败，请手动复制', 'error')
    }
  }

  async function openBackups() {
    try {
      await window.api.openPath('backups')
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), 'error')
    }
  }

  if (loading || !state) {
    return (
      <div className="app loading-app">
        <div className="loading-box">
          <div className="brand-logo large">
            <SparkIcon size={26} />
          </div>
          <h1>AIHubMix 接入助手</h1>
          <p>
            <RefreshIcon size={14} className="spin" />
            正在检测本机 Agent…
          </p>
        </div>
      </div>
    )
  }

  const installed = state.agents.filter((agent) => agent.installed).length
  const connected = state.agents.filter((agent) => agent.connected).length
  const codexModel = state.models.find((model) => model.id === state.agentModels.codex)
  const codexEfforts = codexModel?.reasoningLevels?.length
    ? codexModel.reasoningLevels
    : ['low', 'medium', 'high', 'xhigh']

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <div className="brand-logo">
            <SparkIcon size={21} />
          </div>
          <div className="brand-text">
            <h1>AIHubMix 接入助手</h1>
            <p>{state.site.subtitle || '一个 Key，接入所有编程 Agent'}</p>
          </div>
        </div>
        <div className="top-actions">
          <button
            className="btn ghost sm"
            onClick={() => void window.api.openExternal(state.site.apiBaseUrl)}
          >
            官网
          </button>
          <button className="btn ghost sm" onClick={() => setAccountOpen(true)}>
            <UserIcon size={14} />
            {state.account.loggedIn ? state.account.email?.split('@')[0] || '账户' : '账户'}
          </button>
          <button className="icon-btn" onClick={() => setSettingsOpen(true)} aria-label="设置">
            <SettingsIcon size={18} />
          </button>
        </div>
      </header>

      <main className="main">
        <KeyCard
          state={state}
          keyInput={keyInput}
          setKeyInput={setKeyInput}
          showKey={showKey}
          setShowKey={setShowKey}
          probe={probe}
          busy={busy}
          onSave={handleSave}
          onTest={handleTest}
          onClear={handleClear}
          onOpenAccount={() => setAccountOpen(true)}
        />

        <section className="agents-section">
          <div className="section-head">
            <div className="section-title">
              <span className="step">2</span>
              <div>
                <h2>接入 Agent</h2>
                <p>选择模型后点击接入，配置会自动备份，可随时还原</p>
              </div>
            </div>
            <div className="agent-stats">
              <span className="chip">已安装 {installed}/4</span>
              <span className={`chip ${connected ? 'ok' : ''}`}>已接入 {connected}</span>
              <button
                className="icon-btn"
                onClick={handleRefresh}
                disabled={Boolean(busy)}
                aria-label="刷新状态"
                title="重新检测 Agent 与连接状态"
              >
                <RefreshIcon size={16} className={busy === 'refresh' ? 'spin' : ''} />
              </button>
            </div>
          </div>

          <div className="agent-grid">
            {AGENT_ORDER.map((agent) => {
              const agentState = state.agents.find((item) => item.id === agent)
              if (!agentState) return null
              return (
                <AgentCard
                  key={agent}
                  meta={AGENTS[agent]}
                  state={agentState}
                  models={state.models}
                  model={state.agentModels[agent] || ''}
                  messages={state.messages}
                  busy={busy}
                  effort={state.options.codexReasoningEffort}
                  appliedEffort={state.agents.find((item) => item.id === 'codex')?.appliedEffort}
                  efforts={codexEfforts}
                  onEffortChange={handleCodexEffortChange}
                  onApply={() => handleApply(agent)}
                  onRestore={() => handleRestore(agent)}
                  onModelChange={(model) => handleModelChange(agent, model)}
                  onOpenConfig={() => {
                    void window.api.openPath(agent).catch((error) => {
                      notify(error instanceof Error ? error.message : String(error), 'error')
                    })
                  }}
                  onCopyInstall={() => copyInstall(agent)}
                />
              )
            })}
          </div>
        </section>
      </main>

      <footer className="bottombar">
        <div className="bottom-actions">
          <button className="btn primary" onClick={handleApplyAll} disabled={Boolean(busy)}>
            全部接入
          </button>
          <button className="btn" onClick={handleRestoreAll} disabled={Boolean(busy)}>
            全部还原
          </button>
          <button className="btn" onClick={openBackups} disabled={Boolean(busy)}>
            <FolderIcon size={14} />
            备份目录
          </button>
        </div>
        <div className="bottom-meta">
          <span>配置只写入本机 · v{state.version}</span>
        </div>
      </footer>

      <AccountModal
        open={accountOpen}
        onClose={() => setAccountOpen(false)}
        state={state}
        onStateChange={setState}
        notify={notify}
      />
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        state={state}
        onStateChange={setState}
        notify={notify}
      />
      <Toast toast={toast} />
    </div>
  )
}
