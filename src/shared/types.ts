export type AgentId =
  | 'claude'
  | 'codex'
  | 'pi'
  | 'opencode'
  | 'grokbuild'
  | 'kimi'
  | 'openclaw'
  | 'gemini'
  | 'claude-desktop'
  | 'hermes'
  | 'mcode'
  | 'omp'
  | 'agy'

export interface AgentMeta {
  id: AgentId
  name: string
  subtitle: string
  color: string
  configHint: string
  installCommand: string
  setupNote?: string
  desktop?: boolean
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
  launchCommand?: string | null
  id: AgentId
  cliPath?: string | null
  detectionNote?: string
  installed: boolean
  version: string | null
  configPath: string
  connected: boolean
  appliedModel: string | null
  appliedEffort?: string | null
  keyLabel?: string | null
  keyId?: number | null
  verifiedAt?: number
  compatibility?: boolean
  keyDeleted?: boolean
  configChanged?: boolean
  backupAvailable: boolean
  error?: string
}

export interface AgentOptions {
  piApi: 'openai-responses' | 'openai-completions'
  codexWireApi: 'responses' | 'chat'
  /** auto = 不写 model_reasoning_effort，交给 Codex 自己决定 */
  codexReasoningEffort: string
  agyModelName?: string
}

export interface AppState {
  version: string
  platform?: string
  selectedKeyId?: number | null
  lastAgent?: string
  legacyData?: boolean
  compatibilityReady?: boolean
  compatibilityError?: string | null
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
  agentModels: Partial<Record<AgentId, string>>
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
  agyModelName?: string
}

export interface RendererApi {
  sessions: {
    list(query?: string, agent?: string): Promise<SessionIndex>
    read(id: string): Promise<SessionDetail>
  }
  updates: UpdatesApi
  getState(): Promise<AppState>
  setCliPath(agent: AgentId, path: string): Promise<AppState>
  refresh(): Promise<AppState>
  keys(): Promise<ApiKeyInfo[]>
  getKey(id: number): Promise<string>
  copyKey(id: number): Promise<void>
  prepareKey(id: number): Promise<RelayProbe>
  saveKey(input: {
    apiKey: string
    name?: string
  }): Promise<{ probe: RelayProbe; state: AppState; info: ApiKeyInfo }>
  deleteKey(id: number): Promise<{ state: AppState; warning?: string | null }>
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

export interface LocalSession {
  id: string
  agent: string
  sessionId: string
  title: string
  project: string
  modifiedAt: number
  archived: boolean
  preview: string
}
export interface SessionIndex {
  sessions: LocalSession[]
  sources: Array<{ agent: string; count: number; status: string; note: string }>
  truncated: boolean
}
export interface SessionDetail {
  session: LocalSession
  messages: Array<{ role: string; text: string }>
  resumeCommand: string | null
  shell: string
  truncated: boolean
  note: string
}

export type UpdatePhase =
  'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'ready' | 'installing' | 'error'
export interface UpdateStatus {
  phase: UpdatePhase
  currentVersion: string
  downloadUrl?: string | null
  version: string | null
  notes: string | null
  downloaded: number
  total: number | null
  canInstall: boolean
  format: string
  error: string | null
}
export interface UpdatesApi {
  status(): Promise<UpdateStatus>
  check(): Promise<UpdateStatus>
  download(): Promise<UpdateStatus>
  install(confirmed: boolean): Promise<UpdateStatus>
  onStatus(callback: (status: UpdateStatus) => void): Promise<() => void>
}
