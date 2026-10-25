import type { ApiKeyInfo } from '../../../shared/types'
import { CopyIcon, EyeIcon, KeyIcon, PlusIcon, RefreshIcon, ShieldIcon, TrashIcon } from './Icons'

export function ApiList({
  items,
  loading,
  error,
  onRefresh,
  onCreate,
  onAdd,
  onDetails,
  onCopy,
  onImport,
  onDelete
}: {
  items: ApiKeyInfo[]
  loading: boolean
  error: string
  onRefresh: () => void
  onCreate: () => void
  onAdd: () => void
  onDetails: (item: ApiKeyInfo) => void
  onCopy: (id: number) => void
  onImport: (item: ApiKeyInfo) => void
  onDelete: (item: ApiKeyInfo) => void
}) {
  const status: Record<string, string> = {
    active: '正常',
    inactive: '已停用',
    expired: '已过期',
    quota_exhausted: '额度用尽'
  }
  return (
    <section className="api-panel" aria-label="API Key 列表">
      <div className="panel-toolbar">
        <div className="panel-label">
          <KeyIcon size={16} />
          <strong>API 密钥</strong>
          <span className="count">{items.length}</span>
        </div>
        <div className="toolbar-actions">
          <button className="link-btn" onClick={onAdd}>
            <PlusIcon size={14} />
            添加已有 Key
          </button>
          <span className="toolbar-divider" />
          <button
            className="icon-btn"
            aria-label="刷新 API 列表"
            onClick={onRefresh}
            disabled={loading}
          >
            <RefreshIcon size={15} className={loading ? 'spin' : ''} />
          </button>
        </div>
      </div>
      {error && (
        <div className="list-error notice notice-error" role="alert">
          <span>{error}</span>
          <button className="link-btn" disabled={loading} onClick={onRefresh}>
            重试
          </button>
        </div>
      )}
      {loading && !items.length ? (
        <div className="list-loading" role="status">
          <span className="skeleton" />
          <span className="skeleton" />
          <span className="skeleton" />
          <p>正在加载 API Key…</p>
        </div>
      ) : items.length ? (
        <>
          <div className="api-columns" aria-hidden="true">
            <span>名称 / Key</span>
            <span>模型服务</span>
            <span>状态</span>
            <span>操作</span>
          </div>
          <div className="api-rows">
            {items.map((item) => (
              <div className="api-row" key={item.id}>
                <button className="key-identity" onClick={() => onDetails(item)}>
                  <span className="key-symbol">
                    <KeyIcon size={20} />
                  </span>
                  <span>
                    <strong>{item.name}</strong>
                    <code>{item.keyMasked}</code>
                  </span>
                </button>
                <span className="key-service">
                  {item.groupName || (item.local ? '本机 Key' : '—')}
                  {item.local && <small>仅保存在此设备</small>}
                </span>
                <span
                  className={`status-dot ${item.status === 'active' ? 'status-ok' : 'status-warn'}`}
                >
                  {status[item.status] || '状态未知'}
                </span>
                <div className="row-actions">
                  <button
                    className="icon-btn danger-text"
                    aria-label={`${item.local ? '移除' : '删除'} ${item.name}`}
                    title={item.local ? '从本机移除' : '删除 API Key'}
                    onClick={() => onDelete(item)}
                  >
                    <TrashIcon size={16} />
                  </button>
                  <button
                    className="icon-btn"
                    aria-label={`查看 ${item.name}`}
                    title="查看完整 Key 与详情"
                    onClick={() => onDetails(item)}
                  >
                    <EyeIcon size={16} />
                  </button>
                  <button
                    className="icon-btn"
                    aria-label={`复制 ${item.name}`}
                    title="复制 API Key"
                    onClick={() => onCopy(item.id)}
                  >
                    <CopyIcon size={16} />
                  </button>
                  <button
                    className="btn outline-blue sm"
                    disabled={item.status !== 'active'}
                    onClick={() => onImport(item)}
                  >
                    导入<span aria-hidden="true">↗</span>
                  </button>
                </div>
              </div>
            ))}
          </div>
          <div className="panel-bottom">
            <ShieldIcon size={13} />
            <span>Key 默认隐藏，选择后即可导入智能体。</span>
          </div>
        </>
      ) : (
        !error && (
          <div className="empty-state">
            <div className="empty-illustration">
              <div className="empty-key">
                <KeyIcon size={31} />
              </div>
              <span className="empty-spark">+</span>
            </div>
            <h3>从你的第一个 API Key 开始</h3>
            <p>
              创建一个 Key，连接你熟悉的编程工具。
              <br />
              我们会帮你完成配置，并自动保留备份。
            </p>
            <button className="btn primary" onClick={onCreate}>
              <PlusIcon size={16} />
              创建 API Key
            </button>
            <button className="link-btn empty-secondary" onClick={onAdd}>
              我已有 API Key，直接添加
            </button>
          </div>
        )
      )}
    </section>
  )
}
