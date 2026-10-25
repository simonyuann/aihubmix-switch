import { readFile, readdir, writeFile, stat } from 'node:fs/promises'
import { join } from 'node:path'
import { createHash } from 'node:crypto'
import { verifyUpdateSignature } from './verify-update-signature.mjs'
import { pathToFileURL } from 'node:url'
export const formats = {
  'darwin-aarch64-app': 'macos-arm64.app.tar.gz',
  'darwin-x86_64-app': 'macos-x64.app.tar.gz',
  'windows-x86_64-nsis': 'windows-x64-setup.exe',
  'windows-x86_64-portable': 'windows-x64-portable.exe',
  'linux-x86_64-appimage': 'linux-x64.AppImage',
  'linux-x86_64-deb': 'linux-x64.deb'
}
export async function generateManifest(
  directory,
  version,
  notes,
  date = new Date().toISOString(),
  expectedKey
) {
  if (!/^\d+\.\d+\.\d+$/.test(version))
    throw new Error('Release requires a stable semantic version')
  const publicKey =
    expectedKey ||
    JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8')).plugins.updater.pubkey
  for (const suffix of ['macos-arm64.dmg', 'macos-x64.dmg'])
    await stat(join(directory, `AIHubMix-Switch_${version}_${suffix}`))
  const platforms = {}
  for (const [target, suffix] of Object.entries(formats)) {
    const name = `AIHubMix-Switch_${version}_${suffix}`
    if (!(await stat(join(directory, name))).isFile()) throw new Error(`Missing updater ${target}`)
    const signature = (await readFile(join(directory, `${name}.sig`), 'utf8')).trim()
    if (!signature || !/^[A-Za-z0-9+/=]+$/.test(signature))
      throw new Error(`Invalid updater signature ${target}`)
    verifyUpdateSignature(await readFile(join(directory, name)), signature, publicKey, version)
    platforms[target] = {
      url: `https://github.com/simonyuann/aihubmix-switch/releases/download/v${version}/${name}`,
      signature
    }
  }
  const manifest = { version, notes, pub_date: date, platforms }
  await writeFile(join(directory, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n')
  const names = (await readdir(directory)).filter((name) => name !== 'SHA256SUMS.txt').sort()
  const checks = []
  for (const name of names) {
    if (!(await stat(join(directory, name))).isFile())
      throw new Error(`Unexpected release directory ${name}`)
    checks.push(
      `${createHash('sha256')
        .update(await readFile(join(directory, name)))
        .digest('hex')}  ${name}`
    )
  }
  await writeFile(join(directory, 'SHA256SUMS.txt'), checks.join('\n') + '\n')
  return manifest
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { version } = JSON.parse(await readFile('package.json', 'utf8'))
  await generateManifest(
    process.argv[2] || 'artifacts',
    version,
    await readFile(`docs/releases/v${version}.md`, 'utf8')
  )
  console.log('Generated six-format updater manifest and complete SHA256 checksums')
}
