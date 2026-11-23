import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, CallAction, CallArgs, NewItem as NewItemInput } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { setConnections } from '../src/api/store'
import { NewItem } from '../src/screens/items/NewItem'
import { TooltipProvider } from '../src/ui/overlay'
import { initialProject, rememberProject, rememberedProject, searchTargets } from '../src/screens/items/projects'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })

beforeEach(() => localStorage.clear())
afterEach(cleanup)

describe('projeto do composer', () => {
  test('lembra o último projeto por conexão e esquece os das outras', () => {
    expect(initialProject(['a', 'b'])).toBeNull()
    rememberProject(['a', 'b'], { connection: 'b', key: 'OPS' })
    expect(initialProject(['a', 'b'])).toEqual({ connection: 'b', key: 'OPS' })
    rememberProject(['a', 'b'], { connection: 'a', key: 'APP' })
    expect(rememberedProject('b')).toBeNull()
    expect(initialProject(['a', 'b'])).toEqual({ connection: 'a', key: 'APP' })
    rememberProject(['a', 'b'], null)
    expect(initialProject(['a', 'b'])).toBeNull()
  })

  test('sem armazenamento disponível não quebra', () => {
    const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
    Object.defineProperty(globalThis, 'localStorage', { configurable: true, get: () => { throw new Error('bloqueado') } })
    try {
      expect(() => rememberProject(['a'], { connection: 'a', key: 'X' })).not.toThrow()
      expect(initialProject(['a'])).toBeNull()
    } finally {
      if (original) Object.defineProperty(globalThis, 'localStorage', original)
    }
  })

  test('o projeto escolhido filtra a busca só na sua conexão', () => {
    expect(searchTargets(['a', 'b'], { connection: 'b', key: 'OPS' }, 'x')).toEqual([{ connection: 'b', args: { query: 'x', project: 'OPS' } }])
    expect(searchTargets(['a', 'b'], null, 'x')).toEqual([{ connection: 'a', args: { query: 'x' } }, { connection: 'b', args: { query: 'x' } }])
    expect(searchTargets(['a'], { connection: 'z', key: 'OPS' }, '')).toEqual([{ connection: 'a', args: { query: '' } }])
  })

  test('o mock lista projetos, filtra a busca e cria o item no projeto', async () => {
    const api = fresh()
    const projects = await api.call('c-jira', 'tracker.projects', {})
    expect(projects.map((p) => p.key)).toEqual(['APP', 'OPS'])
    const ops = await api.call('c-jira', 'tracker.search', { query: '', project: 'OPS' })
    expect(ops.length).toBeGreaterThan(0)
    expect(ops.every((i) => i.key.startsWith('OPS-'))).toBe(true)
    const it = await api.createItem({ text: 'alerta novo', issue: null, project: { connection: 'c-jira', key: 'OPS' }, model: 'auto', attachments: [] })
    expect(it.title.toLowerCase()).toContain('alerta')
  })

  test('o seletor usa o projeto lembrado na busca e na criação do item', async () => {
    const base = fresh()
    const calls: { action: string; args: unknown }[] = []
    const created: NewItemInput[] = []
    const spy: Api = {
      ...base,
      call: <A extends CallAction>(connection: string, action: A, args: CallArgs<A>) => {
        calls.push({ action, args })
        return base.call(connection, action, args)
      },
      createItem: (input) => {
        created.push(input)
        return base.createItem(input)
      },
    }
    setApi(spy)
    const connections = await base.connections()
    const tracker = connections.connections.find((c) => c.capabilities.includes('tracker'))
    if (!tracker) throw new Error('sem tracker no mock')
    rememberProject([tracker.id], { connection: tracker.id, key: 'OPS' })
    act(() => setConnections(connections))
    const view = render(<TooltipProvider><NewItem onCreated={() => undefined} /></TooltipProvider>)
    const trigger = await view.findByRole('button', { name: /Começar de uma issue/ })
    expect(trigger.textContent).toContain('OPS')
    fireEvent.click(trigger)
    await waitFor(() => expect(calls.some((c) => c.action === 'tracker.search' && (c.args as { project?: string }).project === 'OPS')).toBe(true))
    await waitFor(() => expect(view.getByRole('button', { name: 'Projeto: Operações' })).toBeTruthy())
    expect(calls.some((c) => c.action === 'tracker.projects')).toBe(true)
    fireEvent.keyDown(document, { key: 'Escape' })
    const box = view.getByLabelText('Mensagem')
    fireEvent.change(box, { target: { value: 'Rotacionar chaves' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    await waitFor(() => expect(created.length).toBe(1))
    expect(created[0].project).toEqual({ connection: tracker.id, key: 'OPS' })
    setApi(null)
  })
})
