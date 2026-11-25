import { describe, expect, test } from 'bun:test'
import { mockApi } from '../src/api/mock'
import type { Bindings } from '../src/api/contract'
import { configuredSpaces, searchTargets } from '../src/screens/items/projects'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })

describe('espaços do composer', () => {
  test('os espaços vêm das aplicações, sem repetir e só das conexões ligadas', () => {
    const apps: { bindings: Bindings }[] = [
      { bindings: { tracker: { connection: 'a', value: 'APP' } } },
      { bindings: { tracker: { connection: 'a', value: 'APP' }, code: { connection: 'g', value: 'acme/web' } } },
      { bindings: {} },
      { bindings: { tracker: { connection: 'z', value: 'OPS' } } },
    ]
    expect(configuredSpaces(apps, ['a', 'b'])).toEqual([{ connection: 'a', key: 'APP' }])
  })

  test('um vínculo com vários espaços oferece cada um deles', () => {
    const apps: { bindings: Bindings }[] = [{ bindings: { tracker: { connection: 'a', value: 'APP', more: [{ value: 'OPS' }, { value: 'APP' }] } } }]
    expect(configuredSpaces(apps, ['a'])).toEqual([{ connection: 'a', key: 'APP' }, { connection: 'a', key: 'OPS' }])
  })

  test('a busca fica nos espaços configurados ou, sem nenhum, em todas as conexões', () => {
    expect(searchTargets(['a', 'b'], [{ connection: 'b', key: 'OPS' }], 'x')).toEqual([{ connection: 'b', args: { query: 'x', project: 'OPS' } }])
    expect(searchTargets(['a', 'b'], [], 'x')).toEqual([{ connection: 'a', args: { query: 'x' } }, { connection: 'b', args: { query: 'x' } }])
    expect(searchTargets(['a'], [{ connection: 'z', key: 'OPS' }], '')).toEqual([{ connection: 'a', args: { query: '' } }])
  })

  test('o mock filtra a busca pelo espaço', async () => {
    const api = fresh()
    const ops = await api.call('c-jira', 'tracker.search', { query: '', project: 'OPS' })
    expect(ops.length).toBeGreaterThan(0)
    expect(ops.every((i) => i.key.startsWith('OPS-'))).toBe(true)
  })
})
