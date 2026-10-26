import { useAppUpdates } from './updates'
import { useCallback, useEffect, useRef, useState } from 'react'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { AgentState, ApiKeyInfo, AppState, CreatedKey } from '../../shared/types'
import { AGENTS } from '../../shared/constants'
import { ApiList } from './components/ApiList'
import { AuthModal } from './components/AuthModal'
import {
  CreateKeyModal,
  AddKeyModal,
  KeyDetailModal,
  CreatedKeyModal
} from './components/KeyModals'
import { ImportModal } from './components/ImportModal'
import { SessionPage } from './components/SessionPage'
import { DiagnosticsModal } from './components/DiagnosticsModal'
import { RechargeModal } from './components/RechargeModal'
import { BalanceChip } from './components/BalanceChip'
import { SettingsModal } from './components/SettingsModal'
import { Modal } from './components/Modal'
import { Toast, type ToastKind, type ToastState } from './components/Toast'
import {
  CheckIcon,
  ExternalIcon,
  FolderIcon,
  KeyIcon,
  LoaderIcon,
  LogoutIcon,
  PlusIcon,
  RefreshIcon,
  SettingsIcon,
  SparkIcon,
  TerminalIcon,
  UserIcon
} from './components/Icons'

type Panel =
  | 'login'
  | 'register'
  | 'create'
  | 'add'
  | 'details'
  | 'import'
  | 'created'
  | 'settings'
  | 'charge'
  | 'account'
  | 'restore'
  | 'delete'
  | 'diagnostics'
  | 'connection-key'
  | null

