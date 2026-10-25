import { useEffect, useState } from 'react'
import type { AgentId, ApiKeyInfo, AppState, RelayProbe } from '../../../shared/types'
import { AGENTS, CODEX_EFFORT_LABELS } from '../../../shared/constants'
import { isSelectableModel } from '../../../shared/url'
import { Modal } from './Modal'
import { CheckIcon, CopyIcon, KeyIcon, LoaderIcon, ShieldIcon, TerminalIcon } from './Icons'

const order: AgentId[] = ['codex', 'claude', 'opencode', 'pi']

export function ImportModal({
  item,
  state,
  onClose,
  onImported,
  onCopyCommand
}: {
  item: ApiKeyInfo
  state: AppState
  onClose: () => void
  onImported: (state: AppState) => void
  onCopyCommand: (command: string) => Promise<void>
}) {
  const installed = state.agents.filter((agent) => agent.installed)
  const initial =
    state.lastAgent && order.includes(state.lastAgent as AgentId)
      ? (state.lastAgent as AgentId)
      : installed.length === 1
        ? installed[0]?.id || ''
        : ''
  const [agent, setAgent] = useState<AgentId | ''>(initial)
  const [model, setModel] = useState('')
  const [effort, setEffort] = useState(state.options.codexReasoningEffort)
  const [probe, setProbe] = useState<RelayProbe | null>(null)
  const [testing, setTesting] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState(false)
  const [retry, setRetry] = useState(0)

  useEffect(() => {
    let cancelled = false
    setTesting(true)
    setError('')
    setProbe(null)
    void window.api
      .prepareKey(item.id)
      .then((result) => {
        if (!cancelled) {
          setProbe(result)
          if (!result.ok) setError(result.error || '连接验证失败')
        }
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err))
      })
      .finally(() => {
        if (!cancelled) setTesting(false)
      })
    return () => {
      cancelled = true
    }
  }, [item.id, retry])

  const models = probe?.models.filter((m) => isSelectableModel(m.id)) || []
  useEffect(() => {
    const options = probe?.models.filter((m) => isSelectableModel(m.id)) || []
    const saved = agent ? state.agentModels[agent] : undefined
    setModel(
      saved && options.some((m) => m.id === saved)
        ? saved
        : options.length === 1
          ? options[0]?.id || ''
          : ''
    )
  }, [agent, probe, state.agentModels])
  const meta = agent ? AGENTS[agent] : null
  const agentState = state.agents.find((value) => value.id === agent)
  const compatibility =
    agent === 'claude' &&
    (probe?.messages !== 'supported' || !model.toLowerCase().startsWith('claude'))
  const compatibilityBlocked = compatibility && state.compatibilityReady === false
  const selectedModel = models.find((m) => m.id === model)
  const efforts = selectedModel?.reasoningLevels?.length
    ? selectedModel.reasoningLevels
    : ['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra']
  const actualEffort =
    selectedModel?.reasoning === false || (effort !== 'auto' && !efforts.includes(effort))
      ? 'auto'
      : effort

  async function apply() {
    if (!agent || !model || compatibilityBlocked || !probe?.ok) return
    setBusy(true)
    setError('')
    try {
      const next = await window.api.importAgent({
        keyId: item.id,
        keyLabel: `${item.name} · ${item.keyMasked}`,
        agent,
        model,
        effort: agent === 'codex' ? actualEffort : undefined
      })
      onImported(next)
      setSuccess(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Modal
      open
      title={success ? '已成功导入' : '导入到智能体'}
      subtitle={success ? '连接已准备好，可以开始编程了。' : '选择一个工具，我们帮你完成配置。'}
      onClose={onClose}
      width={500}
      busy={busy}
    >
      {success ? (
        <>
          <div className="success-block">
            <div className="success-symbol">
              <CheckIcon size={26} />
            </div>
            <h3>{meta?.name}</h3>
            <p>{model}</p>
          </div>
          <div className="notice notice-ok">
            <ShieldIcon size={16} />
            <span>已自动备份原配置，可在「已接入」中随时还原。</span>
          </div>
          {agentState?.compatibility && (
            <div className="notice notice-ok">
              本地兼容连接已启用。关闭窗口后会保留托盘运行，使用 Claude Code 时请勿完全退出应用。
            </div>
          )}
          <p className="fine-print centered">
            {agentState?.installed
              ? `请重新打开 ${meta?.name}，让新配置生效。`
              : `配置已就绪，安装 ${meta?.name} 后即可使用。`}
          </p>
          <button className="btn primary block" onClick={onClose}>
            完成
          </button>
          <button
            className="btn ghost block"
            onClick={() => {
              setSuccess(false)
              setAgent('')
              setModel('')
            }}
          >
            继续导入其他智能体
          </button>
        </>
      ) : (
        <>
          <div className="selected-key">
            <span className="mini-icon">
              <KeyIcon size={17} />
            </span>
            <div>
              <strong>{item.name}</strong>
              <code>{item.keyMasked}</code>
            </div>
            <span className="badge">本次使用</span>
          </div>
          <div className="form-stack">
            <label className="field">
              <span>选择智能体</span>
              <select
                value={agent}
                disabled={busy}
                onChange={(event) => setAgent(event.target.value as AgentId | '')}
              >
                <option value="">选择你使用的编程工具</option>
                {order.map((id) => (
                  <option key={id} value={id}>
                    {AGENTS[id].name}
                    {state.agents.find((a) => a.id === id)?.installed
                      ? ' · 已安装'
                      : ' · 未检测到安装'}
                  </option>
                ))}
              </select>
            </label>
            {testing && (
              <div className="probe-status" role="status">
                <LoaderIcon size={15} />
                正在验证 Key 并获取可用模型…
              </div>
            )}
            {agent && !testing && probe?.ok && (
              <>
                <label className="field">
                  <span>使用模型</span>
                  <select
                    value={model}
                    disabled={busy || !models.length}
                    required
                    onChange={(event) => setModel(event.target.value)}
                  >
                    <option value="">
                      {models.length ? '选择一个模型' : '暂无可用于编程的模型'}
                    </option>
                    {models.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name === item.id ? item.id : `${item.name} · ${item.id}`}
                      </option>
                    ))}
                  </select>
                </label>
                {compatibility && (
                  <div className="notice notice-ok">
                    <ShieldIcon size={16} />
                    <div>
                      通过本地兼容连接使用 Claude Code。
                      <p>
                        使用当前 Key
                        的模型，支持对话和工具调用。关闭窗口后会在托盘继续运行；完全退出应用会停止连接。
                      </p>
                    </div>
                  </div>
                )}
                {compatibilityBlocked && (
                  <div className="notice notice-error">
                    {state.compatibilityError || '本地兼容服务未就绪，请重启应用。'}
                  </div>
                )}
                {!agentState?.installed && (
                  <div className="install-hint">
                    <TerminalIcon size={17} />
                    <div>
                      <strong>未检测到 {meta?.name}</strong>
                      <p>可以先导入配置，安装后直接使用。</p>
                    </div>
                    <button
                      className="icon-btn"
                      title="复制安装命令"
                      aria-label="复制安装命令"
                      onClick={() => {
                        if (meta) void onCopyCommand(meta.installCommand)
                      }}
                    >
                      <CopyIcon size={15} />
                    </button>
                  </div>
                )}
                <details className="advanced">
                  <summary>高级设置</summary>
                  {agent === 'codex' && (
                    <label className="field">
                      <span>思考强度</span>
                      <select
                        value={actualEffort}
                        aria-label="Codex 思考强度"
                        disabled={busy || selectedModel?.reasoning === false}
                        onChange={(event) => setEffort(event.target.value)}
                      >
                        <option value="auto">自动（交给 Codex 决定）</option>
                        {efforts.map((value) => (
                          <option key={value} value={value}>
                            {CODEX_EFFORT_LABELS[value] || value}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  <div className="advanced-path">
                    <span>配置位置</span>
                    <code>{agentState?.configPath || meta?.configHint}</code>
                  </div>
                  <p className="fine-print">配置前会自动备份。接口兼容格式可在「设置」中调整。</p>
                </details>
              </>
            )}
            {error && (
              <div className="notice notice-error" role="alert">
                <div>
                  {error}
                  {!probe?.ok && (
                    <button
                      className="link-btn"
                      disabled={busy || testing}
                      onClick={() => setRetry((v) => v + 1)}
                    >
                      重新检测
                    </button>
                  )}
                </div>
              </div>
            )}
            <button
              className="btn primary block"
              onClick={() => void apply()}
              disabled={busy || testing || !probe?.ok || !agent || !model || compatibilityBlocked}
            >
              {busy ? <LoaderIcon size={16} /> : <CheckIcon size={16} />}
              {busy ? '正在备份并导入…' : meta ? `导入到 ${meta.name}` : '选择智能体后导入'}
            </button>
            <p className="fine-print centered">仅修改所选智能体，其他工具的配置保持原样。</p>
          </div>
        </>
      )}
    </Modal>
  )
}
