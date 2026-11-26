import { describe, expect, test } from 'bun:test'
import { fromBase64Url, nextPath, toBase64Url } from '@/api/auth'

describe('auth', () => {
  test('only a local path is followed after signing in', () => {
    expect(nextPath('?next=%2Fitems%2F1')).toBe('/items/1')
    expect(nextPath('?next=%2F%2Fevil.com')).toBe('/')
    expect(nextPath('?next=https%3A%2F%2Fevil.com')).toBe('/')
    expect(nextPath('')).toBe('/')
  })

  test('base64url survives a round trip without padding', () => {
    const bytes = new Uint8Array([0, 251, 255, 62, 63, 1])
    const text = toBase64Url(bytes.buffer)
    expect(text).not.toContain('=')
    expect(text).not.toContain('+')
    expect(new Uint8Array(fromBase64Url(text))).toEqual(bytes)
  })
})
