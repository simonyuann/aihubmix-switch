import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { applicationUpdatePrefix, formats, formatsForVersion, generateManifest, manifestName, packageUrl } from '../scripts/generate-update-manifest.mjs'
import { verifyUpdateSignature } from '../scripts/verify-update-signature.mjs'
const folder = new URL('../src-tauri/fixtures/updater/', import.meta.url)
const payload = await readFile(new URL('payload.bin', folder))
const signature = (await readFile(new URL('payload.sig', folder), 'utf8')).trim()
const key = (await readFile(new URL('public.key', folder), 'utf8')).trim()
const version = '0.2.3'

test('real Tauri signatures reject altered contents, announced versions and trusted comments', () => {
  verifyUpdateSignature(payload, signature, key, version)
  assert.throws(() =>
    verifyUpdateSignature(
      Buffer.concat([payload, Buffer.from('tampered')]),
      signature,
      key,
      version
    )
  )
  assert.throws(() => verifyUpdateSignature(payload, signature, key, '0.2.4'))
  const changed = Buffer.from(
    Buffer.from(signature, 'base64').toString().replace('version:0.2.3', 'version:0.2.4')
  ).toString('base64')
  assert.throws(() => verifyUpdateSignature(payload, changed, key, '0.2.4'))
})
test('manifest binds all installation formats and every package has a valid signed version', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'aihubmix-manifest-'))
  try {
    for (const suffix of [...Object.values(formats), 'macos-arm64.dmg', 'macos-x64.dmg']) {
      const name = `AIHubMix-Switch_${version}_${suffix}`
      await writeFile(join(directory, name), payload)
      if (Object.values(formats).includes(suffix))
        await writeFile(join(directory, `${name}.sig`), signature)
    }
    const manifest = await generateManifest(
      directory,
      version,
      'Fixture release',
      '2026-10-25T00:00:00Z',
      key
    )
    assert.equal(Object.keys(manifest.platforms).length, 6)
    assert.notEqual(
      manifest.platforms['linux-x86_64-deb'].url,
      manifest.platforms['linux-x86_64-appimage'].url
    )
    assert.notEqual(
      manifest.platforms['windows-x86_64-nsis'].url,
      manifest.platforms['windows-x86_64-portable'].url
    )
    for (const asset of Object.values(manifest.platforms))
      assert.ok(
        asset.url.startsWith(`https://dl.csmy.fun:23457/updates/aihubmix-switch/v${version}/`)
      )
    const checks = await readFile(join(directory, 'SHA256SUMS.txt'), 'utf8')
    assert.equal(checks.trim().split('\n').length, 15)
    assert.ok(!checks.includes('  SHA256SUMS.txt'))
    await writeFile(join(directory, `AIHubMix-Switch_${version}_linux-x64.deb`), 'corrupted')
    await assert.rejects(() => generateManifest(directory, version, '', undefined, key))
    await rm(join(directory, `AIHubMix-Switch_${version}_macos-arm64.app.tar.gz.sig`))
    await assert.rejects(() => generateManifest(directory, version, '', undefined, key))
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})

test('production updater checks only the download station', async () => {
  const config = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'))
  assert.deepEqual(config.plugins.updater.endpoints, [
    'https://dl.csmy.fun:23457/github-updates/gh-2dfe2a292f49bc85/latest-v2.json'
  ])
  assert.equal(config.plugins.updater.requireSignedVersion, true)
  assert.equal(config.plugins.updater.allowDowngrades, false)
  assert.equal(packageUrl('1.1.1', 'package.exe'), `https://dl.csmy.fun:23457${applicationUpdatePrefix}v1.1.1/package.exe`)
  assert.equal(packageUrl('1.0.2', 'package.exe'), 'https://dl.csmy.fun:23457/updates/aihubmix-switch/v1.0.2/package.exe')
})

test('macOS 12+ releases use a distinct feed from macOS 11 legacy clients', () => {
  assert.equal(manifestName('1.0.2'), 'latest.json')
  assert.equal(manifestName('1.0.99'), 'latest.json')
  assert.equal(manifestName('1.1.0'), 'latest-v2.json')
  assert.equal(manifestName('1.2.0'), 'latest-v2.json')
  assert.equal(manifestName('2.0.0'), 'latest-v2.json')
  assert.equal(manifestName('3.0.0'), 'latest-v2.json')
  assert.throws(() => manifestName('2.0.0-rc.1'))
})

test('CC rebuild releases require macOS and Windows without waiting for Linux', () => {
  assert.deepEqual(Object.keys(formatsForVersion('1.1.0')), [
    'darwin-aarch64-app', 'darwin-x86_64-app',
    'windows-x86_64-nsis', 'windows-x86_64-portable'
  ])
  assert.deepEqual(formatsForVersion('1.0.2'), formats)
})
