import { useState } from 'react'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { AgentState, ConnectionReport } from '../../../shared/types'
import { AGENTS } from '../../../shared/constants'
import { Modal } from './Modal'
import { CheckIcon, CopyIcon, LoaderIcon } from './Icons'

export function DiagnosticsModal({
  agent,
  onClose,
  notify
}: {
  agent: AgentState
  onClose: () => void
  notify: (message: string, kind?: 'success' | 'error' | 'info') => void
}) {
  const [report, setReport] = useState<ConnectionReport | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  async function run() {
    setBusy(true)
    setError('')
    setReport(null)
    try {
      setReport(await window.api.diagnoseAgent(agent.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }
  async function copy() {
    if (!report) return
    try {
      const text = [
        `AIHubMix Switch · ${AGENTS[agent.id].name}`,
        `模型：${report.model}`,
        `协议：${report.protocol || '尚未请求'}`,
        `时间：${new Date(report.checkedAt * 1000).toISOString()}`,
        `模型验证耗时：${report.latencyMs} ms`,
        ...report.checks.map(
          (check) =>
            `${check.status === 'ok' ? '通过' : check.status === 'error' ? '失败' : '提示'} · ${check.name}：${check.detail}`
        )
      ].join('\n')
      await writeText(text)
      notify('诊断报告已复制')
    } catch {
      notify('复制失败，请重试', 'error')
    }
  }
  return (
    <Modal
      open
      title={`${AGENTS[agent.id].name} 连接诊断`}
      subtitle="检查当前配置与模型调用，原配置保持不变。"
      onClose={onClose}
      busy={busy}
      width={540}
    >
      <div className="diagnostic-summary">
        <strong>{agent.appliedModel || '未指定模型'}</strong>
        <p>
          诊断会发送两次简短模型请求，可能消耗少量额度。报告不包含 API Key、原始配置或会话内容。
        </p>
      </div>
      {report && (
        <div className="diagnostic-results" aria-label="诊断结果">
          <div className={`notice ${report.ok ? 'notice-ok' : 'notice-error'}`}>
            <strong>{report.ok ? '模型连接验证通过' : '发现需要处理的问题'}</strong>
            {report.protocol && (
              <span>
                {report.protocol} · {report.latencyMs} ms
              </span>
            )}
          </div>
          {report.checks.map((check, index) => (
            <div className="diagnostic-check" key={index}>
              <span className={`diagnostic-marker ${check.status}`} aria-hidden="true">
                {check.status === 'ok' ? '✓' : check.status === 'error' ? '!' : '·'}
              </span>
              <div>
                <strong>{check.name}</strong>
                <p>{check.detail}</p>
              </div>
            </div>
          ))}
          <p className="fine-print">
            这是本次请求的检查结果，原工具的版本、运行环境与其他功能需在工具中确认。
          </p>
        </div>
      )}
      {error && (
        <div className="notice notice-error" role="alert">
          {error}
        </div>
      )}
      <div className="diagnostic-actions">
        {report && (
          <button className="btn" onClick={() => void copy()}>
            <CopyIcon size={14} />
            复制报告
          </button>
        )}
        <button className="btn primary" disabled={busy} onClick={() => void run()}>
          {busy ? <LoaderIcon size={15} /> : <CheckIcon size={15} />}
          {busy ? '正在检查配置与模型…' : report ? '重新诊断' : '开始诊断'}
        </button>
      </div>
    </Modal>
  )
}
