import fs from 'node:fs'
import path from 'node:path'

export function ensureDir(dir: string): void {
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 })
}

export function exists(file: string): boolean {
  try {
    return fs.existsSync(file)
  } catch {
    return false
  }
}

export function readText(file: string): string | null {
  try {
    return fs.readFileSync(file, 'utf8')
  } catch {
    return null
  }
}

export function writeTextAtomic(file: string, content: string, mode = 0o600): void {
  ensureDir(path.dirname(file))
  const tmp = `${file}.tmp-${process.pid}-${Date.now()}`
  fs.writeFileSync(tmp, content, { mode })
  fs.renameSync(tmp, file)
  try {
    fs.chmodSync(file, mode)
  } catch {
    // Windows 忽略
  }
}

export function parseJson<T = Record<string, unknown>>(text: string | null, label: string): T {
  if (!text || !text.trim()) return {} as T
  try {
    return JSON.parse(text) as T
  } catch {
    throw new Error(`${label} 不是合法的 JSON，请先手动检查该文件`)
  }
}

export function stringifyJson(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`
}

export function removeFile(file: string): void {
  try {
    fs.unlinkSync(file)
  } catch {
    // 文件不存在时忽略
  }
}
