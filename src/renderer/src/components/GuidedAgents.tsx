import { useEffect, useState } from 'react'
import { openaiBaseUrl } from '../../../shared/url'

// Guided entries are intentionally separate from native configuration adapters.
const guides = [
  {
    name: 'Cursor',
    url: 'https://docs.cursor.com/settings/api-keys',
    steps: [
      '打开 Settings → Models → API Keys，启用 OpenAI Key 并设置自定义 Base URL。',
      '填入当前 Key 和平台提供的模型 ID；部分 Cursor 专有功能仍需要其官方订阅。'
    ]
  },
  {
    name: 'Windsurf',
    url: 'https://docs.windsurf.com/windsurf/models',
    steps: [
      '在模型设置中查看当前版本的 BYOK 供应商与模型范围。',
      '仅在支持自定义供应商和地址时填写平台 Key；BYOK 不等于支持任意中转模型。'
    ]
  },
  {
    name: 'Cline',
    url: 'https://docs.cline.bot/provider-config/openai-compatible',
    steps: [
      '在 API Provider 中选择 OpenAI Compatible。',
      '填写 Base URL、API Key 和 Model ID，再按模型能力设置上下文长度与工具支持。'
    ]
  },
  {
    name: 'Roo Code',
    url: 'https://docs.roocode.com/providers/openai-compatible',
    steps: [
      '打开 Providers，选择 OpenAI Compatible。',
      '填写 Base URL、API Key、Model ID 并保存；在插件内测试实际工具调用。'
    ]
  },
  {
    name: 'Continue',
    url: 'https://docs.continue.dev/customize/model-providers/top-level/openai',
    steps: [
      '在模型配置中选择 openai provider，设置 apiBase、apiKey 和 model。',
      '保存后重新加载配置，按官方文档选择 chat / edit 等用途。'
    ]
  },
  {
    name: 'Aider',
    url: 'https://aider.chat/docs/llms/openai-compat.html',
    steps: [
      '按照官方 OpenAI Compatible 文档设置 OPENAI_API_BASE 和 OPENAI_API_KEY。',
      '启动时选择 openai/<模型 ID>；模型仍需具备代码编辑所需能力。'
    ]
  },
  {
    name: '其他 OpenAI 兼容工具',
    url: '',
    steps: [
      '在原工具选择 OpenAI Compatible / 自定义供应商。',
      '填写下面的 API 地址、当前 Key 和可用模型 ID。是否可用取决于原工具支持的协议和工具调用能力。'
    ]
  }
]

export function GuidedAgents({
  query,
  baseUrl,
  keyId,
  onCopy,
  onSelect,
  nativeAgent,
  models
}: {
  query: string
  baseUrl: string
  keyId: number
  onCopy: (text: string) => Promise<void>
  onSelect: () => void
  nativeAgent: string
  models: string[]
}) {
  const [selected, setSelected] = useState('')
  const [model, setModel] = useState('')
  useEffect(() => {
    if (nativeAgent) setSelected('')
  }, [nativeAgent])
  const [error, setError] = useState('')
  const visible = guides.filter((g) => g.name.toLowerCase().includes(query.toLowerCase()))
  const guide = guides.find((g) => g.name === selected)
  const address = openaiBaseUrl(baseUrl)
  const configurable = guide && guide.name !== 'Windsurf'
  return (
    <details className="guided-tools" open={query ? true : undefined}>
      <summary>更多工具 · 手动接入说明</summary>
      <div className="guide-list">
        {visible.length ? (
          visible.map((g) => (
            <button
              key={g.name}
              className={selected === g.name ? 'active' : ''}
              type="button"
              onClick={() => {
                setSelected(g.name)
                setError('')
                onSelect()
              }}
            >
              {g.name}
            </button>
          ))
        ) : (
          <span className="fine-print">没有匹配的手动接入工具</span>
        )}
      </div>
      {guide && (
        <div className="guide-card">
          <h4>
            {guide.name} <span className="badge">手动配置</span>
          </h4>
          <ol>
            {guide.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
          {configurable && (
            <p>
              API 地址：<code>{address}</code>
            </p>
          )}
          {configurable && (
            <label className="field">
              <span>手动接入模型</span>
              <select value={model} onChange={(e) => setModel(e.target.value)}>
                <option value="">选择当前 Key 可用的模型</option>
                {models.map((id) => (
                  <option key={id} value={id}>
                    {id}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="button-row">
            {configurable && (
              <>
                <button
                  className="btn sm"
                  onClick={() => void onCopy(address).catch(() => setError('复制地址失败'))}
                >
                  复制地址
                </button>
                <button
                  className="btn sm"
                  disabled={!model || !models.includes(model)}
                  onClick={() => void onCopy(model).catch(() => setError('复制模型失败'))}
                >
                  复制模型 ID
                </button>
                <button
                  className="btn sm"
                  onClick={() =>
                    void window.api
                      .copyKey(keyId)
                      .then(() => setError('Key 已复制，请在原工具中粘贴'))
                      .catch(() => setError('复制 Key 失败，请返回查看 Key'))
                  }
                >
                  复制当前 Key
                </button>
              </>
            )}
            {guide.url && (
              <button
                className="link-btn"
                onClick={() =>
                  void window.api
                    .openExternal(guide.url)
                    .catch(() => setError('无法打开文档，请稍后重试'))
                }
              >
                官方说明 ↗
              </button>
            )}
          </div>
          <p className="fine-print">此说明不会写入配置，也不代表已通过该工具的模型验证。</p>
          {error && <p role="status">{error}</p>}
        </div>
      )}
    </details>
  )
}
