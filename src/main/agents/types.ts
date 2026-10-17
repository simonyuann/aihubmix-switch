import type { AgentId, AgentOptions, AgentState, RelayModel } from '../../shared/types'

export interface AgentContext {
  home: string
  userDataDir: string
  env: NodeJS.ProcessEnv
  platform: NodeJS.Platform
  baseUrl: string
  apiKey: string
  options: AgentOptions
}

export interface ApplyOptions {
  model: string
  models: RelayModel[]
}

export interface ApplyResult {
  configPath: string
  backupId?: string
  message: string
}

export interface AgentAdapter {
  id: AgentId
  configPath(ctx: AgentContext): string
  apply(ctx: AgentContext, input: ApplyOptions): ApplyResult
  restore(ctx: AgentContext): string
  status(ctx: AgentContext): Promise<Omit<AgentState, 'id' | 'installed' | 'version' | 'backupAvailable'>>
}
