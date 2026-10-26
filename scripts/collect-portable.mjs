import { copyFile, mkdir, readFile } from 'node:fs/promises'
import { join } from 'node:path'
const target = process.argv[2]
if (target !== 'x86_64-pc-windows-msvc') throw new Error('Portable snapshot is Windows-only')
const { version } = JSON.parse(await readFile('package.json', 'utf8'))
await mkdir('artifacts', { recursive: true })
// Snapshot before NSIS bundling patches the executable's bundle type marker.
await copyFile(
  join('src-tauri', 'target', target, 'release', 'cc-switch.exe'),
  join('artifacts', `AIHubMix-Switch_${version}_windows-x64-portable.exe`)
)
