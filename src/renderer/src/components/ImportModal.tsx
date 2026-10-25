import { useEffect, useState } from 'react'
import type { AgentId, ApiKeyInfo, AppState, RelayProbe } from '../../../shared/types'
import { AGENTS, AGENT_ORDER, CODEX_EFFORT_LABELS } from '../../../shared/constants'
import { GuidedAgents } from './GuidedAgents'
import { isSelectableModel } from '../../../shared/url'
import { Modal } from './Modal'
import { CheckIcon, CopyIcon, KeyIcon, LoaderIcon, ShieldIcon, TerminalIcon } from './Icons'

const order = AGENT_ORDER

export function ImportModal({
  item,
  initialAgent,
  state,
  onClose,
  onImported,
  onCopyCommand
}: {
  item: ApiKeyInfo
  initialAgent?: AgentId
  state: AppState
  onClose: () => void
  onImported: (state: AppState) => void
  onCopyCommand: (command: string) => Promise<void>
}) {
  const installed = state.agents.filter((agent) => agent.installed)
  const initial =
    initialAgent ||
    (state.lastAgent && order.includes(state.lastAgent as AgentId)
      ? (state.lastAgent as AgentId)
      : installed.length === 1
        ? installed[0]?.id || ''
        : '')
  const [agent, setAgent] = useState<AgentId | ''>(initial)
  const [search, setSearch] = useState('')
  const [manual, setManual] = useState(false)
  const [model, setModel] = useState('')
  const [effort, setEffort] = useState(state.options.codexReasoningEffort)
  const [agyName, setAgyName] = useState(state.options.agyModelName || '')
  const [probe, setProbe] = useState<RelayProbe | null>(null)
  const [testing, setTesting] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState(false)
  const [retry, setRetry] = useState(0)
  const [detectedAgents, setDetectedAgents] = useState(state.agents)
  const [scanning, setScanning] = useState(false)
  const [cliPath, setCliPath] = useState('')
  const [detectionError, setDetectionError] = useState('')
  useEffect(() => {
    let cancelled = false
    void window.api
      .getState()
      .then((next) => {
        if (!cancelled) setDetectedAgents(next.agents)
      })
      .catch(() => {
        if (!cancelled) setDetectionError('扫描失败，请重新扫描或手动填写路径')
      })
    return () => {
      cancelled = true
    }
  }, [])
  useEffect(() => {
    setCliPath(detectedAgents.find((a) => a.id === agent)?.cliPath || '')
    setDetectionError('')
  }, [agent, detectedAgents])
  async function scan(usePath = false) {
    if (!agent) return
    setScanning(true)
    setDetectionError('')
    try {
      const next = usePath
        ? await window.api.setCliPath(agent, cliPath.trim())
        : await window.api.getState()
      setDetectedAgents(next.agents)
    } catch (err) {
      setDetectionError(err instanceof Error ? err.message : String(err))
    } finally {
      setScanning(false)
    }
  }

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
  const agentState = detectedAgents.find((value) => value.id === agent)
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
        effort: agent === 'codex' ? actualEffort : undefined,
        agyModelName: agent === 'agy' ? agyName.trim() : undefined
      })
      setDetectedAgents(next.agents)
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
      subtitle={success ? '模型调用验证通过，配置已写入。' : '选择一个工具，我们帮你完成配置。'}
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
            {agent === 'agy'
              ? '请通过下方专用启动命令运行，直接输入 agy 不会使用本次接入。'
              : agentState?.installed
                ? `请重新打开 ${meta?.name}，让新配置生效。`
                : `配置已写入；请确认 ${meta?.name} 的实际运行环境及配置路径。`}
          </p>
          {meta?.setupNote && <p className="fine-print">{meta.setupNote}</p>}
          {agentState?.launchCommand && (
            <button
              className="btn block"
              onClick={() => void onCopyCommand(agentState.launchCommand!)}
            >
              <TerminalIcon size={15} />
              {state.platform === 'windows' ? '复制 PowerShell 启动命令' : '复制专用启动命令'}
            </button>
          )}
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
            <input
              className="catalog-search"
              aria-label="搜索智能体"
              placeholder="搜索你的智能体或编辑器…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              disabled={busy}
            />
            <label className="field">
              <span>选择智能体</span>
              <select
                value={agent}
                disabled={busy}
                onChange={(event) => {
                  setAgent(event.target.value as AgentId | '')
                  setManual(false)
                }}
              >
                <option value="">选择你使用的编程工具</option>
                {[...order]
                  .sort(
                    (a, b) =>
                      Number(b === state.lastAgent) - Number(a === state.lastAgent) ||
                      Number(Boolean(detectedAgents.find((v) => v.id === b)?.installed)) -
                        Number(Boolean(detectedAgents.find((v) => v.id === a)?.installed))
                  )
                  .filter(
                    (id) =>
                      id === agent ||
                      `${AGENTS[id].name} ${AGENTS[id].subtitle}`
                        .toLowerCase()
                        .includes(search.toLowerCase())
                  )
                  .map((id) => (
                    <option key={id} value={id}>
                      {AGENTS[id].name} · 自动配置
                      {detectedAgents.find((a) => a.id === id)?.installed
                        ? AGENTS[id].desktop
                          ? ' · 已定位 App'
                          : ' · 已定位 CLI'
                        : AGENTS[id].desktop
                          ? ' · 未定位 App'
                          : ' · 未定位 CLI'}
                    </option>
                  ))}
              </select>
            </label>
            <GuidedAgents
              query={search}
              baseUrl={probe?.baseUrl || state.relayBaseUrl}
              keyId={item.id}
              onCopy={onCopyCommand}
              onSelect={() => {
                setAgent('')
                setManual(true)
              }}
              nativeAgent={agent}
              models={models.map((m) => m.id)}
            />
            {testing && (
              <div className="probe-status" role="status">
                <LoaderIcon size={15} />
                正在验证 Key 并获取可用模型…
              </div>
            )}
            {agent && !testing && probe?.ok && (
              <>
                {meta?.setupNote && (
                  <div className="notice compact">
                    <span>{meta.setupNote}</span>
                  </div>
                )}
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
                {agent === 'agy' && (
                  <label className="field">
                    <span>Antigravity 模型名称</span>
                    <input
                      value={agyName}
                      onChange={(e) => setAgyName(e.target.value)}
                      disabled={busy}
                      placeholder="例如 Gemini 3.1 Pro (High)"
                      maxLength={160}
                    />
                    <small>
                      运行 agy models，填写与上方模型对应的原生名称，包含 High / Low 等后缀。
                    </small>
                  </label>
                )}
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
                      <strong>尚未定位到 {meta?.name}</strong>
                      <p>
                        这不代表未安装。可重新扫描，或在高级设置中填写
                        {meta?.desktop ? '应用可执行文件' : ' CLI '}路径。
                      </p>
                    </div>
                    <button
                      className="icon-btn"
                      title={
                        meta?.installCommand.startsWith('https:') ? '打开安装说明' : '复制安装命令'
                      }
                      aria-label={
                        meta?.installCommand.startsWith('https:') ? '打开安装说明' : '复制安装命令'
                      }
                      onClick={() => {
                        if (meta) {
                          if (meta.installCommand.startsWith('https:'))
                            void window.api
                              .openExternal(meta.installCommand)
                              .catch(() => setDetectionError('无法打开安装说明'))
                          else void onCopyCommand(meta.installCommand)
                        }
                      }}
                    >
                      <CopyIcon size={15} />
                    </button>
                  </div>
                )}
                <div className="detection-row">
                  <span className="fine-print">
                    {agentState?.installed
                      ? `已定位：${agentState.cliPath || meta?.name}`
                      : `尚未定位到${meta?.desktop ? '桌面应用' : ' CLI'}，可先验证并写入配置。`}
                  </span>
                  <button
                    type="button"
                    className="link-btn"
                    disabled={busy || scanning}
                    onClick={() => void scan()}
                  >
                    {scanning ? '扫描中…' : '重新扫描'}
                  </button>
                </div>
                {detectionError && (
                  <div className="notice notice-error" role="alert">
                    {detectionError}
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
                  <label className="field">
                    <span>
                      {meta?.desktop ? '应用可执行文件路径（可选）' : 'CLI 文件路径（可选）'}
                    </span>
                    <input
                      value={cliPath}
                      disabled={busy || scanning}
                      onChange={(event) => setCliPath(event.target.value)}
                      placeholder={
                        state.platform === 'windows'
                          ? '例如 C:\\Users\\你的用户名\\.local\\bin\\claude.exe'
                          : '例如 /home/你的用户名/.opencode/bin/opencode'
                      }
                    />
                  </label>
                  <button
                    type="button"
                    className="btn sm"
                    disabled={busy || scanning}
                    onClick={() => void scan(true)}
                  >
                    {cliPath.trim() ? '使用此路径' : '恢复自动检测'}
                  </button>
                  {state.platform === 'windows' && (
                    <p className="fine-print">
                      Windows 原生与 WSL 的安装和配置相互独立。WSL 中的智能体请使用 WSL
                      内的接入方式。
                    </p>
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
            {!manual && (
              <>
                <button
                  className="btn primary block"
                  onClick={() => void apply()}
                  disabled={
                    busy ||
                    testing ||
                    !probe?.ok ||
                    !agent ||
                    !model ||
                    compatibilityBlocked ||
                    (agent === 'agy' && !agyName.trim())
                  }
                >
                  {busy ? <LoaderIcon size={16} /> : <CheckIcon size={16} />}
                  {busy
                    ? '正在验证模型与工具调用…'
                    : meta
                      ? `导入到 ${meta.name}`
                      : '选择智能体后导入'}
                </button>
                <p className="fine-print centered">
                  导入前会发送两次简短模型请求，可能产生少量费用。验证通过后才备份并写入。
                </p>
              </>
            )}
          </div>
        </>
      )}
    </Modal>
  )
}
