import { readFile, readdir, stat } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { join } from 'node:path'
import { formatsForVersion, manifestName } from './generate-update-manifest.mjs'
const directory = process.argv[2] || 'artifacts'
const release = JSON.parse(await readFile(process.argv[3] || 'release-info.json', 'utf8'))
const { version } = JSON.parse(await readFile('package.json', 'utf8'))
if (!release.draft || release.tag_name !== `v${version}`)
  throw new Error('Expected an unpublished draft for this version')
const names = (await readdir(directory)).sort()
const expected = [
  'SHA256SUMS.txt', manifestName(version),
  ...['macos-arm64.dmg', 'macos-x64.dmg'].map(suffix => `AIHubMix-Switch_${version}_${suffix}`),
  ...Object.values(formatsForVersion(version)).flatMap(suffix =>
    ['', '.sig'].map(extra => `AIHubMix-Switch_${version}_${suffix}${extra}`)
  )
].sort()
if (JSON.stringify(names) !== JSON.stringify(expected) || release.assets.length !== names.length)
  throw new Error('Incomplete update release')
for (const name of names) {
  const remote = release.assets.find((asset) => asset.name === name)
  const file = join(directory, name)
  const digest = `sha256:${createHash('sha256')
    .update(await readFile(file))
    .digest('hex')}`
  if (!remote || remote.size !== (await stat(file)).size || remote.digest !== digest)
    throw new Error(`Uploaded asset mismatch: ${name}`)
}
console.log(`All ${names.length} draft assets match their local SHA256 digests; ready to publish`)
