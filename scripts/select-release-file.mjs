/** Select exactly one bundle for the current version, never a cached older one. */
export function selectReleaseFile(names, version, suffix) {
  const matches = names.filter((name) => name.includes(`_${version}_`) && name.endsWith(suffix))
  if (matches.length !== 1)
    throw new Error(`Expected exactly one ${suffix} bundle for version ${version}; found ${matches.length}`)
  return matches[0]
}