export default function App() {
  const updates = useAppUpdates()
  const [state, setState] = useState<AppState | null>(null)
  const [keys, setKeys] = useState<ApiKeyInfo[]>([])
  const [page, setPage] = useState<'keys' | 'agents' | 'sessions'>('keys')
  const [panel, setPanel] = useState<Panel>(null)
  const [item, setItem] = useState<ApiKeyInfo | null>(null)
  const [created, setCreated] = useState<CreatedKey | null>(null)
  const [walletTick, setWalletTick] = useState(0)
  const [restoreTarget, setRestoreTarget] = useState<AgentState | null>(null)
  const [connectionTarget, setConnectionTarget] = useState<AgentState | null>(null)
  const [sessionSource, setSessionSource] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [deleteError, setDeleteError] = useState('')
  const [toast, setToast] = useState<ToastState | null>(null)
  const toastId = useRef(0)
  const loadId = useRef(0)
  const afterAuth = useRef<'create' | 'charge' | null>(null)
  const notify = useCallback((message: string, kind: ToastKind = 'success') => {
    setToast({ id: ++toastId.current, message, kind })
  }, [])

  const load = useCallback(async () => {
    const id = ++loadId.current
    setLoading(true)
    setError('')
    try {
      const next = await window.api.getState()
      if (id !== loadId.current) return
      setState(next)
      const list = await window.api.keys()
      if (id === loadId.current) setKeys(list)
    } catch (err) {
      if (id === loadId.current) {
        setError(err instanceof Error ? err.message : String(err))
        try {
          const next = await window.api.getState()
          if (id === loadId.current) {
            setState(next)
            if (!next.account.loggedIn) setKeys((current) => current.filter((key) => key.local))
          }
        } catch {
          /* Show the original actionable error. */
        }
      }
    } finally {
      if (id === loadId.current) setLoading(false)
    }
  }, [])
  useEffect(() => {
    void load()
    return () => {
      ++loadId.current
    }
  }, [load])
  useEffect(() => {
    if (!toast) return
    const timer = setTimeout(() => setToast(null), toast.kind === 'error' ? 6500 : 3500)
    return () => clearTimeout(timer)
  }, [toast])
  const close = () => {
    if (!busy) {
      if (panel === 'charge') setWalletTick((tick) => tick + 1)
      setPanel(null)
      setItem(null)
      setCreated(null)
    }
  }
  const create = () => {
    if (state?.account.loggedIn) setPanel('create')
    else {
      afterAuth.current = 'create'
      setPanel('register')
    }
  }
  const startImport = (key: ApiKeyInfo) => {
    setConnectionTarget(null)
    setItem(key)
    setCreated(null)
    setPanel('import')
  }
  async function copy(id: number) {
    try {
      await window.api.copyKey(id)
      notify('API Key 已复制')
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    }
  }
  async function copyCommand(command: string) {
    try {
      await writeText(command)
      notify('启动命令已复制')
    } catch {
      notify('复制失败，请重试', 'error')
    }
  }
  async function openPath(target: 'backups' | AgentState['id']) {
    try {
      await window.api.openPath(target)
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    }
  }
  async function logout() {
    setBusy(true)
    try {
      setState(await window.api.account.logout())
      setKeys((current) => current.filter((key) => key.local))
      setPanel(null)
      setCreated(null)
      setItem(null)
      notify('已退出账号')
      void load()
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    } finally {
      setBusy(false)
    }
  }
  async function restore() {
    if (!restoreTarget) return
    setBusy(true)
    try {
      setState(await window.api.restoreAgent(restoreTarget.id))
      setPanel(null)
      notify('已还原接入前的配置')
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    } finally {
      setBusy(false)
    }
  }
  const confirmDelete = (key: ApiKeyInfo) => {
    setItem(key)
    setDeleteError('')
    setPanel('delete')
  }
  async function deleteSelected() {
    if (!item) return
    setBusy(true)
    setDeleteError('')
    try {
      if (item.local) {
        setState(await window.api.removeLocalKey(item.id))
        notify('已从本机移除，智能体配置仍保留')
      } else {
        const result = await window.api.deleteKey(item.id)
        setState(result.state)
        notify(
          result.warning
            ? `平台 Key 已删除；本机清理未完成：${result.warning}`
            : 'API Key 已从平台删除',
          result.warning ? 'info' : 'success'
        )
      }
      setKeys((current) => current.filter((key) => key.id !== item.id))
      setPanel(null)
      setItem(null)
      void load()
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  const connected = state?.agents.filter((agent) => agent.connected || agent.backupAvailable) || []
  const preview = import.meta.env.DEV && new URLSearchParams(location.search).get('preview') === '1'

  if (!state)
    return (
      <div className="boot-screen">
        <div className="brand-logo">
          <SparkIcon size={24} />
        </div>
        <h1>AIHubMix Switch</h1>
        {error ? (
          <>
            <p role="alert">{error}</p>
            <button className="btn primary" onClick={() => void load()}>
              重试
            </button>
          </>
        ) : (
          <p>
            <LoaderIcon size={16} />
            正在准备你的工作台…
          </p>
        )}
      </div>
    )

  return (
    <div className="app-shell">
      {preview && <div className="preview-banner">界面预览 · 演示数据，操作不会修改本机配置</div>}
      <header className="topbar">
        <div className="brand">
          <div className="brand-logo">
            <SparkIcon size={21} />
          </div>
          <div className="brand-text">
            <h1>
              AIHubMix <span>Switch</span>
            </h1>
            <p>连接你的 AI 编程工具</p>
          </div>
        </div>
        <div className="top-actions">
          <BalanceChip state={state} refresh={walletTick} onOpen={() => setPanel('charge')} />
          <button className="btn charge-btn sm" onClick={() => setPanel('charge')}>
            <span aria-hidden="true">＋</span>充值
          </button>
          <span className="top-divider" />
          <button
            className="account-btn"
            onClick={() => {
              afterAuth.current = null
              setPanel(state.account.loggedIn ? 'account' : 'login')
            }}
          >
            <span className="avatar small">
              {state.account.loggedIn ? (
                state.account.email?.slice(0, 1).toUpperCase() || 'U'
              ) : (
                <UserIcon size={16} />
              )}
            </span>
            <span>
              {state.account.loggedIn
                ? state.account.email?.split('@')[0] || '我的账户'
                : '登录 / 注册'}
            </span>
          </button>
        </div>
      </header>
      {['available', 'ready'].includes(updates.status.phase) && (
        <div className="update-banner" role="status">
          <span>
            {updates.status.phase === 'ready'
              ? '更新已下载，准备好后可安装'
              : `新版本 v${updates.status.version} 已发布`}
          </span>
          <button className="link-btn" onClick={() => setPanel('settings')}>
            查看更新
          </button>
        </div>
      )}
      <nav className="navigation" aria-label="主导航">
        <div className="page-tabs">
          <button
            className={page === 'keys' ? 'active' : ''}
            aria-current={page === 'keys' ? 'page' : undefined}
            onClick={() => setPage('keys')}
          >
            <KeyIcon size={16} />
            我的 API
          </button>
          <button
            className={page === 'agents' ? 'active' : ''}
            aria-current={page === 'agents' ? 'page' : undefined}
            onClick={() => setPage('agents')}
          >
            <TerminalIcon size={16} />
            已接入{connected.length > 0 && <span className="tab-count">{connected.length}</span>}
          </button>
          <button
            className={page === 'sessions' ? 'active' : ''}
            aria-current={page === 'sessions' ? 'page' : undefined}
            onClick={() => {
              setSessionSource('')
              setPage('sessions')
            }}
          >
            <FolderIcon size={16} />
            会话
          </button>
        </div>
        <button className="btn ghost sm settings-trigger" onClick={() => setPanel('settings')}>
          <SettingsIcon size={16} />
          设置
        </button>
      </nav>

      <main className="workspace">
        <div className="page-heading">
          <div>
            <div className="eyebrow">YOUR WORKSPACE</div>
            <h2>
              {page === 'keys' ? '我的 API' : page === 'agents' ? '已接入的智能体' : '会话历史'}
            </h2>
            <p>
              {page === 'keys'
                ? '管理你的密钥，让连接变得简单。'
                : page === 'agents'
                  ? '你的编程工具，连接状态一目了然。'
                  : '在本机找回上下文，回到熟悉的工具继续。'}
            </p>
          </div>
          {page !== 'sessions' && (
            <button
              className="btn primary"
              onClick={page === 'keys' ? create : () => setPage('keys')}
            >
              <PlusIcon size={16} />
              {page === 'keys' ? '创建 API Key' : '添加连接'}
            </button>
          )}
        </div>
        {state.storageError && (
          <div className="notice notice-error" role="alert">
            {state.storageError}
          </div>
        )}
        {state.legacyData && !state.hasApiKey && (
          <div className="notice">
            检测到旧版数据，原配置与备份已保留。请重新登录，或添加已有 API Key。
          </div>
        )}
        {!state.account.loggedIn && page === 'keys' && (
          <div className="welcome-banner">
            <span className="welcome-icon">
              <UserIcon size={21} />
            </span>
            <div>
              <strong>登录，开始管理你的 API</strong>
              <p>注册后即可申请 Key；已有 Key 也可以直接添加。</p>
            </div>
            <button
              className="btn sm"
              onClick={() => {
                afterAuth.current = null
                setPanel('login')
              }}
            >
              登录
            </button>
            <button
              className="btn text-blue sm"
              onClick={() => {
                afterAuth.current = 'create'
                setPanel('register')
              }}
            >
              注册账号 <span aria-hidden="true">→</span>
            </button>
          </div>
        )}
        {page === 'keys' ? (
          <ApiList
            items={keys}
            loading={loading}
            error={error}
            onRefresh={() => void load()}
            onCreate={create}
            onAdd={() => setPanel('add')}
            onDetails={(key) => {
              setItem(key)
              setPanel('details')
            }}
            onCopy={(id) => void copy(id)}
            onImport={startImport}
            onDelete={confirmDelete}
          />
        ) : page === 'sessions' ? (
          <SessionPage key={sessionSource} initialSource={sessionSource} onNotify={notify} />
        ) : (
          <section className="api-panel connected-panel" aria-label="已接入的智能体">
            <div className="panel-toolbar">
              <div className="panel-label">
                <TerminalIcon size={16} />
                <strong>智能体连接</strong>
                <span className="count">{connected.length}</span>
              </div>
              <button
                className="icon-btn"
                aria-label="刷新接入状态"
                onClick={() => {
                  void window.api
                    .refresh()
                    .then(setState)
                    .catch((err) => notify(String(err), 'error'))
                }}
              >
                <RefreshIcon size={15} />
              </button>
            </div>
            {connected.length ? (
              connected.map((agent) => (
                <div key={agent.id} className="connected-row">
                  <span className="agent-monogram" style={{ color: AGENTS[agent.id].color }}>
                    {AGENTS[agent.id].name.slice(0, 1)}
                  </span>
                  <div className="connection-info">
                    <div>
                      <strong>{AGENTS[agent.id].name}</strong>
                      <span
                        className={`status-dot ${agent.connected && !agent.configChanged ? 'status-ok' : 'status-warn'}`}
                      >
                        {agent.keyDeleted
                          ? 'Key 已删除'
                          : agent.configChanged
                            ? '配置已变更'
                            : agent.connected
                              ? '已配置'
                              : '当前未启用'}
                      </span>
                    </div>
                    <p>
                      {agent.appliedModel || '未指定模型'}
                      {agent.compatibility && <span className="chip tiny">本地兼容连接</span>}
                    </p>
                    <small>
                      {agent.configChanged
                        ? '当前配置与上次导入记录不同'
                        : agent.keyLabel || '从已有配置识别'}
                    </small>
                    {['mcode', 'omp'].includes(agent.id) && (
                      <p className="fine-print">供应商已添加，请在原工具中选择 csmy 下的模型。</p>
                    )}
                    {agent.error && <p className="danger-text">{agent.error}</p>}
                  </div>
                  <div className="connection-actions">
                    <button
                      className="btn sm"
                      onClick={() => {
                        setConnectionTarget(agent)
                        setPanel('diagnostics')
                      }}
                    >
                      连接诊断
                    </button>
                    {agent.launchCommand && !agent.keyDeleted && (
                      <button
                        className="btn sm"
                        onClick={() => void copyCommand(agent.launchCommand!)}
                      >
                        {state.platform === 'windows' ? '复制 PowerShell 命令' : '复制启动命令'}
                      </button>
                    )}
                    <button
                      className="btn sm"
                      onClick={() => {
                        setConnectionTarget(agent)
                        setPanel('connection-key')
                      }}
                    >
                      更换配置
                    </button>
                    {['claude', 'codex', 'pi', 'grokbuild', 'gemini', 'openclaw'].includes(
                      agent.id
                    ) && (
                      <button
                        className="btn ghost sm"
                        onClick={() => {
                          setSessionSource(agent.id)
                          setPage('sessions')
                        }}
                      >
                        查看会话
                      </button>
                    )}
                    {agent.backupAvailable && (
                      <button
                        className="btn ghost sm"
                        onClick={() => {
                          setRestoreTarget(agent)
                          setPanel('restore')
                        }}
                      >
                        还原
                      </button>
                    )}
                    <button
                      className="icon-btn"
                      title="打开配置目录"
                      aria-label={`打开 ${AGENTS[agent.id].name} 配置目录`}
                      onClick={() => void openPath(agent.id)}
                    >
                      <FolderIcon size={16} />
                    </button>
                  </div>
                </div>
              ))
            ) : (
              <div className="empty-state">
                <div className="empty-illustration">
                  <div className="empty-key neutral">
                    <TerminalIcon size={30} />
                  </div>
                </div>
                <h3>还没有连接智能体</h3>
                <p>先选择一个 API Key，再导入到你使用的工具。</p>
                <button className="btn primary" onClick={() => setPage('keys')}>
                  去选择 API Key <span aria-hidden="true">→</span>
                </button>
              </div>
            )}
          </section>
        )}
      </main>
      <footer className="app-footer">
        <div>
          <button className="link-btn subtle" onClick={() => void openPath('backups')}>
            备份目录
          </button>
          <span className="footer-separator">·</span>
          <button
            className="link-btn subtle"
            onClick={() => {
              void window.api
                .openExternal(state.site.docUrl)
                .catch((err) => notify(String(err), 'error'))
            }}
          >
            帮助文档
            <ExternalIcon size={11} />
          </button>
          <span className="version">v{state.version}</span>
        </div>
      </footer>

      {(panel === 'login' || panel === 'register') && (
        <AuthModal
          initialMode={panel}
          onClose={close}
          onSuccess={(next) => {
            setState(next)
            setKeys((current) => current.filter((key) => key.local))
            setPanel(afterAuth.current)
            afterAuth.current = null
            notify('登录成功')
            void load()
          }}
        />
      )}
      {panel === 'diagnostics' && connectionTarget && (
        <DiagnosticsModal agent={connectionTarget} onClose={close} notify={notify} />
      )}
      {panel === 'connection-key' && connectionTarget && (
        <Modal
          open
          title={`更换 ${AGENTS[connectionTarget.id].name} 配置`}
          subtitle="选择本次使用的 Key，下一步会自动验证并写入配置。"
          onClose={close}
          width={460}
        >
          <div className="connection-key-list">
            {keys.map((key) => (
              <button
                key={key.id}
                className="connection-key-choice"
                onClick={() => {
                  setItem(key)
                  setPanel('import')
                }}
              >
                <div>
                  <strong>{key.name}</strong>
                  <code>{key.keyMasked}</code>
                </div>
                <span>
                  {key.id === connectionTarget.keyId && !connectionTarget.keyDeleted
                    ? '当前使用'
                    : '选择 →'}
                </span>
              </button>
            ))}
          </div>
          {!keys.length && <p className="fine-print">暂无可读取的 Key，请先登录或添加已有 Key。</p>}
          <button
            className="btn block"
            onClick={() => {
              setPage('keys')
              setPanel(null)
            }}
          >
            管理 API Key
          </button>
        </Modal>
      )}
      {panel === 'create' && (
        <CreateKeyModal
          onClose={close}
          onCreated={(result) => {
            setCreated(result)
            setState(result.state)
            setPanel('created')
            void load()
          }}
        />
      )}
      {panel === 'created' && created && (
        <CreatedKeyModal
          item={created.info}
          warning={created.warning}
          onClose={close}
          onImport={() => startImport(created.info)}
          onCopy={() => void copy(created.info.id)}
        />
      )}
      {panel === 'add' && (
        <AddKeyModal
          onClose={close}
          onSaved={(info) => {
            void load()
            startImport(info)
            notify('Key 已验证并保存')
          }}
        />
      )}
      {panel === 'details' && item && (
        <KeyDetailModal
          item={item}
          onClose={close}
          onCopy={copy}
          onDelete={() => confirmDelete(item)}
          onRemove={async (id) => {
            setState(await window.api.removeLocalKey(id))
            setPanel(null)
            setItem(null)
            notify('已从本机移除')
            void load()
          }}
        />
      )}
      {panel === 'import' && item && (
        <ImportModal
          key={item.id}
          item={item}
          initialAgent={connectionTarget?.id}
          state={state}
          onClose={close}
          onImported={setState}
          onCopyCommand={async (command) => {
            try {
              await writeText(command)
              notify('已复制')
            } catch {
              notify('复制失败，请稍后重试', 'error')
            }
          }}
        />
      )}
      <SettingsModal
        update={updates}
        open={panel === 'settings'}
        state={state}
        onClose={close}
        onStateChange={setState}
        notify={notify}
      />
      <RechargeModal
        key={`${state.account.userId || 'guest'}:${state.relayBaseUrl}:${state.account.loggedIn}`}
        open={panel === 'charge'}
        state={state}
        onClose={close}
        onLogin={() => {
          afterAuth.current = 'charge'
          setPanel('login')
        }}
        onPaid={() => {
          setWalletTick((tick) => tick + 1)
          void load()
        }}
      />
      {panel === 'account' && (
        <Modal open title="我的账户" onClose={close} width={420} busy={busy}>
          <div className="account-identity">
            <span className="avatar">{state.account.email?.slice(0, 1).toUpperCase() || 'U'}</span>
            <div>
              <strong>{state.account.email || '已登录'}</strong>
              <p>AIHubMix 账户</p>
            </div>
          </div>
          <button className="btn block" disabled={busy} onClick={() => void logout()}>
            {busy ? <LoaderIcon size={16} /> : <LogoutIcon size={16} />}退出登录
          </button>
        </Modal>
      )}
      {panel === 'restore' && restoreTarget && (
        <Modal
          open
          title={`还原 ${AGENTS[restoreTarget.id].name}`}
          subtitle="恢复导入前的配置，后续手动修改的字段会尽量保留。"
          onClose={close}
          width={440}
          busy={busy}
        >
          <div className="notice">备份文件会继续保留在本机。还原后，可能需要重新打开智能体。</div>
          <div className="confirm-actions">
            <button className="btn" onClick={close} disabled={busy}>
              取消
            </button>
            <button className="btn primary" disabled={busy} onClick={() => void restore()}>
              {busy ? <LoaderIcon size={15} /> : <CheckIcon size={15} />}确认还原
            </button>
          </div>
        </Modal>
      )}
      {panel === 'delete' && item && (
        <Modal
          open
          title={item.local ? '从本机移除 API Key' : '删除 API Key'}
          subtitle={item.name}
          onClose={close}
          width={460}
          busy={busy}
        >
          <div className="selected-key">
            <KeyIcon size={17} />
            <code>{item.keyMasked}</code>
          </div>
          <div className={`notice ${item.local ? '' : 'notice-warn'}`}>
            {item.local
              ? '仅移除此设备保存的 Key，平台 Key 和已导入的智能体配置不会改变。'
              : '此操作将从平台删除 Key，删除后不能再用它调用模型。已导入的智能体配置不会自动还原。'}
          </div>
          {!item.local && state.agents.some((agent) => agent.keyId === item.id) && (
            <p className="fine-print">
              此 Key 已用于{' '}
              {state.agents
                .filter((agent) => agent.keyId === item.id)
                .map((agent) => AGENTS[agent.id].name)
                .join('、')}
              ，删除后这些连接可能失效。你可以在「已接入」中更换 Key 或还原配置。
            </p>
          )}
          {deleteError && (
            <div className="notice notice-error" role="alert">
              {deleteError}
            </div>
          )}
          <div className="confirm-actions">
            <button className="btn" disabled={busy} onClick={close}>
              取消
            </button>
            <button className="btn danger" disabled={busy} onClick={() => void deleteSelected()}>
              {busy && <LoaderIcon size={15} />} {item.local ? '确认移除' : '确认删除'}
            </button>
          </div>
        </Modal>
      )}
      <Toast toast={toast} />
    </div>
  )
}
