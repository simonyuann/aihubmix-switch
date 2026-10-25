export type AgentId = 'claude' | 'codex' | 'pi' | 'opencode'

export interface AgentMeta {
  id: AgentId
  name: string
  subtitle: string
  color: string
  configHint: string
  installCommand: string
}

export interface RelayModel {
  id: string
  name: string
  ownedBy?: string
  contextWindow?: number
  maxTokens?: number
  reasoning?: boolean
  reasoningLevels?: string[]
  defaultReasoningLevel?: string
  input?: string[]
}

export interface SiteInfo {
  name: string
  subtitle: string
  apiBaseUrl: string
  docUrl: string
  registerUrl: string
  loginUrl: string
}

export type MessageCapability = 'supported' | 'unsupported' | 'unknown'

export interface RelayProbe {
  ok: boolean
  baseUrl: string
  models: RelayModel[]
  balance: number | null
  rateMultiplier: number | null
  messages: MessageCapability
  latencyMs: number
  error?: string
}

export interface AccountState {
  loggedIn: boolean
  email?: string
  userId?: number
}

export interface GroupInfo {
  id: number
  name: string
  description: string
  platform: string
  rateMultiplier: number
  allowMessagesDispatch: boolean
  subscriptionType: string
}

export interface ApiKeyInfo {
  id: number
  name: string
  keyMasked: string
  groupId: number | null
  groupName: string
  status: string
  quotaUsed: number | null
  quota: number | null
  createdAt: string
  local?: boolean
}

export interface AgentState {
  id: AgentId
  installed: boolean
  version: string | null
  configPath: string
  connected: boolean
  appliedModel: string | null
  appliedEffort?: string | null
  keyLabel?: string | null
  keyId?: number | null
  configChanged?: boolean
  backupAvailable: boolean
  error?: string
}

export interface AgentOptions {
  piApi: 'openai-responses' | 'openai-completions'
  codexWireApi: 'responses' | 'chat'
  /** auto = 不写 model_reasoning_effort，交给 Codex 自己决定 */
  codexReasoningEffort: string
}

export interface AppState {
  version: string
  selectedKeyId?: number | null
  lastAgent?: string
  legacyData?: boolean
  storageError?: string | null
  secureStorage: boolean
  site: SiteInfo
  relayBaseUrl: string
  hasApiKey: boolean
  keyPreview: string
  account: AccountState
  models: RelayModel[]
  balance: number | null
  rateMultiplier: number | null
  messages: MessageCapability
  relayError: string | null
  agents: AgentState[]
  agentModels: Record<AgentId, string>
  options: AgentOptions
}

export type IpcResponse<T> = { ok: true; data: T } | { ok: false; error: string }

export interface ApplyAgentInput {
  agent: AgentId
  model: string
}

export interface ApplyAgentResult {
  agent: AgentId
  configPath: string
  backupId?: string
  message: string
}

export interface RestoreResult {
  agent: AgentId
  message: string
}

export interface ApplyAllResult {
  results: ApplyAgentResult[]
  errors: Array<{ agent: AgentId; message: string }>
}

export interface TestRelayInput {
  baseUrl: string
  apiKey: string
}

export interface SaveKeyInput extends TestRelayInput {}

export interface AccountAuthInput {
  email: string
  password: string
  invitationCode?: string
}

export interface CreateKeyInput {
  groupId: number
  name?: string
}

export interface CreatedKey {
  info: ApiKeyInfo
  state: AppState
  warning?: string | null
}

export interface IpcErrorShape {
  message: string
  code?: string
}

export interface ImportInput {
  keyId: number
  keyLabel: string
  agent: AgentId
  model: string
  effort?: string
}

export interface RendererApi {
  getState(): Promise<AppState>
  refresh(): Promise<AppState>
  keys(): Promise<ApiKeyInfo[]>
  getKey(id: number): Promise<string>
  copyKey(id: number): Promise<void>
  prepareKey(id: number): Promise<RelayProbe>
  saveKey(input: {
    apiKey: string
    name?: string
  }): Promise<{ probe: RelayProbe; state: AppState; info: ApiKeyInfo }>
  removeLocalKey(id: number): Promise<AppState>
  importAgent(input: ImportInput): Promise<AppState>
  restoreAgent(agent: AgentId): Promise<AppState>
  updateOptions(patch: Partial<AgentOptions>): Promise<AppState>
  openExternal(url: string): Promise<void>
  openPath(target: 'userData' | 'backups' | AgentId): Promise<void>
  account: {
    register(input: AccountAuthInput): Promise<AppState>
    login(input: AccountAuthInput): Promise<AppState>
    logout(): Promise<AppState>
    groups(): Promise<GroupInfo[]>
    createKey(input: CreateKeyInput): Promise<CreatedKey>
  }
}
