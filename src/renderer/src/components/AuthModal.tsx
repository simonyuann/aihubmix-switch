import { useState } from 'react'
import type { AppState } from '../../../shared/types'
import { Modal } from './Modal'
import { LoaderIcon, SparkIcon } from './Icons'

export function AuthModal({
  initialMode,
  onClose,
  onSuccess
}: {
  initialMode: 'login' | 'register'
  onClose: () => void
  onSuccess: (state: AppState) => void
}) {
  const [mode, setMode] = useState(initialMode)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [invite, setInvite] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  async function submit() {
    setBusy(true)
    setError('')
    try {
      const next = await window.api.account[mode]({
        email: email.trim(),
        password,
        invitationCode: mode === 'register' ? invite.trim() || undefined : undefined
      })
      setPassword('')
      onSuccess(next)
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Modal
      open
      title={mode === 'login' ? '欢迎回来' : '开始使用 AIHubMix'}
      subtitle="一个账号，管理你的 API 与智能体连接。"
      onClose={onClose}
      width={420}
      busy={busy}
    >
      <div className="auth-brand">
        <SparkIcon size={24} />
      </div>
      <div className="segmented">
        <button
          className={mode === 'login' ? 'active' : ''}
          disabled={busy}
          onClick={() => {
            setMode('login')
            setError('')
          }}
        >
          登录
        </button>
        <button
          className={mode === 'register' ? 'active' : ''}
          disabled={busy}
          onClick={() => {
            setMode('register')
            setError('')
          }}
        >
          注册账号
        </button>
      </div>
      <form
        className="form-stack"
        onSubmit={(event) => {
          event.preventDefault()
          void submit()
        }}
      >
        <label className="field">
          <span>邮箱</span>
          <input
            type="email"
            value={email}
            required
            autoComplete="email"
            placeholder="you@example.com"
            disabled={busy}
            onChange={(event) => setEmail(event.target.value)}
          />
        </label>
        <label className="field">
          <span>密码</span>
          <input
            type="password"
            value={password}
            required
            minLength={mode === 'register' ? 6 : undefined}
            autoComplete={mode === 'register' ? 'new-password' : 'current-password'}
            placeholder={mode === 'register' ? '至少 6 位密码' : '请输入密码'}
            disabled={busy}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>
        {mode === 'register' && (
          <details className="optional-field">
            <summary>
              我有邀请码 <span>选填</span>
            </summary>
            <label className="field">
              <span className="sr-only">邀请码</span>
              <input
                placeholder="填写邀请码"
                value={invite}
                disabled={busy}
                onChange={(event) => setInvite(event.target.value)}
              />
            </label>
          </details>
        )}
        {error && (
          <div className="notice notice-error" role="alert">
            {error}
          </div>
        )}
        <button className="btn primary block" disabled={busy} type="submit">
          {busy && <LoaderIcon size={16} />}
          {busy ? '请稍候…' : mode === 'login' ? '登录' : '注册并继续'}
        </button>
      </form>
      <p className="fine-print centered">API Key 和登录凭据通过系统安全存储保护。</p>
    </Modal>
  )
}
