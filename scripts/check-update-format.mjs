import { execFileSync } from 'node:child_process'
import { mkdtemp, readdir, rm } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
const target = process.argv[2]
const release = join('src-tauri', 'target', target, 'release')
function check(file, expected) {
  const value = execFileSync(resolve(file), ['--print-update-format'], {
    encoding: 'utf8',
    env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: '1' },
    timeout: 60000
  }).trim()
  if (value !== expected) throw new Error(`Wrong update format: expected ${expected}, got ${value}`)
  console.log(`Native update format verified: ${expected}`)
}
if (target.includes('windows')) {
  const portable = (await readdir('artifacts')).find((name) => name.endsWith('-portable.exe'))
  if (!portable) throw new Error('Portable executable is missing')
  check(join('artifacts', portable), 'portable')
  // Tauri restores the raw executable after bundling. Inspect the actual installer payload.
  const folder = join(release, 'bundle', 'nsis')
  const installer = (await readdir(folder)).find((name) => name.endsWith('.exe'))
  if (!installer) throw new Error('NSIS installer is missing')
  const directory = await mkdtemp(join(tmpdir(), 'aihubmix-nsis-check-'))
  try {
    execFileSync('7z', ['x', resolve(folder, installer), `-o${directory}`, '-y'], { stdio: 'pipe' })
    const binary = (await readdir(directory, { recursive: true })).find((name) =>
      name.endsWith('aihubmix-switch.exe')
    )
    if (!binary) throw new Error('NSIS app payload is missing')
    check(join(directory, binary), 'nsis')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
} else if (target.includes('apple')) {
  check(
    join(release, 'bundle', 'macos', 'AIHubMix Switch.app', 'Contents', 'MacOS', 'aihubmix-switch'),
    'app'
  )
} else if (target.includes('linux')) {
  const image = (await readdir(join(release, 'bundle', 'appimage'))).find((name) =>
    name.endsWith('.AppImage')
  )
  if (!image) throw new Error('AppImage is missing')
  check(join(release, 'bundle', 'appimage', image), 'appimage')
  const deb = (await readdir(join(release, 'bundle', 'deb'))).find((name) => name.endsWith('.deb'))
  if (!deb) throw new Error('Debian package is missing')
  const directory = await mkdtemp(join(tmpdir(), 'aihubmix-deb-check-'))
  try {
    execFileSync('dpkg-deb', ['--extract', join(release, 'bundle', 'deb', deb), directory])
    check(join(directory, 'usr', 'bin', 'aihubmix-switch'), 'deb')
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
} else throw new Error('Unknown target')
