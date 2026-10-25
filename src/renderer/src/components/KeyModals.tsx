import { useEffect, useState } from 'react'
import type { ApiKeyInfo, CreatedKey, GroupInfo } from '../../../shared/types'
import { Modal } from './Modal'
import { CheckIcon, CopyIcon, EyeIcon, KeyIcon, LoaderIcon, PlusIcon, ShieldIcon } from './Icons'

export function CreateKeyModal({
  onClose,
  onCreated
}: {
  onClose: () => void
  onCreated: (created: CreatedKey) => void
}) {
  const [groups, setGroups] = useState<GroupInfo[]>([])
  const [selected, setSelected] = useState('')
  const [name, setName] = useState('我的 API Key')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError('')
    void window.api.account
      .groups()
      .then((next) => {
        if (!cancelled) {
          setGroups(next)
          if (next.length === 1) setSelected(String(next[0]?.id))
        }
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [retry])
  async function create() {
    if (!selected) return
    setBusy(true)
    setError('')
    try {
      onCreated(
        await window.api.account.createKey({
          groupId: Number(selected),
          name: name.trim() || '我的 API Key'
        })
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  const group = groups.find((item) => item.id === Number(selected))
  return (
    <Modal
      open
      title="创建 API Key"
      subtitle="选择需要的模型服务，创建后即可导入智能体。"
      onClose={onClose}
      width={480}
      busy={busy}
    >
      <form
        className="form-stack"
        onSubmit={(event) => {
          event.preventDefault()
          void create()
        }}
      >
        <label className="field">
          <span>模型服务</span>
          <select
            required
            value={selected}
            disabled={loading || busy}
            onChange={(event) => setSelected(event.target.value)}
          >
            <option value="">{loading ? '正在加载…' : '请选择模型服务'}</option>
            {groups.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        {group && (
          <div className="service-detail">
            <strong>{group.name}</strong>
            <p>{group.description || group.platform || '按量计费模型服务'}</p>
            <div className="inline-meta">
              <span>分组倍率 {group.rateMultiplier ?? '—'}×</span>
              <span className={group.allowMessagesDispatch ? 'text-success' : ''}>
                {group.allowMessagesDispatch
                  ? '原生支持 Claude Code'
                  : '支持 Claude Code（本地兼容）'}
              </span>
            </div>
          </div>
        )}
        {!loading && !groups.length && !error && (
          <div className="notice">暂无可用模型服务，请联系平台管理员确认账号权限。</div>
        )}
        <label className="field">
          <span>
            名称 <em>方便自己辨认</em>
          </span>
          <input
            value={name}
            maxLength={80}
            disabled={busy}
            placeholder="我的 API Key"
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {error && (
          <div className="notice notice-error" role="alert">
            {error}
            <button
              className="link-btn"
              type="button"
              disabled={busy}
              onClick={() => setRetry((v) => v + 1)}
            >
              重新加载服务
            </button>
          </div>
        )}
        <button className="btn primary block" disabled={busy || loading || !selected} type="submit">
          {busy ? <LoaderIcon size={16} /> : <PlusIcon size={16} />}
          {busy ? '正在创建…' : '创建 API Key'}
        </button>
      </form>
    </Modal>
  )
}

export function AddKeyModal({
  onClose,
  onSaved
}: {
  onClose: () => void
  onSaved: (info: ApiKeyInfo) => void
}) {
  const [value, setValue] = useState('')
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  async function save() {
    setBusy(true)
    setError('')
    try {
      const result = await window.api.saveKey({ apiKey: value.trim(), name })
      setValue('')
      onSaved(result.info)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Modal
      open
      title="添加已有 API Key"
      subtitle="无需登录，验证后保存在本机。"
      onClose={onClose}
      width={460}
      busy={busy}
    >
      <form
        className="form-stack"
        onSubmit={(event) => {
          event.preventDefault()
          void save()
        }}
      >
        <label className="field">
          <span>API Key</span>
          <input
            type="password"
            value={value}
            required
            autoComplete="off"
            spellCheck={false}
            placeholder="粘贴 sk-…"
            disabled={busy}
            onChange={(event) => setValue(event.target.value)}
          />
        </label>
        <label className="field">
          <span>
            名称 <em>选填</em>
          </span>
          <input
            value={name}
            placeholder="例如：日常编程"
            maxLength={80}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {error && (
          <div className="notice notice-error" role="alert">
            {error}
          </div>
        )}
        <button className="btn primary block" type="submit" disabled={busy || !value.trim()}>
          {busy ? <LoaderIcon size={16} /> : <ShieldIcon size={16} />}
          {busy ? '正在验证连接…' : '验证并保存'}
        </button>
        <p className="fine-print centered">验证和保存不会修改任何智能体配置。</p>
      </form>
    </Modal>
  )
}

export function KeyDetailModal({
  item,
  onClose,
  onCopy,
  onRemove,
  onDelete
}: {
  item: ApiKeyInfo
  onClose: () => void
  onCopy: (id: number) => Promise<void>
  onRemove: (id: number) => Promise<void>
  onDelete: () => void
}) {
  const [full, setFull] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [confirm, setConfirm] = useState(false)
  useEffect(() => {
    if (!full) return
    const timer = setTimeout(() => setFull(''), 60000)
    return () => clearTimeout(timer)
  }, [full])
  async function reveal() {
    if (full) {
      setFull('')
      return
    }
    setBusy(true)
    setError('')
    try {
      setFull(await window.api.getKey(item.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Modal open title={item.name} subtitle="API Key 详情" onClose={onClose} width={490} busy={busy}>
      <div className="secret-display">
        <KeyIcon size={18} />
        <code>{full || item.keyMasked}</code>
      </div>
      <div className="detail-actions">
        <button className="btn sm" onClick={() => void reveal()} disabled={busy}>
          {busy ? <LoaderIcon size={14} /> : <EyeIcon size={14} open={Boolean(full)} />}
          {full ? '隐藏' : '查看完整 Key'}
        </button>
        <button className="btn sm" disabled={busy} onClick={() => void onCopy(item.id)}>
          <CopyIcon size={14} />
          复制 Key
        </button>
      </div>
      <dl className="detail-list">
        <div>
          <dt>模型服务</dt>
          <dd>{item.groupName || (item.local ? '手动添加' : '—')}</dd>
        </div>
        <div>
          <dt>Key 状态</dt>
          <dd>
            {item.status === 'active'
              ? '正常'
              : item.status === 'expired'
                ? '已过期'
                : item.status === 'quota_exhausted'
                  ? '额度用尽'
                  : '已停用'}
          </dd>
        </div>
        {item.quota != null && (
          <div>
            <dt>Key 配额 / 已用</dt>
            <dd>
              {item.quota} / {item.quotaUsed ?? '—'}
            </dd>
          </div>
        )}
        <div>
          <dt>创建时间</dt>
          <dd>
            {item.createdAt && Number.isFinite(Date.parse(item.createdAt))
              ? new Date(item.createdAt).toLocaleString()
              : '—'}
          </dd>
        </div>
      </dl>
      {full && <p className="fine-print">完整 Key 将在 60 秒后自动隐藏，请勿分享给他人。</p>}
      {error && (
        <div className="notice notice-error" role="alert">
          {error}
        </div>
      )}
      {!item.local && (
        <div className="danger-area">
          <button className="link-btn danger-text" onClick={onDelete}>
            删除平台 API Key
          </button>
        </div>
      )}
      {item.local && (
        <div className="danger-area">
          {confirm ? (
            <>
              <p>仅移除本机保存的 Key，已导入的智能体配置仍会保留。</p>
              <button
                className="btn danger sm"
                disabled={busy}
                onClick={() => {
                  setBusy(true)
                  void onRemove(item.id)
                    .catch((err) => setError(String(err)))
                    .finally(() => setBusy(false))
                }}
              >
                确认移除
              </button>
              <button className="btn ghost sm" disabled={busy} onClick={() => setConfirm(false)}>
                取消
              </button>
            </>
          ) : (
            <button className="link-btn danger-text" onClick={() => setConfirm(true)}>
              从本机移除
            </button>
          )}
        </div>
      )}
    </Modal>
  )
}

export function CreatedKeyModal({
  item,
  warning,
  onClose,
  onImport,
  onCopy
}: {
  item: ApiKeyInfo
  warning?: string | null
  onClose: () => void
  onImport: () => void
  onCopy: () => void
}) {
  return (
    <Modal
      open
      title="API Key 已创建"
      subtitle="接下来，把它连接到你喜欢的编程工具。"
      onClose={onClose}
      width={440}
    >
      <div className="success-block">
        <div className="success-symbol">
          <CheckIcon size={26} />
        </div>
        <h3>{item.name}</h3>
        <code>{item.keyMasked}</code>
      </div>
      {warning && (
        <div className="notice notice-warn">
          Key 已在平台创建。本机保存未完成，当前会话仍可导入或复制；请先复制保管。{warning}
        </div>
      )}
      <button className="btn primary block" onClick={onImport}>
        导入到智能体 <span aria-hidden="true">→</span>
      </button>
      <button className="btn ghost block" onClick={onCopy}>
        <CopyIcon size={15} />
        复制 API Key
      </button>
    </Modal>
  )
}
