import { execFileSync } from 'node:child_process'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { existsSync } from 'node:fs'
import { selectReleaseFile } from './select-release-file.mjs'
const target = process.argv[2]
const { version } = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'))
const release = join('src-tauri', 'target', target, 'release')
function check(file, expected) {
  const value = execFileSync(resolve(file), ['--print-update-format'], {
    encoding: 'utf8',
    env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: '1' },
    timeout: 60000
  }).trim()
  if (value !== expected) throw new Error(`Wrong update format: expected ${expected}, got ${value}`)
  const mode = execFileSync(resolve(file), ['--print-update-install-mode'], {
    encoding: 'utf8', env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: '1' }, timeout: 60000
  }).trim()
  if (mode !== (expected === 'portable' ? 'manual' : 'automatic'))
    throw new Error(`Wrong update install mode for ${expected}: ${mode}`)
  console.log(`Native update format verified: ${expected}`)
}
if (target.includes('windows')) {
  const portable = selectReleaseFile(await readdir('artifacts'), version, '-portable.exe')
  if (existsSync(join('artifacts', 'portable.ini')))
    throw new Error('Portable regression must run without portable.ini')
  check(join('artifacts', portable), 'portable')
  // Tauri restores the raw executable after bundling. Inspect the actual installer payload.
  const folder = join(release, 'bundle', 'nsis')
  const installer = selectReleaseFile(await readdir(folder), version, '.exe')
  const directory = await mkdtemp(join(tmpdir(), 'aihubmix-nsis-check-'))
  try {
    execFileSync('7z', ['x', resolve(folder, installer), `-o${directory}`, '-y'], { stdio: 'pipe' })
    const binary = (await readdir(directory, { recursive: true })).find((name) =>
      name.endsWith('cc-switch.exe')
    )
    if (!binary) throw new Error('NSIS app payload is missing')
    check(join(directory, binary), 'nsis')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
} else if (target.includes('apple')) {
  check(
    join(release, 'bundle', 'macos', 'AIHubMix Switch.app', 'Contents', 'MacOS', 'cc-switch'),
    'app'
  )
} else if (target.includes('linux')) {
  const image = selectReleaseFile(await readdir(join(release, 'bundle', 'appimage')), version, '.AppImage')
  check(join(release, 'bundle', 'appimage', image), 'appimage')
  const deb = selectReleaseFile(await readdir(join(release, 'bundle', 'deb')), version, '.deb')
  const directory = await mkdtemp(join(tmpdir(), 'aihubmix-deb-check-'))
  try {
    execFileSync('dpkg-deb', ['--extract', join(release, 'bundle', 'deb', deb), directory])
    check(join(directory, 'usr', 'bin', 'cc-switch'), 'deb')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
} else throw new Error('Unknown target')
