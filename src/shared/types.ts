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
  codexHistory?: CodexHistoryStatus
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
  /** 旧版调用可指定；新版由后端从当前 Key 的可用列表中自动选择。 */
  model?: string
  effort?: string
  agyModelName?: string
}

export interface RechargeMethod {
  id: string
  name: string
  minAmount: number
  maxAmount: number
  feeRate: number
}
export interface RechargeOrder {
  id: number
  amount: number | null
  payAmount: number | null
  bonusAmount: number | null
  currency: string
  method: string
  status: string
  createdAt: string
  expiresAt: string
  tradeNo: string
  qrCode?: string
  payUrl?: string
}
export interface RechargeOverview {
  enabled: boolean
  email: string
  balance: number | null
  currency: string
  minAmount: number
  maxAmount: number
  methods: RechargeMethod[]
  orders: RechargeOrder[]
}
export interface WalletBalance {
  email: string
  balance: number | null
  currency: string
}
export interface PaymentsApi {
  overview(): Promise<RechargeOverview>
  balance(): Promise<WalletBalance>
  create(input: { amount: string; method: string }): Promise<RechargeOrder>
  order(id: number): Promise<RechargeOrder>
  verify(id: number): Promise<RechargeOrder>
  cancel(id: number): Promise<RechargeOrder>
}

export interface RendererApi {
  payments: PaymentsApi
  diagnoseAgent(agent: AgentId): Promise<ConnectionReport>
  codexHistory: {
    change(input: {
      enabled: boolean
      migrate?: boolean
      restore?: boolean
    }): Promise<{ files: number; rows: number; history: CodexHistoryStatus }>
    recover(): Promise<CodexHistoryStatus>
  }
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

export interface ConnectionReport {
  agent: AgentId
  ok: boolean
  checkedAt: number
  latencyMs: number
  model: string
  protocol: string
  checks: Array<{ name: string; status: 'ok' | 'error' | 'info'; detail: string }>
}

export interface CodexHistoryStatus {
  enabled: boolean
  backupAvailable: boolean
  recoveryRequired: boolean
  directory: string
  error: string | null
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
