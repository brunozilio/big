import { describe, expect, test } from 'bun:test'
import { cleanup, render, screen } from '@testing-library/react'
import type { Application, Item } from '@/api/contract'
import { mockApi } from '@/api/mock'
import { gitOf, placesOf, pullSubject, sourcesOf } from '@/screens/items/NewItem'
import { Origins } from '@/screens/items/parts'

const app: Application = {
  id: 'a1', name: 'Loja', tracker: null,
  repositories: [{ id: 'r1', code: { connection: 'c-code', repo: 'acme/web' }, directory: '/p/web', run: '', verify: [], e2e: [] }],
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

  test('o mock cria um item de pull request com base e mostra as origens', async () => {
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
    expect(it.git).toEqual([{ repository: repo.id, name: repo.code?.repo ?? repo.directory, base: 'develop', head: null }])
    render(<Origins it={it as Item} />)
    expect(screen.getByLabelText('Pull request acme/checkout#9')).toBeTruthy()
    expect(screen.getByText('develop')).toBeTruthy()
    expect(screen.getByText('branch novo')).toBeTruthy()
    cleanup()
  })

  test('só uma issue não repete a origem no cabeçalho', () => {
    const it = { sources: [{ kind: 'issue', key: 'APP-1', connection: 'c', repository: null, provider: null, name: null, logo: null, url: null }], git: [] } as unknown as Item
    const view = render(<Origins it={it} />)
    expect(view.container.textContent).toBe('')
    cleanup()
  })
})
