import { useState } from 'react'
import type { AppState } from '../../../shared/types'

export function CodexHistoryPanel({
  state,
  onStateChange,
  notify
}: {
  state: AppState
  onStateChange: (state: AppState) => void
  notify: (message: string, kind?: 'success' | 'error' | 'info') => void
}) {
  const history = state.codexHistory
  const [confirm, setConfirm] = useState<'enable' | 'migrate' | 'disable' | null>(null)
  const [migrate, setMigrate] = useState(false)
  const [restore, setRestore] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  async function save() {
    setBusy(true)
    setError('')
    const enabling = confirm !== 'disable'
    try {
      const result = await window.api.codexHistory.change({
        enabled: enabling,
        migrate: enabling && migrate,
        restore: confirm === 'disable' && restore
      })
      onStateChange({ ...state, codexHistory: result.history })
      const changed = result.files || result.rows
      if (confirm === 'migrate') {
        notify(
          changed
            ? `已迁入 ${result.files} 个会话文件、${result.rows} 条索引；重启 Codex CLI 后可在 /resume 看到。`
            : '没有找到需要迁入的会话，历史已统一。'
        )
      } else {
        notify(
          `历史设置已生效${changed ? `，更新 ${result.files} 个会话文件、${result.rows} 条索引` : ''}。请重新启动 Codex CLI。`
        )
      }
      setConfirm(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  async function recover() {
    setBusy(true)
    setError('')
    try {
      const next = await window.api.codexHistory.recover()
      onStateChange({ ...state, codexHistory: next })
      notify('未完成操作已恢复')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section className="settings-section">
      <h3>Codex 续聊历史</h3>
      <div className="option-row">
        <div>
          <strong>统一 CLI 会话历史</strong>
          <p>官方与本工具的 API 连接共用续聊列表。桌面版通常无需开启。</p>
        </div>
        <button
          className={`btn sm ${history?.enabled ? '' : 'primary'}`}
          disabled={busy || history?.recoveryRequired}
          onClick={() => {
            setConfirm(history?.enabled ? 'disable' : 'enable')
            setMigrate(false)
            setRestore(false)
            setError('')
          }}
        >
          {history?.enabled ? '关闭统一历史' : '开启统一历史'}
        </button>
        {history?.enabled && !history?.recoveryRequired && (
          <button
            className="btn sm"
            disabled={busy}
            onClick={() => {
              setConfirm('migrate')
              setMigrate(true)
              setRestore(false)
              setError('')
            }}
          >
            迁入既有会话
          </button>
        )}
      </div>
      {confirm && (
        <div className="history-confirm">
          <strong>
            {confirm === 'disable'
              ? '关闭统一续聊历史'
              : confirm === 'migrate'
                ? '迁入既有 Codex 会话'
                : '开启统一续聊历史'}
          </strong>
          <p className="fine-print">
            请先关闭所有 Codex CLI 和 App 窗口。操作仅影响当前 Codex 目录，备份保存在本机。
          </p>
          {confirm === 'migrate' ? (
            <p className="fine-print">
              把仍在官方分组的既有会话和索引改到统一分组；已迁入的会话不会重复处理，官方与中转站会共用同一个
              /resume 列表。
            </p>
          ) : confirm === 'enable' ? (
            <label className="history-checkbox">
              <input
                type="checkbox"
                checked={migrate}
                disabled={busy}
                onChange={(e) => setMigrate(e.target.checked)}
              />
              同时迁入既有官方会话（会修改会话与数据库，迁移前自动备份）
            </label>
          ) : history?.backupAvailable ? (
            <label className="history-checkbox">
              <input
                type="checkbox"
                checked={restore}
                disabled={busy}
                onChange={(e) => setRestore(e.target.checked)}
              />
              按迁移账本还原既有会话的原分组，保留新增消息
            </label>
          ) : null}
          <p className="fine-print">
            跨服务的加密推理可能无法继续使用。启用 profile 或在命令行覆盖 provider
            时，请以原工具的实际配置为准。
          </p>
          <div className="modal-actions">
            <button className="btn sm" disabled={busy} onClick={() => setConfirm(null)}>
              取消
            </button>
            <button className="btn primary sm" disabled={busy} onClick={() => void save()}>
              {busy ? '正在处理…' : '确认并应用'}
            </button>
          </div>
        </div>
      )}
      {history?.recoveryRequired && (
        <div className="notice notice-error">
          <p>检测到未完成操作。关闭 Codex 后，可按账本恢复。</p>
          <button className="btn sm" disabled={busy} onClick={() => void recover()}>
            {busy ? '恢复中…' : '恢复未完成操作'}
          </button>
        </div>
      )}
      {(error || history?.error) && (
        <p className="danger-text" role="alert">
          {error || history?.error}
        </p>
      )}
    </section>
  )
}
