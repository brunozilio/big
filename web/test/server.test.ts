import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { isBlock, isItem, isLiveEvent, isSession, isStepView } from '../src/api/guards'
import type { Branding, CallResult, Connections, Models } from '../src/api/contract'

const load = (name: string): unknown => JSON.parse(readFileSync(join(import.meta.dir, 'fixtures', 'server', `${name}.json`), 'utf8'))

describe('respostas reais do servidor', () => {
  test('itens', () => {
    expect(isItem(load('item'))).toBe(true)
    const items = load('items') as unknown[]
    expect(items.length).toBeGreaterThan(0)
    expect(items.every(isItem)).toBe(true)
  })

  test('etapas e blocos', () => {
    for (const name of ['step', 'step-waiting']) {
      const view = load(name) as { blocks: unknown[] }
      expect(isStepView(view)).toBe(true)
      expect(view.blocks.every(isBlock)).toBe(true)
    }
  })

  test('sessão', () => {
    expect(isSession(load('session'))).toBe(true)
  })

  test('feed ao vivo', () => {
    expect(isLiveEvent(load('live-item'))).toBe(true)
    expect(isLiveEvent(load('live-session-status'))).toBe(true)
  })

  test('conexões, modelos e marca', () => {
    const connections = load('connections') as Connections
    expect(connections.providers.every((p) => typeof p.key === 'string' && typeof p.logo === 'string' && Array.isArray(p.fields))).toBe(true)
    expect(connections.connections.every((c) => c.status === 'ok' || c.status === 'error')).toBe(true)
    const models = load('models') as Models
    expect(models.models[0].id).toBe(models.default)
    const branding = load('branding') as Branding
    expect(['system', 'light', 'dark']).toContain(branding.theme)
  })

  test('leituras dos blocos', () => {
    const issue = load('call-issue') as CallResult<'tracker.issue'>
    expect(Array.isArray(issue.comments) && Array.isArray(issue.subtasks) && typeof issue.title === 'string').toBe(true)
    const change = load('call-change') as CallResult<'code.change'>
    expect(change.files.every((f) => typeof f.diff === 'string' && typeof f.firstLine === 'number')).toBe(true)
  })
})
