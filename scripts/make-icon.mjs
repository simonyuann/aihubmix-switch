import sharp from 'sharp'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)))
const source = path.join(root, 'build', 'icon.svg')
const target = path.join(root, 'build', 'icon.png')

await sharp(source).resize(512, 512).png().toFile(target)
console.log(`icon written: ${target}`)
