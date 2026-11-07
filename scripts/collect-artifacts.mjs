import { mkdir, readdir, copyFile, access } from 'node:fs/promises'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { selectReleaseFile } from './select-release-file.mjs'

const target = process.argv[2]
if (
  ![
    'aarch64-apple-darwin',
    'x86_64-apple-darwin',
    'x86_64-pc-windows-msvc',
    'x86_64-unknown-linux-gnu'
  ].includes(target)
)
  throw new Error('Unsupported release target')
const { version } = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
const platform =
  target === 'aarch64-apple-darwin'
    ? 'macos-arm64'
    : target === 'x86_64-apple-darwin'
      ? 'macos-x64'
      : target === 'x86_64-unknown-linux-gnu'
        ? 'linux-x64'
        : 'windows-x64'
const release = join('src-tauri', 'target', target, 'release')
const prefix = `AIHubMix-Switch_${version}_${platform}`
await mkdir('artifacts', { recursive: true })
if (platform.startsWith('macos')) {
  const folder = join(release, 'bundle', 'dmg')
  const file = selectReleaseFile(await readdir(folder), version, '.dmg')
  await copyFile(join(folder, file), join('artifacts', `${prefix}.dmg`))
  const folderMac = join(release, 'bundle', 'macos')
  const archive = (await readdir(folderMac)).find((file) => file.endsWith('.app.tar.gz'))
  if (!archive) throw new Error('Official macOS updater archive was not generated')
  await copyFile(join(folderMac, archive), join('artifacts', `${prefix}.app.tar.gz`))
} else if (platform === 'linux-x64') {
  for (const [format, extension] of [
    ['deb', '.deb'],
    ['appimage', '.AppImage']
  ]) {
    const folder = join(release, 'bundle', format)
    const file = selectReleaseFile(await readdir(folder), version, extension)
    await copyFile(join(folder, file), join('artifacts', `${prefix}${extension}`))
  }
} else {
  const folder = join(release, 'bundle', 'nsis')
  const file = selectReleaseFile(await readdir(folder), version, '.exe')
  await copyFile(join(folder, file), join('artifacts', `${prefix}-setup.exe`))
  await access(join('artifacts', `${prefix}-portable.exe`))
}
const updaterSuffixes = platform.startsWith('macos')
  ? ['.app.tar.gz']
  : platform === 'linux-x64'
    ? ['.AppImage', '.deb']
    : ['-setup.exe', '-portable.exe']
if (!process.env.TAURI_SIGNING_PRIVATE_KEY || !process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD)
  throw new Error('Updater signing credentials are not configured')
for (const suffix of updaterSuffixes) {
  const file = join('artifacts', `${prefix}${suffix}`)
  try {
    execFileSync(
      process.execPath,
      [
        join('node_modules', '@tauri-apps', 'cli', 'tauri.js'),
        'signer',
        'sign',
        '--app-version',
        version,
        file
      ],
      { stdio: 'pipe' }
    )
  } catch {
    throw new Error(
      `Unable to sign updater package ${platform}${suffix}; private material withheld`
    )
  }
  await access(`${file}.sig`)
}
console.log(`Collected and signed ${platform} release artifacts`)
