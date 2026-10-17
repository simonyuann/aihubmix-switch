import { useEffect, useState } from 'react'
import type { AppState, ApiKeyInfo, GroupInfo } from '../../../shared/types'
import { Modal, LinkButton } from './Modal'
import type { ToastKind } from './Toast'
import {
  CheckIcon,
  CopyIcon,
  ExternalIcon,
  KeyIcon,
  LoaderIcon,
  LogoutIcon,
  PlusIcon,
  ShieldIcon,
  UserIcon
} from './Icons'

interface Props {
  open: boolean
  onClose: () => void
  state: AppState
  onStateChange: (state: AppState) => void
  notify: (message: string, kind?: ToastKind) => void
}

const PLATFORM_LABEL: Record<string, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  gemini: 'Gemini',
  deepseek: 'DeepSeek',
  grok: 'Grok',
  antigravity: 'Antigravity',
  composite: '聚合'
}

function statusLabel(status: string): string {
  if (status === 'active') return '正常'
  if (status === 'inactive') return '已停用'
  if (status === 'quota_exhausted') return '额度用尽'
  if (status === 'expired') return '已过期'
  return status
}

export function AccountModal({ open, onClose, state, onStateChange, notify }: Props) {
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [invite, setInvite] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [groups, setGroups] = useState<GroupInfo[]>([])
  const [keys, setKeys] = useState<ApiKeyInfo[]>([])
  const [selectedGroup, setSelectedGroup] = useState<number | null>(null)
  const [createdKey, setCreatedKey] = useState<string | null>(null)
  const [tab, setTab] = useState<'create' | 'keys'>('create')

  const loggedIn = state.account.loggedIn

  async function loadAccountData() {
    setBusy('load')
    setError(null)
    try {
      const [nextGroups, nextKeys] = await Promise.all([
        window.api.account.groups(),
        window.api.account.keys()
      ])
      setGroups(nextGroups)
      setKeys(nextKeys)
      setSelectedGroup((current) => {
        if (current && nextGroups.some((group) => group.id === current)) return current
        return (
          nextGroups.find((group) => group.allowMessagesDispatch)?.id ??
          nextGroups.find(
            (group) =>
              group.platform === 'openai' ||
              group.platform === 'anthropic' ||
              group.name.toLowerCase().includes('openai')
          )?.id ??
          nextGroups[0]?.id ??
          null
        )
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  useEffect(() => {
    if (!open) return
    setError(null)
    setCreatedKey(null)
    if (loggedIn) {
      void loadAccountData()
    } else {
      setGroups([])
      setKeys([])
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, loggedIn])

  async function submitAuth() {
    if (!email.trim() || !password) {
      setError('请填写邮箱和密码')
      return
    }
    if (mode === 'register' && password.length < 6) {
      setError('密码至少 6 位')
      return
    }
    setBusy('auth')
    setError(null)
    try {
      if (mode === 'login') {
        await window.api.account.login({ email: email.trim(), password })
      } else {
        await window.api.account.register({
          email: email.trim(),
          password,
          invitationCode: invite.trim() || undefined
        })
      }
      setPassword('')
      setInvite('')
      const nextState = await window.api.getState()
      onStateChange(nextState)
      notify(mode === 'login' ? '登录成功' : '注册成功，请选择分组申请 Key')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  async function createKey() {
    if (!selectedGroup) {
      setError('请选择要使用的分组')
      return
    }
    setBusy('create')
    setError(null)
    try {
      const created = await window.api.account.createKey({
        groupId: selectedGroup,
        name: 'AIHubMix Switch'
      })
      setCreatedKey(created.key)
      onStateChange(created.state)
      notify('API Key 已创建并保存')
      const nextKeys = await window.api.account.keys()
      setKeys(nextKeys)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  async function useKey(id: number) {
    setBusy(`use:${id}`)
    setError(null)
    try {
      const nextState = await window.api.account.useKey(id)
      onStateChange(nextState)
      notify('已切换到该 Key')
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  async function logout() {
    setBusy('logout')
    try {
      await window.api.account.logout()
      const nextState = await window.api.getState()
      onStateChange(nextState)
      setGroups([])
      setKeys([])
      setCreatedKey(null)
      notify('已退出账号')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value)
      notify('已复制到剪贴板')
    } catch {
      notify('复制失败，请手动选择文本复制', 'error')
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      width={640}
      title={loggedIn ? '账号与 API Key' : '登录 / 注册'}
      subtitle={`在 ${state.site.name} 注册后，可直接在客户端申请 API Key`}
    >
      {!loggedIn ? (
        <div className="auth-box">
          <div className="segmented">
            <button
              className={mode === 'login' ? 'active' : ''}
              onClick={() => {
                setMode('login')
                setError(null)
              }}
            >
              登录
            </button>
            <button
              className={mode === 'register' ? 'active' : ''}
              onClick={() => {
                setMode('register')
                setError(null)
              }}
            >
              注册
            </button>
          </div>

          <label className="field">
            <span>邮箱</span>
            <input
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              placeholder="you@example.com"
              autoComplete="email"
            />
          </label>
          <label className="field">
            <span>密码</span>
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder={mode === 'register' ? '至少 6 位' : '你的登录密码'}
              autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void submitAuth()
              }}
            />
          </label>
          {mode === 'register' ? (
            <label className="field">
              <span>
                邀请码 <em>可选</em>
              </span>
              <input
                value={invite}
                onChange={(event) => setInvite(event.target.value)}
                placeholder="有邀请码时填写"
                onKeyDown={(event) => {
                  if (event.key === 'Enter') void submitAuth()
                }}
              />
            </label>
          ) : null}

          {error ? <div className="notice notice-error compact">{error}</div> : null}

          <button className="btn primary block" onClick={submitAuth} disabled={busy === 'auth'}>
            {busy === 'auth' ? <LoaderIcon size={16} /> : <UserIcon size={16} />}
            {busy === 'auth' ? '请稍候' : mode === 'login' ? '登录' : '注册账号'}
          </button>

          <p className="auth-tip">
            {mode === 'register' ? '注册即表示同意服务条款，' : '还没有账号？'}
            <LinkButton href={state.site.registerUrl}>
              {mode === 'register' ? '查看服务条款' : '前往注册'}
            </LinkButton>
          </p>
        </div>
      ) : (
        <div className="account-box">
          <div className="account-bar">
            <div className="account-identity">
              <span className="avatar">{state.account.email?.slice(0, 1).toUpperCase() || 'U'}</span>
              <div>
                <strong>{state.account.email || '已登录'}</strong>
                <span>余额 {state.balance !== null ? `$${state.balance.toFixed(2)}` : '—'}</span>
              </div>
            </div>
            <button className="btn sm" onClick={logout} disabled={Boolean(busy)}>
              <LogoutIcon size={14} />
              退出
            </button>
          </div>

          <div className="segmented">
            <button className={tab === 'create' ? 'active' : ''} onClick={() => setTab('create')}>
              <PlusIcon size={14} />
              申请新 Key
            </button>
            <button className={tab === 'keys' ? 'active' : ''} onClick={() => setTab('keys')}>
              <KeyIcon size={14} />
              我的 Key（{keys.length}）
            </button>
          </div>

          {tab === 'create' ? (
            <div className="create-box">
              <div className="box-title">
                <div>
                  <h3>选择分组</h3>
                  <p>不同分组对应不同模型与倍率，实际可用模型以 Key 为准</p>
                </div>
                <button className="link-btn" onClick={loadAccountData} disabled={busy === 'load'}>
                  {busy === 'load' ? <LoaderIcon size={13} /> : null}
                  刷新
                </button>
              </div>

              {groups.length ? (
                <div className="group-list">
                  {groups.map((group) => {
                    const selected = selectedGroup === group.id
                    return (
                      <button
                        key={group.id}
                        className={`group-item ${selected ? 'selected' : ''}`}
                        onClick={() => setSelectedGroup(group.id)}
                      >
                        <span className="group-radio">{selected ? <CheckIcon size={13} /> : null}</span>
                        <span className="group-main">
                          <span className="group-name">
                            {group.name}
                            <span className="chip tiny">{PLATFORM_LABEL[group.platform] || group.platform}</span>
                            {group.allowMessagesDispatch ? (
                              <span className="chip ok tiny">Claude Code</span>
                            ) : null}
                          </span>
                          <span className="group-desc">
                            {group.description || '按量计费分组'} · 倍率 {group.rateMultiplier}x
                          </span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              ) : (
                <div className="empty-line">
                  {busy === 'load' ? '正在加载分组…' : '没有可用分组，请确认账号权限或到网页端查看'}
                </div>
              )}

              {error ? <div className="notice notice-error compact">{error}</div> : null}

              <button
                className="btn primary block"
                onClick={createKey}
                disabled={busy === 'create' || !selectedGroup}
              >
                {busy === 'create' ? <LoaderIcon size={16} /> : <PlusIcon size={16} />}
                {busy === 'create' ? '创建中' : '创建 API Key 并保存'}
              </button>

              {createdKey ? (
                <div className="created-key">
                  <div className="created-key-head">
                    <span>
                      <ShieldIcon size={14} /> 新 Key 已创建并保存
                    </span>
                    <button className="btn sm" onClick={() => copy(createdKey)}>
                      <CopyIcon size={13} />
                      复制
                    </button>
                  </div>
                  <code>{createdKey}</code>
                  <p>客户端已自动测试并保存这个 Key，可以随时在「我的 Key」中切换。</p>
                </div>
              ) : null}

              <p className="fine-print">
                申请前请确认账号已有余额。充值、订阅等操作请前往
                <LinkButton href={state.site.apiBaseUrl}>网页后台</LinkButton>。
              </p>
            </div>
          ) : (
            <div className="keys-box">
              {keys.length ? (
                <div className="key-list">
                  {keys.map((item) => (
                    <div className="key-item" key={item.id}>
                      <div className="key-item-main">
                        <div className="key-item-title">
                          <strong>{item.name}</strong>
                          <span className={`chip tiny ${item.status === 'active' ? 'ok' : 'warn'}`}>
                            {statusLabel(item.status)}
                          </span>
                          {item.groupName ? <span className="chip tiny">{item.groupName}</span> : null}
                        </div>
                        <code>{item.keyMasked}</code>
                        <span className="key-item-sub">
                          创建于 {item.createdAt ? new Date(item.createdAt).toLocaleDateString() : '—'}
                        </span>
                      </div>
                      <button
                        className="btn sm"
                        onClick={() => useKey(item.id)}
                        disabled={Boolean(busy) || item.status !== 'active'}
                      >
                        {busy === `use:${item.id}` ? <LoaderIcon size={13} /> : null}
                        使用
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="empty-line">
                  {busy === 'load' ? '正在加载…' : '还没有 API Key，先到「申请新 Key」创建一个'}
                </div>
              )}
              {error ? <div className="notice notice-error compact">{error}</div> : null}
            </div>
          )}

          <div className="account-foot">
            <span className="muted">站点：{state.site.apiBaseUrl}</span>
            <LinkButton href={state.site.apiBaseUrl}>
              打开网页后台 <ExternalIcon size={12} />
            </LinkButton>
          </div>
        </div>
      )}
    </Modal>
  )
}
