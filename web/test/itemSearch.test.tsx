import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Item } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { boot, getState, stop } from '../src/api/store'
import { filterItems, ItemsView, matchesQuery } from '../src/screens/items/ItemsView'
import { readView, toggleClosed, writeView } from '../src/screens/items/viewState'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

const it = (over: Partial<Item>) => ({ id: 'x', workflow: 'default', archivedAt: null, title: '', issue: null, change: null, sources: [], git: [], ...over }) as Item

describe('busca da lista', () => {
  const login = it({ id: 'a', title: 'Corrigir Login', issue: { key: 'APP-12' } as Item['issue'] })
  const pull = it({ id: 'b', title: 'Revisar', change: { key: '#481' } as Item['change'], sources: [{ kind: 'branch', key: 'feat/Checkout', repository: 'acme/web' } as Item['sources'][number]] })

  test('acha por título, issue, pull request e origem, sem ligar para maiúsculas', () => {
    expect(matchesQuery(login, 'login')).toBe(true)
    expect(matchesQuery(login, 'app-12')).toBe(true)
    expect(matchesQuery(pull, '#481')).toBe(true)
    expect(matchesQuery(pull, 'CHECKOUT')).toBe(true)
    expect(matchesQuery(pull, 'acme/web')).toBe(true)
    expect(matchesQuery(pull, 'login')).toBe(false)
    expect(matchesQuery(login, 'corrigir app')).toBe(true)
    expect(matchesQuery(login, '   ')).toBe(true)
  })

  test('a busca se soma aos outros filtros e o pacote aparece pelo membro', () => {
    const pack = it({ id: 'p', title: 'Pacote', workflow: 'package', links: ['a'] })
    const all = [login, pull, pack]
    expect(filterItems(all, { kind: 'all', archived: false, query: 'login' }).map((x) => x.id)).toEqual(['a', 'p'])
    expect(filterItems(all, { kind: 'btw', archived: false, query: 'login' })).toEqual([])
    expect(filterItems(all, { kind: 'all', archived: false, query: '' }).length).toBe(3)
  })
})

describe('estado da lista na URL', () => {
  test('lê e escreve sem mexer nos parâmetros dos outros', () => {
    const start = new URLSearchParams('connect=jira&flow=f1&group=x&edit=1')
    const next = writeView(start, { kind: 'btw', archived: true, query: 'app 12', closed: ['delivered', 'you'] })
    expect(next.get('connect')).toBe('jira')
    expect(next.get('flow')).toBe('f1')
    expect(next.get('group')).toBe('x')
    expect(next.get('edit')).toBe('1')
    expect(readView(next)).toEqual({ kind: 'btw', archived: true, query: 'app 12', closed: ['you', 'delivered'] })
    const back = writeView(next, { kind: 'all', archived: false, query: '', closed: [] })
    expect(back.toString()).toBe('connect=jira&flow=f1&group=x&edit=1')
  })

  test('valores estranhos caem no padrão', () => {
    expect(readView(new URLSearchParams('kind=zzz&archived=yes&closed=nope,running'))).toEqual({ kind: 'all', archived: false, query: '', closed: ['running'] })
    expect(toggleClosed(['you'], 'you')).toEqual([])
    expect(toggleClosed([], 'running')).toEqual(['running'])
  })

  async function open(path: string) {
    const api = mockApi({ tickMs: 0, delayMs: 0 })
    setApi(api)
    await act(() => boot(api))
    const at = { search: '' }
    function Probe() {
      at.search = useLocation().search
      return null
    }
    const view = render(<MemoryRouter initialEntries={[path]}><Probe /><TooltipProvider><ItemsView onOpen={() => undefined} onConnect={() => undefined} fresh={null} /></TooltipProvider></MemoryRouter>)
    return { at, view }
  }

  test('a URL restaura filtro, arquivadas, busca e grupos fechados', async () => {
    const { view } = await open('/?kind=work&archived=1&q=zzz&closed=you&connect=jira')
    expect(view.getByRole('tab', { name: 'Atividades' }).getAttribute('aria-selected')).toBe('true')
    expect(view.getByRole('button', { name: /Arquivadas/ }).getAttribute('aria-pressed')).toBe('true')
    expect((view.getByRole('searchbox') as HTMLInputElement).value).toBe('zzz')
    expect(view.getByRole('button', { name: /Com você/ }).getAttribute('aria-expanded')).toBe('false')
    expect(view.getByRole('button', { name: /Em andamento/ }).getAttribute('aria-expanded')).toBe('true')
    expect(view.getAllByText('Nada encontrado para “zzz”.').length).toBeGreaterThan(0)
  })

  test('mexer na lista grava na URL e preserva o resto', async () => {
    const { at, view } = await open('/?connect=jira')
    const title = getState().items.find((x) => !x.archivedAt && x.group !== 'delivered')?.title ?? ''
    expect(title).not.toBe('')
    fireEvent.click(view.getByRole('tab', { name: 'Perguntas' }))
    fireEvent.click(view.getByRole('button', { name: /Em andamento/ }))
    await waitFor(() => expect(at.search).toBe('?connect=jira&kind=btw&closed=running'))
    fireEvent.click(view.getByRole('tab', { name: 'Todas' }))
    fireEvent.click(view.getByRole('button', { name: /Em andamento/ }))
    const search = view.getByRole('searchbox') as HTMLInputElement
    fireEvent.change(search, { target: { value: title } })
    await waitFor(() => expect(new URLSearchParams(at.search).get('q')).toBe(title))
    expect(new URLSearchParams(at.search).get('connect')).toBe('jira')
    expect(view.getAllByText(title).length).toBeGreaterThan(0)
    fireEvent.keyDown(search, { key: 'Escape' })
    await waitFor(() => expect(at.search).toBe('?connect=jira'))
    expect(search.value).toBe('')
  })

  test('a barra foca a busca', async () => {
    const { view } = await open('/')
    const search = view.getByRole('searchbox')
    expect(document.activeElement).not.toBe(search)
    fireEvent.keyDown(document.body, { key: '/' })
    expect(document.activeElement).toBe(search)
  })
})
