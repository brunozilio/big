import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { isApplication, isBlock, isCloudAccount, isItem, isLiveEvent, isMcpTool, isSession, isStepView } from '../src/api/guards'
import type { Application, Branding, CallResult, Connections, Models } from '../src/api/contract'

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

  test('resumo, aviso e sugestão seguem o contrato', () => {
    const item = load('item') as Record<string, unknown>
    expect(item.recap === null || typeof item.recap === 'string').toBe(true)
    expect(isItem({ ...item, recap: 'Espera sua revisão do PR.' })).toBe(true)
    expect(isItem({ ...item, recap: 3 })).toBe(false)
    const step = load('step') as Record<string, unknown>
    expect(isStepView({ ...step, headsUp: { title: 'Migração', explain: 'Altera tabelas.', urgent: true } })).toBe(true)
    expect(isStepView({ ...step, headsUp: { title: 'Migração' } })).toBe(false)
    const session = load('session') as Record<string, unknown>
    expect(isSession({ ...session, suggestion: 'Rode os testes de novo' })).toBe(true)
    expect(isSession({ ...session, suggestion: false })).toBe(false)
  })

  test('feed ao vivo', () => {
    expect(isLiveEvent(load('live-item'))).toBe(true)
    expect(isLiveEvent(load('live-session-status'))).toBe(true)
    expect(isLiveEvent(load('live-session-suggestion'))).toBe(true)
  })

  test('conexões, modelos e marca', () => {
    const connections = load('connections') as Connections
    expect(connections.providers.every((p) => typeof p.key === 'string' && typeof p.logo === 'string' && Array.isArray(p.fields))).toBe(true)
    expect(connections.connections.every((c) => c.status === 'ok' || c.status === 'error')).toBe(true)
    const models = load('models') as Models
    expect(models.models[0].id).toBe(models.default ?? '')
    expect(models.models.every((m) => Array.isArray(m.efforts) && typeof m.fast === 'boolean')).toBe(true)
    const branding = load('branding') as Branding
    expect(['system', 'light', 'dark']).toContain(branding.theme)
    expect(['en', 'es', 'pt-BR']).toContain(branding.language)
  })

  test('leituras dos blocos', () => {
    const issue = load('call-issue') as CallResult<'tracker.issue'>
    expect(Array.isArray(issue.comments) && Array.isArray(issue.subtasks) && typeof issue.title === 'string').toBe(true)
    const change = load('call-change') as CallResult<'code.change'>
    expect(change.files.every((f) => typeof f.diff === 'string' && typeof f.firstLine === 'number')).toBe(true)
    const projects = load('call-projects') as CallResult<'tracker.projects'>
    expect(projects.every((p) => typeof p.key === 'string' && typeof p.name === 'string' && (p.avatar === null || typeof p.avatar === 'string'))).toBe(true)
    const accounts = load('call-cloud-accounts') as unknown[]
    expect(accounts.length).toBeGreaterThan(0)
    expect(accounts.every(isCloudAccount)).toBe(true)
    expect(isCloudAccount({ id: '1', name: 'x', email: '', roles: [3] })).toBe(false)
    const tools = load('call-mcp-tools') as unknown[]
    expect(tools.length).toBeGreaterThan(0)
    expect(tools.every(isMcpTool)).toBe(true)
    expect(isMcpTool({ name: '' })).toBe(false)
  })

  test('aplicações', () => {
    const applications = load('applications') as Application[]
    expect(applications.length).toBeGreaterThan(0)
    expect(applications.every(isApplication)).toBe(true)
    const chosen = { ...applications[0], bindings: { cloud: { connection: 'c1', value: '111122223333/Deploy' } } }
    expect(isApplication(chosen)).toBe(true)
    expect(isApplication({ ...applications[0], bindings: { cloud: { connection: 'c1' } } })).toBe(false)
    for (const a of applications) {
      expect(typeof a.id === 'string' && typeof a.name === 'string' && Array.isArray(a.repositories)).toBe(true)
      expect(Object.values(a.bindings ?? {}).every((b) => typeof b.connection === 'string' && typeof b.value === 'string')).toBe(true)
      for (const r of a.repositories) {
        expect(typeof r.id === 'string' && typeof r.directory === 'string' && Array.isArray(r.run) && Array.isArray(r.verify)).toBe(true)
        expect(Object.values(r.bindings ?? {}).every((b) => typeof b.connection === 'string' && typeof b.value === 'string')).toBe(true)
      }
    }
  })
})
