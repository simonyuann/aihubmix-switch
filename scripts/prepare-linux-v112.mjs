import { readFile, writeFile, readdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { join } from 'node:path'
import { verifyUpdateSignature } from './verify-update-signature.mjs'
const dir = process.argv[2] || 'artifacts'
const manifest = JSON.parse(await readFile(join(dir, 'latest-v2.json'), 'utf8'))
if (manifest.version !== '1.1.2' || Object.keys(manifest.platforms).length !== 4) throw Error('Unexpected original manifest')
const key = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8')).plugins.updater.pubkey
for (const [target, suffix] of Object.entries({'linux-x86_64-appimage': 'AppImage', 'linux-x86_64-deb': 'deb'})) {
  const name = `AIHubMix-Switch_1.1.2_linux-x64.${suffix}`
  const bytes = await readFile(join(dir, name))
  const signature = (await readFile(join(dir, name + '.sig'), 'utf8')).trim()
  verifyUpdateSignature(bytes, signature, key, '1.1.2')
  manifest.platforms[target] = {url: `https://dl.csmy.fun:23457/github-updates/gh-2dfe2a292f49bc85/v1.1.2/${name}`, signature}
  console.log(`${target}: ${bytes.length} bytes, signature verified`)
}
await writeFile(join(dir, 'latest-v2.json'), JSON.stringify(manifest, null, 2) + '\n')
const files = (await readdir(dir)).filter(n => n !== 'SHA256SUMS.txt').sort()
const checks = await Promise.all(files.map(async name => `${createHash('sha256').update(await readFile(join(dir, name))).digest('hex')}  ${name}`))
await writeFile(join(dir, 'SHA256SUMS.txt'), checks.join('\n') + '\n')
