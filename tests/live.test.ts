import { describe, expect, it } from 'vitest'
import { probeRelay } from '../src/main/relay'

const BASE = process.env.TEST_BASE_URL || 'https://csmy.fun:23457'
const KEY = process.env.TEST_API_KEY || ''

describe('live relay probe', () => {
  it.skipIf(!KEY)('probes the configured relay', async () => {
    const result = await probeRelay(BASE, KEY)
    expect(result.ok).toBe(true)
    expect(result.models.length).toBeGreaterThan(0)
    expect(result.messages).not.toBe('unknown')
  })
})
