import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { verifyUpdateSignature } from './verify-update-signature.mjs'
const origin = 'https://dl.csmy.fun:23457'
const response = await fetch(`${origin}/updates/aihubmix-switch/latest.json`, {
  redirect: 'error', signal: AbortSignal.timeout(20000)
})
assert.equal(response.status, 200)
const manifest = await response.json()
if (process.argv[2]) assert.equal(manifest.version, process.argv[2])
const key = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8')).plugins.updater.pubkey
assert.equal(Object.keys(manifest.platforms).length, 6)
for (const [target, asset] of Object.entries(manifest.platforms)) {
  assert.equal(new URL(asset.url).origin, origin)
  assert.ok(new URL(asset.url).pathname.startsWith(`/updates/aihubmix-switch/v${manifest.version}/`))
  const download = await fetch(asset.url, { redirect: 'error', signal: AbortSignal.timeout(60000) })
  assert.equal(download.status, 200)
  const bytes = Buffer.from(await download.arrayBuffer())
  verifyUpdateSignature(bytes, asset.signature, key, manifest.version)
  console.log(`${target}: ${bytes.length} bytes, signature/version verified`)
}
console.log(`Mirror ${manifest.version}: all six formats verified without GitHub`)
