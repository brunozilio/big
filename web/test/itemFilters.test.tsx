import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { setApi } from '../src/api'
import { BTW_WORKFLOW, type Item } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { boot, stop } from '../src/api/store'
import { filterItems, ItemsView } from '../src/screens/items/ItemsView'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

const it = (id: string, workflow: string, archivedAt: string | null) => ({ id, workflow, archivedAt }) as Item

describe('filtros da lista', () => {
  const items = [it('a', 'default', null), it('b', BTW_WORKFLOW, null), it('c', BTW_WORKFLOW, '2026-01-01T00:00:00Z'), it('d', 'default', '2026-01-01T00:00:00Z')]
  const ids = (list: Item[]) => list.map((x) => x.id)

  test('arquivadas ficam de fora até pedir por elas', () => {
    expect(ids(filterItems(items, { kind: 'all', archived: false }))).toEqual(['a', 'b'])
    expect(ids(filterItems(items, { kind: 'all', archived: true }))).toEqual(['c', 'd'])
  })

  test('separa perguntas das atividades', () => {
    expect(ids(filterItems(items, { kind: 'btw', archived: false }))).toEqual(['b'])
    expect(ids(filterItems(items, { kind: 'work', archived: false }))).toEqual(['a'])
    expect(ids(filterItems(items, { kind: 'btw', archived: true }))).toEqual(['c'])
  })

  test('a pergunta aparece marcada e os filtros respondem', async () => {
    const api = mockApi({ tickMs: 0, delayMs: 0 })
    setApi(api)
    await act(() => boot(api))
    await act(async () => void (await api.btw({ text: 'o swagger está atualizado?', application: 'a1' })))
    const view = render(<TooltipProvider><ItemsView onOpen={() => undefined} onConnect={() => undefined} fresh={null} /></TooltipProvider>)
    await waitFor(() => expect(view.getAllByText('Pergunta').length).toBe(1))
    fireEvent.click(view.getByRole('tab', { name: 'Perguntas' }))
    expect(view.getByRole('tab', { name: 'Perguntas' }).getAttribute('aria-selected')).toBe('true')
    expect(view.getAllByText('Pergunta').length).toBe(1)
    const archived = view.getByRole('button', { name: /Arquivadas/ })
    fireEvent.click(archived)
    expect(archived.getAttribute('aria-pressed')).toBe('true')
    await waitFor(() => expect(view.getByText('Nada arquivado.')).toBeTruthy())
  })
})
