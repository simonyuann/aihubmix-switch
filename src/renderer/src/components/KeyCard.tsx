import type { AppState, RelayProbe } from '../../../shared/types'
import { CheckIcon, EyeIcon, KeyIcon, LoaderIcon, PlusIcon, RefreshIcon, ShieldIcon, TrashIcon } from './Icons'

interface Props {
  state: AppState
  keyInput: string
  setKeyInput: (value: string) => void
  showKey: boolean
  setShowKey: (value: boolean) => void
  probe: RelayProbe | null
  busy: string | null
  onSave: () => void
  onTest: () => void
  onClear: () => void
  onOpenAccount: () => void
}

function money(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return '—'
  return `$${value.toFixed(value >= 100 ? 0 : 2)}`
}

function capabilityChip(state: AppState) {
  if (state.messages === 'supported') {
    return <span className="chip ok">Claude Code 可用</span>
  }
  if (state.messages === 'unsupported') {
    return <span className="chip warn">Claude Code 未开启</span>
  }
  return <span className="chip">Claude Code 待检测</span>
}

export function KeyCard({
  state,
  keyInput,
  setKeyInput,
  showKey,
  setShowKey,
  probe,
  busy,
  onSave,
  onTest,
  onClear,
  onOpenAccount
}: Props) {
  const testing = busy === 'test'
  const saving = busy === 'save'

  return (
    <section className="card key-card">
      <div className="section-head">
        <div className="section-title">
          <span className="step">1</span>
          <div>
            <h2>连接 API</h2>
            <p>粘贴你的 API Key，客户端会自动检测余额、模型和 Claude Code 支持情况</p>
          </div>
        </div>
        <button className="link-btn" onClick={onOpenAccount}>
          <PlusIcon size={14} />
          还没有 Key？注册申请
        </button>
      </div>

      <div className="key-input-row">
        <div className={`key-input ${state.hasApiKey && !keyInput ? 'is-filled' : ''}`}>
          <KeyIcon size={17} className="key-input-icon" />
          <input
            type={showKey ? 'text' : 'password'}
            value={keyInput}
            onChange={(event) => setKeyInput(event.target.value)}
            placeholder={state.hasApiKey ? `已保存 ${state.keyPreview}（输入新 Key 可替换）` : '粘贴 API Key，例如 sk-...'}
            spellCheck={false}
            autoComplete="off"
          />
          <button
            className="icon-btn inside"
            onClick={() => setShowKey(!showKey)}
            aria-label={showKey ? '隐藏 Key' : '显示 Key'}
            type="button"
          >
            <EyeIcon open={showKey} size={17} />
          </button>
        </div>
        <button className="btn" onClick={onTest} disabled={testing || saving || (!keyInput.trim() && !state.hasApiKey)}>
          {testing ? <LoaderIcon size={16} /> : <RefreshIcon size={16} />}
          {testing ? '测试中' : '测试连接'}
        </button>
        <button className="btn primary" onClick={onSave} disabled={saving || testing || !keyInput.trim()}>
          {saving ? <LoaderIcon size={16} /> : <CheckIcon size={16} />}
          {saving ? '保存中' : '保存并使用'}
        </button>
      </div>

      <div className="meta-row">
        <span className="chip mono">{state.relayBaseUrl.replace(/^https?:\/\//, '')}</span>
        {state.hasApiKey ? (
          <>
            <span className="chip ok">
              <ShieldIcon size={13} />
              {state.keyPreview}
            </span>
            <span className="chip">余额 {money(state.balance)}</span>
            <span className="chip">{state.models.length} 个模型</span>
            {state.rateMultiplier !== null ? (
              <span className="chip">分组倍率 {state.rateMultiplier}x</span>
            ) : null}
            {capabilityChip(state)}
            <button className="link-btn subtle" onClick={onClear}>
              <TrashIcon size={13} />
              清除
            </button>
          </>
        ) : (
          <span className="chip">尚未保存 Key</span>
        )}
      </div>

      {probe ? (
        <div className={`notice ${probe.ok ? 'notice-ok' : 'notice-error'}`}>
          {probe.ok ? (
            <>
              <CheckIcon size={15} />
              <span>
                连接成功 · {probe.models.length} 个模型 · 延迟 {probe.latencyMs}ms
                {probe.messages === 'supported'
                  ? ' · Claude Code 可用'
                  : probe.messages === 'unsupported'
                    ? ' · 当前分组未开启 Claude Code 调度'
                    : ''}
              </span>
            </>
          ) : (
            <span>{probe.error || '连接失败'}</span>
          )}
        </div>
      ) : null}

      {state.relayError ? (
        <div className="notice notice-error">
          <span>{state.relayError}</span>
        </div>
      ) : null}

      {!state.hasApiKey ? (
        <div className="onboarding">
          <div className="onboarding-step">
            <span>1</span>
            <div>
              <strong>注册账号</strong>
              <p>在 {state.site.name} 注册，支持 DeepSeek / OpenAI / GLM 等分组</p>
            </div>
          </div>
          <div className="onboarding-step">
            <span>2</span>
            <div>
              <strong>创建 API Key</strong>
              <p>在客户端内直接申请，或到网页后台创建</p>
            </div>
          </div>
          <div className="onboarding-step">
            <span>3</span>
            <div>
              <strong>粘贴并接入</strong>
              <p>保存后，一键写入 Codex、Pi、OpenCode 等客户端</p>
            </div>
          </div>
          <button className="btn primary" onClick={onOpenAccount}>
            注册并申请 API Key
          </button>
        </div>
      ) : null}

      {!state.secureStorage ? (
        <div className="notice notice-warn">
          <span>当前系统未提供安全存储，API Key 将以本地编码方式保存。建议开启系统钥匙串/凭据保护。</span>
        </div>
      ) : null}
    </section>
  )
}
