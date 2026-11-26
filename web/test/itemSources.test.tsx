import { describe, expect, test } from 'bun:test'
import type { Application } from '@/api/contract'
import { mockApi } from '@/api/mock'
import { basesOf, defaultBranch, gitOf, linkedRepos, matchesPull, placesOf, pullSubject, sourcesOf } from '@/screens/items/NewItem'

const app: Application = {
  id: 'a1', name: 'Loja', bindings: {},
  repositories: [{ id: 'r1', bindings: { code: { connection: 'c-code', value: 'acme/web' } }, directory: '/p/web', run: [], verify: [] }],
}

describe('origens do item', () => {
  test('um link ou número de pull request vira a referência dono/repo#n', () => {
    expect(pullSubject('https://code.test/acme/web/pull/12', null)).toBe('acme/web#12')
    expect(pullSubject('https://code.test/grupo/app/-/merge_requests/3', null)).toBe('grupo/app#3')
    expect(pullSubject(' #7 ', app)).toBe('acme/web#7')
    expect(pullSubject('acme/api#4', app)).toBe('acme/api#4')
    expect(pullSubject('  ', app)).toBeNull()
  })

  test('o branch base só vai para os repositórios preenchidos', () => {
    expect(placesOf(app)).toEqual([{ id: 'r1', name: 'acme/web' }])
    expect(placesOf(null)).toEqual([{ id: '', name: 'Repositório' }])
    expect(gitOf({ r1: ' release/2 ' }, placesOf(app))).toEqual({ r1: { base: 'release/2' } })
    expect(gitOf({ r1: ' ' }, placesOf(app))).toBeUndefined()
  })

  test('issue e pull request viram origens tipadas', () => {
    expect(sourcesOf({ connection: 'c-jira', key: 'APP-1' }, { subject: 'acme/web#7', connection: null })).toEqual([
      { kind: 'pull_request', connection: null, subject: 'acme/web#7' },
      { kind: 'issue', connection: 'c-jira', subject: 'APP-1' },
    ])
    expect(sourcesOf(null, null)).toEqual([])
  })

  test('o mock cria um item de pull request com base', async () => {
    const api = mockApi({ tickMs: 0, delayMs: 0 })
    const apps = await api.applications()
    const chosen = apps[0]
    const repo = chosen.repositories[0]
    const it = await api.createItem({
      application: chosen.id, text: '', issue: null, model: 'claude-opus', effort: 'medium', fast: false, attachments: [],
      sources: [{ kind: 'pull_request', connection: null, subject: 'acme/checkout#9' }],
      git: { [repo.id]: { base: 'develop' } },
    })
    expect(it.sources.map((s) => [s.kind, s.key])).toEqual([['pull_request', 'acme/checkout#9']])
    expect(it.git).toEqual([{ repository: repo.id, name: repo.bindings?.code?.value ?? repo.directory, base: 'develop', head: null }])
  })
})

describe('pickers do composer', () => {
  test('o branch base padrão é o que o repositório declara, senão o primeiro', () => {
    expect(defaultBranch([{ name: 'dev', default: true }, { name: 'main', default: false }])).toBe('dev')
    expect(defaultBranch([{ name: 'develop', default: false }, { name: 'main', default: false }])).toBe('develop')
    expect(defaultBranch([{ name: 'a', default: false }, { name: 'trunk', default: true }])).toBe('trunk')
    expect(defaultBranch(undefined)).toBeNull()
    const places = [{ id: 'r1', name: 'acme/web' }, { id: 'r2', name: 'infra' }]
    expect(basesOf({ r2: 'x' }, { r1: [{ name: 'main', default: true }] }, places)).toEqual({ r1: 'main', r2: 'x' })
  })

  test('os pull requests filtram por número, título, branch e autor', () => {
    const pull = { key: 'acme/web#42', number: 42, title: 'Corrigir frete', head: 'fix/frete', base: 'main', author: 'ana', url: null, draft: false }
    expect(matchesPull(pull, '42')).toBe(true)
    expect(matchesPull(pull, 'FRETE')).toBe(true)
    expect(matchesPull(pull, 'bia')).toBe(false)
  })

  test('só repositórios conectados entram na busca', () => {
    const app: Application = { id: 'a', name: 'A', repositories: [
      { id: 'r1', bindings: { code: { connection: 'c-gh', value: 'acme/web' } }, directory: '/w', run: [], verify: [] },
      { id: 'r2', bindings: {}, directory: '/i', run: [], verify: [] },
    ] }
    expect(linkedRepos(app)).toEqual([{ place: 'r1', connection: 'c-gh', repo: 'acme/web' }])
    expect(linkedRepos(null)).toEqual([])
  })
})
