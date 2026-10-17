import { useState } from 'react'
import type { AppState } from '../../../shared/types'
import { Modal, LinkButton } from './Modal'
import type { ToastKind } from './Toast'
import { FolderIcon, LoaderIcon, SettingsIcon, ShieldIcon } from './Icons'

interface Props {
  open: boolean
  onClose: () => void
  state: AppState
  onStateChange: (state: AppState) => void
  notify: (message: string, kind?: ToastKind) => void
}

export function SettingsModal({ open, onClose, state, onStateChange, notify }: Props) {
  const [busy, setBusy] = useState<string | null>(null)

  async function updateOptions(patch: Parameters<typeof window.api.updateOptions>[0]) {
    setBusy('options')
    try {
      const next = await window.api.updateOptions(patch)
      onStateChange(next)
      notify('设置已保存，重新接入后生效')
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    } finally {
      setBusy(null)
    }
  }

  async function openPath(target: 'userData' | 'backups' | 'claude' | 'codex' | 'pi' | 'opencode') {
    try {
      await window.api.openPath(target)
    } catch (err) {
      notify(err instanceof Error ? err.message : String(err), 'error')
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      width={620}
      title="设置"
      subtitle="高级选项改变后，需要重新点击对应 Agent 的「接入」才会写入"
    >
      <div className="settings-box">
        <section className="settings-section">
          <h3>
            <SettingsIcon size={15} />
            服务地址
          </h3>
          <div className="readonly-row">
            <code>{state.relayBaseUrl}</code>
            <span className="chip ok">已固定</span>
          </div>
          <p className="fine-print">
            客户端默认连接 AIHubMix 中转站。如需更换，请重新粘贴新站点的 API Key 并保存。
          </p>
        </section>

        <section className="settings-section">
          <h3>接口格式</h3>
          <div className="option-row">
            <div>
              <strong>Pi 接口</strong>
              <p>默认使用 Responses API；如遇兼容问题可切换为 Chat Completions</p>
            </div>
            <div className="segmented small">
              <button
                className={state.options.piApi === 'openai-responses' ? 'active' : ''}
                disabled={busy === 'options'}
                onClick={() => updateOptions({ piApi: 'openai-responses' })}
              >
                Responses
              </button>
              <button
                className={state.options.piApi === 'openai-completions' ? 'active' : ''}
                disabled={busy === 'options'}
                onClick={() => updateOptions({ piApi: 'openai-completions' })}
              >
                Completions
              </button>
            </div>
          </div>
          <div className="option-row">
            <div>
              <strong>Codex 接口</strong>
              <p>默认 Responses；仅当服务端不支持 /v1/responses 时切换</p>
            </div>
            <div className="segmented small">
              <button
                className={state.options.codexWireApi === 'responses' ? 'active' : ''}
                disabled={busy === 'options'}
                onClick={() => updateOptions({ codexWireApi: 'responses' })}
              >
                Responses
              </button>
              <button
                className={state.options.codexWireApi === 'chat' ? 'active' : ''}
                disabled={busy === 'options'}
                onClick={() => updateOptions({ codexWireApi: 'chat' })}
              >
                Chat
              </button>
            </div>
          </div>
          {busy === 'options' ? (
            <div className="notice compact">
              <LoaderIcon size={14} /> 正在保存…
            </div>
          ) : null}
        </section>

        <section className="settings-section">
          <h3>
            <FolderIcon size={15} />
            配置与备份
          </h3>
          <div className="folder-grid">
            <button className="btn sm" onClick={() => openPath('backups')}>
              <FolderIcon size={13} />
              打开备份目录
            </button>
            <button className="btn sm" onClick={() => openPath('userData')}>
              <FolderIcon size={13} />
              打开数据目录
            </button>
          </div>
          <div className="folder-grid two">
            <button className="btn sm" onClick={() => openPath('codex')}>
              Codex 目录
            </button>
            <button className="btn sm" onClick={() => openPath('pi')}>
              Pi 目录
            </button>
            <button className="btn sm" onClick={() => openPath('opencode')}>
              OpenCode 目录
            </button>
            <button className="btn sm" onClick={() => openPath('claude')}>
              Claude 目录
            </button>
          </div>
        </section>

        <section className="settings-section">
          <h3>
            <ShieldIcon size={15} />
            安全
          </h3>
          <p className="fine-print">
            API Key 使用系统安全存储加密保存在本机；写入 Agent 配置时才会出现在对应配置文件中。
            {state.secureStorage ? '当前系统安全存储可用。' : '当前系统安全存储不可用，Key 仅做本地编码。'}
          </p>
        </section>

        <section className="settings-section about">
          <div>
            <strong>AIHubMix 接入助手</strong>
            <span>v{state.version}</span>
          </div>
          <div className="about-links">
            <LinkButton href={state.site.apiBaseUrl}>官网</LinkButton>
            <LinkButton href={state.site.docUrl}>使用文档</LinkButton>
          </div>
        </section>
      </div>
    </Modal>
  )
}
