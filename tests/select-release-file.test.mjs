import { test } from 'node:test'
import assert from 'node:assert/strict'
import { selectReleaseFile } from '../scripts/select-release-file.mjs'

test('selects the requested release even when older bundles are cached', () => {
  const names = ['AIHubMix Switch_1.1.11_x64-setup.exe', 'AIHubMix Switch_1.1.12_x64-setup.exe']
  assert.equal(selectReleaseFile(names, '1.1.12', '.exe'), names[1])
})

test('rejects missing or ambiguous bundles', () => {
  assert.throws(() => selectReleaseFile(['AIHubMix Switch_1.1.11_x64-setup.exe'], '1.1.12', '.exe'))
  assert.throws(() => selectReleaseFile(['a_1.1.12_x64.dmg', 'b_1.1.12_arm64.dmg'], '1.1.12', '.dmg'))
})
