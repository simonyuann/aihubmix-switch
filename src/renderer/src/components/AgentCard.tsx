import type { AgentMeta, AgentState, MessageCapability, RelayModel } from '../../../shared/types'
import { isSelectableModel } from '../../../shared/url'
import { CheckIcon, CopyIcon, FolderIcon, LoaderIcon, TerminalIcon } from './Icons'

interface Props {
  meta: AgentMeta
  state: AgentState
  models: RelayModel[]
  model: string
  messages: MessageCapability
  busy: string | null
  onApply: () => void
  onRestore: () => void
  onModelChange: (model: string) => void
  onOpenConfig: () => void
  onCopyInstall: () => void
}

export function AgentCard({
  meta,
  state,
  models,
  model,
  messages,
  busy,
  onApply,
  onRestore,
  onModelChange,
  onOpenConfig,
  onCopyInstall
}: Props) {
  const applying = busy === `apply:${meta.id}`
  const restoring = busy === `restore:${meta.id}`
  const disabled = Boolean(busy)
  const options = models.filter((item) => isSelectableModel(item.id))
  if (model && !options.some((item) => item.id === model)) {
    options.unshift({ id: model, name: model })
  }
  const claudeBlocked = meta.id === 'claude' && messages === 'unsupported'

  return (
    <article className={`agent-card ${state.connected ? 'is-connected' : ''}`}>
      <header className="agent-head">
        <div
          className="agent-logo"
          style={{ color: meta.color, background: `${meta.color}18`, borderColor: `${meta.color}33` }}
        >
          {meta.name.slice(0, 1)}
        </div>
        <div className="agent-title">
          <div className="agent-name-row">
            <h3>{meta.name}</h3>
            {state.connected ? (
              <span className="chip ok tiny">
                <CheckIcon size={11} />
                已接入
              </span>
            ) : state.installed ? (
              <span className="chip tiny">未接入</span>
            ) : (
              <span className="chip tiny muted">未安装</span>
            )}
          </div>
          <p>
            {meta.subtitle}
            {state.installed ? (
              <span className="version">
                <TerminalIcon size={12} /> {state.version ? `v${state.version}` : '已安装'}
              </span>
            ) : null}
          </p>
        </div>
      </header>

      <div className="agent-body">
        <label className="field-label">默认模型</label>
        <div className="select-wrap">
          <select
            value={model}
            onChange={(event) => onModelChange(event.target.value)}
            disabled={disabled || !options.length}
          >
            {!options.length ? <option value="">暂无可用模型，请先测试连接</option> : null}
            {options.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name === item.id ? item.id : `${item.name}（${item.id}）`}
              </option>
            ))}
          </select>
        </div>

        <button className="path-row" onClick={onOpenConfig} title="打开配置目录">
          <FolderIcon size={13} />
          <span className="mono">{state.configPath || meta.configHint}</span>
        </button>
      </div>

      {state.error ? <div className="notice notice-error compact">{state.error}</div> : null}

      {claudeBlocked ? (
        <div className="notice notice-warn compact">
          当前 Key 所属分组未开启 /v1/messages 调度。请在平台后台开启 allow_messages_dispatch，或改用
          Anthropic 分组后再接入 Claude Code。
        </div>
      ) : null}

      {!state.installed ? (
        <div className="install-hint">
          <div>
            <strong>未检测到 CLI</strong>
            <span>仍可预配置，安装后直接生效</span>
          </div>
          <button className="btn sm" onClick={onCopyInstall}>
            <CopyIcon size={13} />
            复制安装命令
          </button>
        </div>
      ) : null}

      <footer className="agent-foot">
        <button
          className={`btn ${state.connected ? '' : 'primary'} sm grow`}
          onClick={onApply}
          disabled={disabled || claudeBlocked || !model}
        >
          {applying ? <LoaderIcon size={14} /> : <CheckIcon size={14} />}
          {applying ? '写入中' : state.connected ? '重新接入' : '一键接入'}
        </button>
        <button
          className="btn sm"
          onClick={onRestore}
          disabled={disabled || (!state.backupAvailable && !state.connected)}
        >
          {restoring ? <LoaderIcon size={14} /> : null}
          {restoring ? '还原中' : '还原'}
        </button>
      </footer>
    </article>
  )
}
