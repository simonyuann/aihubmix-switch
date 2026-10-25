import { useEffect, useRef, useState } from 'react'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { SessionDetail, SessionIndex } from '../../../shared/types'
import { AGENTS } from '../../../shared/constants'
import { CopyIcon, LoaderIcon, RefreshIcon, TerminalIcon } from './Icons'

const name = (id: string) =>
  id in AGENTS ? AGENTS[id as keyof typeof AGENTS].name : id === 'gemini' ? 'Gemini CLI' : id
const time = (value: number) =>
  new Date(value).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })

export function SessionPage({
  onNotify,
  initialSource = ''
}: {
  onNotify: (message: string, kind?: 'success' | 'error') => void
  initialSource?: string
}) {
  const [index, setIndex] = useState<SessionIndex | null>(null)
  const [query, setQuery] = useState('')
  const [source, setSource] = useState(initialSource)
  const [selected, setSelected] = useState('')
  const [detail, setDetail] = useState<SessionDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [reading, setReading] = useState(false)
  const [error, setError] = useState('')
  const [detailError, setDetailError] = useState('')
  const generation = useRef(0)
  async function refresh() {
    const id = ++generation.current
    setLoading(true)
    setError('')
    try {
      const result = await window.api.sessions.list()
      if (id !== generation.current) return
      setIndex(result)
      setSelected('')
      setDetail(null)
    } catch (err) {
      if (id === generation.current) setError(String(err instanceof Error ? err.message : err))
    } finally {
      if (id === generation.current) setLoading(false)
    }
  }
  useEffect(() => {
    void refresh()
    return () => {
      ++generation.current
    }
  }, [])
  useEffect(() => {
    let cancelled = false
    setDetail(null)
    setDetailError('')
    setReading(Boolean(selected))
    if (selected)
      void window.api.sessions
        .read(selected)
        .then((value) => {
          if (!cancelled) setDetail(value)
        })
        .catch((err) => {
          if (!cancelled) setDetailError(String(err instanceof Error ? err.message : err))
        })
        .finally(() => {
          if (!cancelled) setReading(false)
        })
    return () => {
      cancelled = true
    }
  }, [selected])
  const search = query.trim().toLowerCase()
  const items =
    index?.sessions.filter(
      (item) =>
        (!source || item.agent === source) &&
        (!search ||
          [item.title, item.project, item.preview, item.sessionId]
            .join(' ')
            .toLowerCase()
            .includes(search))
    ) || []
  async function copy() {
    if (!detail?.resumeCommand) return
    try {
      await writeText(detail.resumeCommand)
      onNotify('继续会话命令已复制，请在终端运行')
    } catch {
      onNotify('复制失败，请手动选择命令复制', 'error')
    }
  }
  return (
    <section className="api-panel session-panel" aria-label="本地会话历史">
      <div className="session-toolbar">
        <input
          aria-label="搜索会话"
          placeholder="搜索标题、项目、预览或会话 ID…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          maxLength={256}
        />
        <select aria-label="会话来源" value={source} onChange={(e) => setSource(e.target.value)}>
          <option value="">全部来源</option>
          {index?.sources.map((s) => (
            <option key={s.agent} value={s.agent}>
              {name(s.agent)} · {s.count}
            </option>
          ))}
        </select>
        <button
          className="icon-btn"
          aria-label="刷新会话"
          disabled={loading}
          onClick={() => void refresh()}
        >
          <RefreshIcon size={16} />
        </button>
      </div>
      {error && (
        <div className="notice notice-error" role="alert">
          {error}
          <button className="link-btn" onClick={() => void refresh()}>
            重试
          </button>
        </div>
      )}
      {index && (
        <details className="session-sources">
          <summary>本机只读 · {index.sessions.length} 个会话 · 查看来源状态</summary>
          <div>
            {index.sources.map((s) => (
              <p key={s.agent}>
                <strong>{name(s.agent)}</strong>
                <span>{s.status === 'ready' ? `${s.count} 个会话` : s.note}</span>
              </p>
            ))}
          </div>
          <p className="fine-print">
            目前读取六种工具的本地文件记录。OpenCode
            数据库等格式尚未接入；搜索基于标题、项目与有限长度的预览。
          </p>
        </details>
      )}
      {index?.truncated && (
        <div className="notice">已达到本次扫描上限，部分记录未列出。完整历史请在原工具查看。</div>
      )}
      <div className="session-layout">
        <div className="session-list" aria-label="会话列表" aria-busy={loading}>
          {loading ? (
            <p className="session-placeholder">
              <LoaderIcon size={18} />
              正在读取本地记录…
            </p>
          ) : items.length ? (
            items.map((item) => (
              <button
                key={item.id}
                className={`session-item ${selected === item.id ? 'selected' : ''}`}
                aria-pressed={selected === item.id}
                onClick={() => setSelected(item.id)}
              >
                <span className="session-item-meta">
                  <span>
                    {name(item.agent)}
                    {item.archived ? ' · 已归档' : ''}
                  </span>
                  <time>{time(item.modifiedAt)}</time>
                </span>
                <strong>{item.title}</strong>
                <span className="session-preview">{item.preview || '无文本预览'}</span>
                {item.project && <code title={item.project}>{item.project}</code>}
              </button>
            ))
          ) : (
            <div className="session-placeholder">
              <TerminalIcon size={25} />
              <strong>{query || source ? '没有匹配的会话' : '暂无可读取的会话'}</strong>
              <span>检查来源状态，或在智能体中完成对话后刷新。</span>
            </div>
          )}
        </div>
        <div className="session-reader" aria-label="会话正文" aria-busy={reading}>
          {reading ? (
            <p className="session-placeholder">
              <LoaderIcon size={18} />
              正在读取会话…
            </p>
          ) : detailError ? (
            <p className="notice notice-error" role="alert">
              {detailError}
            </p>
          ) : detail ? (
            <>
              <div className="session-detail-heading">
                <span className="badge">{name(detail.session.agent)} · 只读</span>
                <h3>{detail.session.title}</h3>
                <p>{detail.session.project || '未记录项目目录'}</p>
              </div>
              {detail.resumeCommand ? (
                <div className="session-resume">
                  <div>
                    <strong>在原工具继续</strong>
                    <button className="btn sm" onClick={() => void copy()}>
                      <CopyIcon size={14} />
                      复制继续命令
                    </button>
                  </div>
                  <code>{detail.resumeCommand}</code>
                  <p className="fine-print">
                    粘贴到{detail.shell}运行；实际续聊取决于原工具、登录状态和模型兼容性。
                  </p>
                </div>
              ) : (
                <p className="notice">此来源暂不提供继续命令，请在原工具中打开对应会话。</p>
              )}
              <p className="fine-print">{detail.note}</p>
              <div className="session-messages">
                {detail.messages.length ? (
                  detail.messages.map((message, i) => (
                    <article className={`session-message ${message.role}`} key={i}>
                      <span>{message.role === 'user' ? '你' : '智能体'}</span>
                      <pre>{message.text}</pre>
                    </article>
                  ))
                ) : (
                  <p className="session-placeholder">此记录暂无可展示的文本消息。</p>
                )}
              </div>
            </>
          ) : (
            <div className="session-placeholder">
              <TerminalIcon size={28} />
              <strong>把上下文找回来</strong>
              <span>选择左侧会话，阅读记录或复制继续命令。</span>
            </div>
          )}
        </div>
      </div>
    </section>
  )
}
