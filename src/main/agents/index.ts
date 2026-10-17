import fs from 'node:fs'
import path from 'node:path'
import { spawn } from 'node:child_process'
import type { AgentId, AgentState } from '../../shared/types'
import { AGENT_ORDER } from '../../shared/constants'
import type { AgentContext } from './types'
import { claudeAdapter } from './claude'
import { codexAdapter } from './codex'
import { piAdapter } from './pi'
import { opencodeAdapter } from './opencode'
import { hasBackup } from './backup'

const COMMANDS: Record<AgentId, string> = {
  claude: 'claude',
  codex: 'codex',
  pi: 'pi',
  opencode: 'opencode'
}

export const adapters = {
  claude: claudeAdapter,
  codex: codexAdapter,
  pi: piAdapter,
  opencode: opencodeAdapter
} as const

function executableNames(command: string): string[] {
  if (process.platform !== 'win32') return [command]
  return [`${command}.cmd`, `${command}.exe`, `${command}.bat`, command]
}

function candidateDirs(ctx: AgentContext): string[] {
  const dirs: string[] = []
  const pathValue = ctx.env.PATH || ctx.env.Path || ''
  const separator = process.platform === 'win32' ? ';' : ':'
  for (const dir of pathValue.split(separator)) {
    if (dir.trim()) dirs.push(dir.trim())
  }

  const home = ctx.home
  dirs.push(
    path.join(home, '.local', 'bin'),
    path.join(home, '.npm-global', 'bin'),
    path.join(home, '.bun', 'bin'),
    path.join(home, '.volta', 'bin')
  )

  if (process.platform === 'win32') {
    if (ctx.env.APPDATA) dirs.push(path.join(ctx.env.APPDATA, 'npm'))
    if (ctx.env.LOCALAPPDATA) dirs.push(path.join(ctx.env.LOCALAPPDATA, 'Programs', 'nodejs'))
    if (ctx.env.ProgramFiles) dirs.push(path.join(ctx.env.ProgramFiles, 'nodejs'))
  } else {
    dirs.push('/opt/homebrew/bin', '/usr/local/bin', '/usr/bin')
    const nvm = path.join(home, '.nvm', 'versions', 'node')
    try {
      for (const version of fs.readdirSync(nvm)) {
        dirs.push(path.join(nvm, version, 'bin'))
      }
    } catch {
      // 没有 nvm 时忽略
    }
  }

  return [...new Set(dirs)]
}

function findExecutable(command: string, ctx: AgentContext): string | null {
  for (const dir of candidateDirs(ctx)) {
    for (const name of executableNames(command)) {
      const full = path.join(dir, name)
      try {
        const stat = fs.statSync(full)
        if (stat.isFile()) return full
      } catch {
        // 继续查找
      }
    }
  }
  return null
}

function readVersion(executable: string): Promise<string | null> {
  return new Promise((resolve) => {
    let output = ''
    let child
    const isWindowsScript =
      process.platform === 'win32' && /\.(cmd|bat)$/i.test(executable)
    try {
      child = isWindowsScript
        ? spawn(
            process.env.ComSpec || 'cmd.exe',
            ['/d', '/s', '/c', `"${executable}" --version`],
            { windowsHide: true, timeout: 5000 }
          )
        : spawn(executable, ['--version'], { windowsHide: true, timeout: 5000 })
    } catch {
      resolve(null)
      return
    }

    child.stdout?.on('data', (chunk) => {
      output += String(chunk)
    })
    child.stderr?.on('data', (chunk) => {
      output += String(chunk)
    })
    child.on('error', () => resolve(null))
    child.on('close', () => {
      const line = output
        .split(/\r?\n/)
        .map((item) => item.trim())
        .find(Boolean)
      resolve(line ? line.replace(/^v/i, '') : null)
    })
    setTimeout(() => {
      try {
        child.kill()
      } catch {
        // 忽略
      }
    }, 5000)
  })
}

export async function detectAgents(ctx: AgentContext): Promise<AgentState[]> {
  const results = await Promise.all(
    AGENT_ORDER.map(async (agent) => {
      const executable = findExecutable(COMMANDS[agent], ctx)
      const version = executable ? await readVersion(executable) : null
      const status = await adapters[agent].status(ctx)
      return {
        id: agent,
        installed: Boolean(executable),
        version,
        backupAvailable: hasBackup(ctx, agent),
        ...status
      } satisfies AgentState
    })
  )
  return results
}

export function agentConfigPath(agent: AgentId, ctx: AgentContext): string {
  return adapters[agent].configPath(ctx)
}
