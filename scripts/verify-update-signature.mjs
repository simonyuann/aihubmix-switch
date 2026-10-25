// Build-time cross-check using Node's Ed25519/Blake2b primitives.
// The production client always uses Tauri's official updater verification.
import { createHash, createPublicKey, verify } from 'node:crypto'
export function verifyUpdateSignature(bytes, encodedSignature, encodedKey, version) {
  const publicLines = Buffer.from(encodedKey.trim(), 'base64')
    .toString('utf8')
    .trim()
    .split(/\r?\n/)
  const key = Buffer.from(publicLines[1] || '', 'base64')
  const lines = Buffer.from(encodedSignature.trim(), 'base64')
    .toString('utf8')
    .trim()
    .split(/\r?\n/)
  const signature = Buffer.from(lines[1] || '', 'base64')
  const globalSignature = Buffer.from(lines[3] || '', 'base64')
  if (
    key.length !== 42 ||
    signature.length !== 74 ||
    globalSignature.length !== 64 ||
    !lines[2]?.startsWith('trusted comment: ')
  )
    throw new Error('Malformed updater signature')
  if (!key.subarray(2, 10).equals(signature.subarray(2, 10)))
    throw new Error('Updater signing key mismatch')
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), key.subarray(10)]),
    format: 'der',
    type: 'spki'
  })
  const algorithm = signature.subarray(0, 2).toString('ascii')
  if (!['ED', 'Ed'].includes(algorithm)) throw new Error('Unsupported updater signature algorithm')
  const data = algorithm === 'ED' ? createHash('blake2b512').update(bytes).digest() : bytes
  const rawSignature = signature.subarray(10)
  if (!verify(null, data, publicKey, rawSignature))
    throw new Error('Updater package signature mismatch')
  const comment = lines[2].slice('trusted comment: '.length)
  if (
    !verify(null, Buffer.concat([rawSignature, Buffer.from(comment)]), publicKey, globalSignature)
  )
    throw new Error('Updater trusted comment signature mismatch')
  if (comment.split('\t').find((field) => field.startsWith('version:')) !== `version:${version}`)
    throw new Error('Updater signed version mismatch')
}
