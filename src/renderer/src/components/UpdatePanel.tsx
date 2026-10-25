import { useState } from 'react'
import type { UpdateController } from '../updates'
import { LoaderIcon, RefreshIcon } from './Icons'

export function UpdatePanel({ update, version }: { update: UpdateController; version: string }) {
  const [confirmed, setConfirmed] = useState(false)
  const [linkError, setLinkError] = useState('')
  const { status, pending } = update
  const working = pending || ['checking', 'downloading', 'installing'].includes(status.phase)
  const available = ['available', 'downloading', 'ready', 'installing'].includes(status.phase)
  const percent = status.total
    ? Math.min(100, Math.floor((status.downloaded / status.total) * 100))
    : null
  async function openDownload(
    url = 'https://github.com/simonyuann/aihubmix-switch/releases/latest'
  ) {
    try {
      await window.api.openExternal(url)
    } catch {
      setLinkError(
        '无法打开浏览器，请访问 github.com/simonyuann/aihubmix-switch/releases/latest 下载。'
      )
    }
  }
  return (
    <section className="settings-section update-section" aria-label="软件更新">
      <div className="update-heading">
        <div>
          <h3>软件更新</h3>
          <p className="fine-print">当前版本 v{status.currentVersion || version}</p>
        </div>
        <button
          className="btn sm"
          disabled={working || status.phase === 'ready'}
          onClick={() => void update.check()}
        >
          {status.phase === 'checking' ? <LoaderIcon size={14} /> : <RefreshIcon size={14} />}
          {status.phase === 'checking' ? '正在检查…' : '检查更新'}
        </button>
      </div>
      {status.phase === 'current' && (
        <p className="update-current" role="status">
          已是最新版本
        </p>
      )}
      {available && (
        <div className="update-card">
          <strong>
            {status.phase === 'ready'
              ? '更新已下载并通过签名校验'
              : `发现新版本 v${status.version}`}
          </strong>
          {status.notes && <div className="update-notes">{status.notes}</div>}
          {status.phase === 'downloading' && (
            <div role="status">
              <progress
                aria-label="更新下载进度"
                max={status.total || undefined}
                value={status.total ? status.downloaded : undefined}
              />
              <p className="fine-print">
                正在下载{percent === null ? '…' : ` ${percent}%`} ·{' '}
                {(status.downloaded / 1024 / 1024).toFixed(1)} MB
              </p>
            </div>
          )}
          {status.phase === 'available' &&
            (status.canInstall ? (
              <button
                className="btn primary sm"
                disabled={working}
                onClick={() => void update.download()}
              >
                下载更新
              </button>
            ) : (
              <>
                <p className="fine-print">
                  此运行方式采用下载覆盖升级，无需卸载。下载完成后退出旧程序，用新文件替换。
                </p>
                <button
                  className="btn primary sm"
                  onClick={() => void openDownload(status.downloadUrl || undefined)}
                >
                  下载覆盖版本
                </button>
              </>
            ))}
          {status.phase === 'ready' && (
            <div className="update-confirm">
              <label>
                <input
                  type="checkbox"
                  checked={confirmed}
                  disabled={working}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                我已准备好重启应用，本地兼容连接会短暂断开。
              </label>
              <button
                className="btn primary sm"
                disabled={!confirmed || working}
                onClick={() => void update.install()}
              >
                安装并重启
              </button>
              <p className="fine-print">
                API Key、登录信息、配置与备份保留。Linux deb 可能弹出系统授权窗口。
              </p>
            </div>
          )}
          {status.phase === 'installing' && (
            <p role="status">
              <LoaderIcon size={15} /> 正在安装更新，请稍候…
            </p>
          )}
        </div>
      )}
      {linkError && (
        <div className="notice notice-error" role="alert">
          {linkError}
        </div>
      )}
      {status.error && (
        <div className="notice notice-error" role="alert">
          {status.error}
        </div>
      )}
      {status.phase === 'error' && (
        <button className="link-btn" onClick={() => void openDownload()}>
          打开官网下载页
        </button>
      )}
    </section>
  )
}
