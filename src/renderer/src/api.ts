import { invoke, isTauri } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { RendererApi } from '../../shared/types'

declare global {
  interface Window {
    api: RendererApi
  }
}

async function request<T>(command: string, payload?: unknown): Promise<T> {
  if (!isTauri()) throw new Error('请使用桌面客户端运行。开发预览可在地址后添加 ?preview=1。')
  try {
    return await invoke<T>('dispatch', { command, payload: payload ?? null })
  } catch (error) {
    throw new Error(
      typeof error === 'string'
        ? error
        : error instanceof Error
          ? error.message
          : '操作失败，请重试'
    )
  }
}

export const api: RendererApi = {
  getState: () => request('state'),
  refresh: () => request('refresh'),
  keys: () => request('keys'),
  getKey: (id) => request('get-key', { id }),
  copyKey: async (id) => {
    await writeText(await request<string>('get-key', { id }))
  },
  prepareKey: (id) => request('prepare-key', { id }),
  saveKey: (input) => request('save-key', input),
  deleteKey: (id) => request('delete-key', { id }),
  removeLocalKey: (id) => request('remove-local-key', { id }),
  importAgent: (input) => request('apply', input),
  restoreAgent: (agent) => request('restore', { agent }),
  updateOptions: (patch) => request('options', patch),
  openExternal: (url) => request('open-external', { url }),
  openPath: (target) => request('open-path', { target }),
  account: {
    login: (input) => request('login', input),
    register: (input) => request('register', input),
    logout: () => request('logout'),
    groups: () => request('groups'),
    createKey: (input) => request('create-key', input)
  }
}
