import { contextBridge, ipcRenderer } from 'electron'
import type { IpcResponse, RendererApi } from '../shared/types'

async function invoke<T>(channel: string, ...args: unknown[]): Promise<T> {
  const result = (await ipcRenderer.invoke(channel, ...args)) as IpcResponse<T>
  if (!result || typeof result !== 'object' || !('ok' in result)) {
    throw new Error('应用内部通信失败，请重启后重试')
  }
  if (!result.ok) throw new Error(result.error)
  return result.data
}

const api: RendererApi = {
  getState: () => invoke('app:get-state'),
  refresh: (forceAgents) => invoke('app:refresh', Boolean(forceAgents)),
  testRelay: (input) => invoke('relay:test', input),
  saveKey: (input) => invoke('key:save', input),
  clearKey: () => invoke('key:clear'),
  setAgentModel: (agent, model) => invoke('agent:set-model', agent, model),
  updateOptions: (patch) => invoke('options:update', patch),
  applyAgent: (input) => invoke('agent:apply', input),
  applyAll: () => invoke('agent:apply-all'),
  restoreAgent: (agent) => invoke('agent:restore', agent),
  openExternal: (url) => invoke('shell:open-external', url),
  openPath: (target) => invoke('shell:open-path', target),
  account: {
    register: (input) => invoke('account:register', input),
    login: (input) => invoke('account:login', input),
    logout: () => invoke('account:logout'),
    groups: () => invoke('account:groups'),
    keys: () => invoke('account:keys'),
    createKey: (input) => invoke('account:create-key', input),
    useKey: (id) => invoke('account:use-key', id)
  }
}

contextBridge.exposeInMainWorld('api', api)
