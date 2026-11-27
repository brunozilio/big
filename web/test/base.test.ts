import { afterEach, describe, expect, test } from 'bun:test'
import { at, basePath } from '../src/api/base'

function served(href: string | null) {
  document.querySelectorAll('base').forEach((tag) => tag.remove())
  if (href === null) return
  const tag = document.createElement('base')
  tag.setAttribute('href', href)
  document.head.appendChild(tag)
}

describe('caminho base', () => {
  afterEach(() => served(null))

  test('na raiz nada muda', () => {
    served('/')
    expect(basePath()).toBe('')
    expect(at('/api/items')).toBe('/api/items')
  })

  test('sob um prefixo, todo caminho absoluto ganha o prefixo', () => {
    served('/app/')
    expect(basePath()).toBe('/app')
    expect(at('/api/live?after=e%3A1')).toBe('/app/api/live?after=e%3A1')
    expect(at('https://idp.example.com/x')).toBe('https://idp.example.com/x')
    expect(at('//evil.example.com')).toBe('//evil.example.com')
  })

  test('sem a tag, ou com outra origem nela, fica na raiz', () => {
    served(null)
    expect(basePath()).toBe('')
    served('//evil.example.com/')
    expect(basePath()).toBe('')
  })
})
