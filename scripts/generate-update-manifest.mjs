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
export const applicationUpdatePrefix = '/github-updates/gh-2dfe2a292f49bc85/'
export function packageUrl(version, name) {
  const prefix = manifestName(version) === 'latest-v2.json' ? applicationUpdatePrefix : '/updates/aihubmix-switch/'
  return `https://dl.csmy.fun:23457${prefix}v${version}/${name}`
}
// The CC rebuild starts at 1.1.0 and requires macOS 12+.
// Keep the existing isolated feed path and leave 1.0.x clients on latest.json.
export function manifestName(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Release requires a stable semantic version')
  const [major, minor] = version.split('.').map(Number)
  return major > 1 || (major === 1 && minor >= 1) ? 'latest-v2.json' : 'latest.json'
}
export function formatsForVersion(version) {
  const [major, minor, patch] = version.split('.').map(Number)
  return Object.fromEntries(Object.entries(formats).filter(([target]) =>
    manifestName(version) === 'latest.json' || major > 1 || minor > 1 || (minor === 1 && patch >= 3) || !target.startsWith('linux-')
  ))
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
  for (const [target, suffix] of Object.entries(formatsForVersion(version))) {
    const name = `AIHubMix-Switch_${version}_${suffix}`
    if (!(await stat(join(directory, name))).isFile()) throw new Error(`Missing updater ${target}`)
    const signature = (await readFile(join(directory, `${name}.sig`), 'utf8')).trim()
    if (!signature || !/^[A-Za-z0-9+/=]+$/.test(signature))
      throw new Error(`Invalid updater signature ${target}`)
    verifyUpdateSignature(await readFile(join(directory, name)), signature, publicKey, version)
    platforms[target] = {
      url: packageUrl(version, name),
      signature
    }
  }
  const manifest = { version, notes, pub_date: date, platforms }
  const feedName = manifestName(version)
  const incompatibleFeed = feedName === 'latest-v2.json' ? 'latest.json' : 'latest-v2.json'
  if ((await readdir(directory)).includes(incompatibleFeed))
    throw new Error('Release directory contains an incompatible updater feed')
  await writeFile(join(directory, feedName), JSON.stringify(manifest, null, 2) + '\n')
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
  console.log('Generated updater manifest and complete SHA256 checksums')
}
