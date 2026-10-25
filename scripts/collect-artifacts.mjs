import { mkdir, readdir, copyFile, access } from 'node:fs/promises'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

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
  const file = (await readdir(folder)).find((f) => f.endsWith('.dmg'))
  if (!file) throw new Error('DMG was not generated')
  await copyFile(join(folder, file), join('artifacts', `${prefix}.dmg`))
  const app = join(release, 'bundle', 'macos', 'AIHubMix Switch.app')
  await access(app)
  execFileSync('tar', [
    '-czf',
    join('artifacts', `${prefix}.app.tar.gz`),
    '-C',
    join(release, 'bundle', 'macos'),
    'AIHubMix Switch.app'
  ])
} else if (platform === 'linux-x64') {
  for (const [format, extension] of [
    ['deb', '.deb'],
    ['appimage', '.AppImage']
  ]) {
    const folder = join(release, 'bundle', format)
    const file = (await readdir(folder)).find((name) => name.endsWith(extension))
    if (!file) throw new Error(`Linux ${format} bundle was not generated`)
    await copyFile(join(folder, file), join('artifacts', `${prefix}${extension}`))
  }
} else {
  const folder = join(release, 'bundle', 'nsis')
  const file = (await readdir(folder)).find((f) => f.endsWith('.exe'))
  if (!file) throw new Error('Windows installer was not generated')
  await copyFile(join(folder, file), join('artifacts', `${prefix}-setup.exe`))
  await copyFile(join(release, 'aihubmix-switch.exe'), join('artifacts', `${prefix}-portable.exe`))
}
console.log(`Collected ${platform} release artifacts`)
