import fs from 'node:fs'
import path from 'node:path'
import type { AgentId } from '../../shared/types'
import type { AgentContext } from './types'
import { ensureDir, exists } from './fs-utils'

export interface BackupFile {
  path: string
  existed: boolean
  storedAs?: string
}

export interface BackupMeta {
  id: string
  agent: AgentId
  createdAt: string
  files: BackupFile[]
  previous?: Record<string, unknown>
}

export function backupsRoot(ctx: AgentContext): string {
  return path.join(ctx.userDataDir, 'backups')
}

export function agentBackupDir(ctx: AgentContext, agent: AgentId): string {
  return path.join(backupsRoot(ctx), agent)
}

export function createBackup(
  ctx: AgentContext,
  agent: AgentId,
  files: string[],
  previous?: Record<string, unknown>
): BackupMeta {
  const now = new Date()
  const id = `${now.toISOString().replace(/[:.]/g, '-')}-${Math.random()
    .toString(36)
    .slice(2, 6)}`
  const dir = path.join(agentBackupDir(ctx, agent), id)
  ensureDir(dir)

  const records: BackupFile[] = files.map((file, index) => {
    const existed = exists(file)
    let storedAs: string | undefined
    if (existed) {
      storedAs = `${index}-${path.basename(file)}`
      const target = path.join(dir, storedAs)
      fs.copyFileSync(file, target)
      try {
        fs.chmodSync(target, 0o600)
      } catch {
        // Windows 忽略
      }
    }
    return { path: file, existed, storedAs }
  })

  const meta: BackupMeta = {
    id,
    agent,
    createdAt: now.toISOString(),
    files: records,
    previous
  }
  fs.writeFileSync(path.join(dir, 'meta.json'), JSON.stringify(meta, null, 2), { mode: 0o600 })
  pruneBackups(ctx, agent)
  return meta
}

export function saveBackupMeta(ctx: AgentContext, meta: BackupMeta): void {
  const dir = path.join(agentBackupDir(ctx, meta.agent), meta.id)
  ensureDir(dir)
  fs.writeFileSync(path.join(dir, 'meta.json'), JSON.stringify(meta, null, 2), { mode: 0o600 })
}

export function listBackups(ctx: AgentContext, agent: AgentId): BackupMeta[] {
  const dir = agentBackupDir(ctx, agent)
  if (!exists(dir)) return []
  const metas: BackupMeta[] = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    try {
      const meta = JSON.parse(
        fs.readFileSync(path.join(dir, entry.name, 'meta.json'), 'utf8')
      ) as BackupMeta
      if (meta && meta.id) metas.push(meta)
    } catch {
      // 跳过损坏的备份
    }
  }
  return metas.sort((a, b) => b.createdAt.localeCompare(a.createdAt))
}

export function latestBackup(ctx: AgentContext, agent: AgentId): BackupMeta | null {
  return listBackups(ctx, agent)[0] || null
}

export function hasBackup(ctx: AgentContext, agent: AgentId): boolean {
  return listBackups(ctx, agent).length > 0
}

export function restoreBackup(ctx: AgentContext, meta: BackupMeta): void {
  const dir = path.join(agentBackupDir(ctx, meta.agent), meta.id)
  for (const file of meta.files) {
    if (file.existed && file.storedAs) {
      const stored = path.join(dir, file.storedAs)
      if (!exists(stored)) continue
      ensureDir(path.dirname(file.path))
      fs.copyFileSync(stored, file.path)
      try {
        fs.chmodSync(file.path, 0o600)
      } catch {
        // 忽略
      }
    } else {
      try {
        fs.unlinkSync(file.path)
      } catch {
        // 忽略
      }
    }
  }
}

function pruneBackups(ctx: AgentContext, agent: AgentId): void {
  const dir = agentBackupDir(ctx, agent)
  const backups = listBackups(ctx, agent)
  for (const meta of backups.slice(20)) {
    try {
      fs.rmSync(path.join(dir, meta.id), { recursive: true, force: true })
    } catch {
      // 忽略
    }
  }
}
