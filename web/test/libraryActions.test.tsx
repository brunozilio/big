import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Api, LibraryItem } from '../src/api/contract'
import { mockLibrary, stepsOf } from '../src/api/mock/library'
import { LibraryView } from '../src/screens/settings/Library'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  setApi(null)
  localStorage.clear()
  document.body.innerHTML = ''
})

const PROPOSED = 'agents/planner.ts'
const APPROVED = 'agents/developer.ts'

function serve() {
  const library = mockLibrary()
  const acted: { path: string; action: string }[] = []
  const declared = (item: LibraryItem): LibraryItem => (item.path === PROPOSED ? { ...item, actions: [{ key: 'approve', label: 'Aprovar', hint: 'Entra no banco' }] } : item)
  const api = {
    library: async () => library.list(),
    libraryItem: async (path: string) => declared(library.shown(path)),
    saveLibraryItem: async (path: string, change: Parameters<typeof library.save>[1]) => declared(library.save(path, change)),
    outlineLibrary: async (_path: string, source: string) => stepsOf(source),
    actLibrary: async (path: string, action: string) => {
      acted.push({ path, action })
      return library.shown(APPROVED)
    },
  } as unknown as Api
  setApi(api)
  return { acted, library }
}

const show = (at: string) => render(<MemoryRouter initialEntries={[at]}><TooltipProvider><LibraryView /></TooltipProvider></MemoryRouter>)

describe('ações declaradas pelo conteúdo', () => {
  test('um item mostra as ações que o conteúdo declara e agir pode levar a outro caminho', async () => {
    const { acted } = serve()
    show(`/settings/library?item=${PROPOSED}`)
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    const approve = within(panel).getByRole('button', { name: 'Aprovar' })
    expect(approve.getAttribute('title')).toBe('Entra no banco')
    fireEvent.click(approve)
    await waitFor(() => expect(acted).toEqual([{ path: PROPOSED, action: 'approve' }]))
    await within(document.body).findByRole('dialog', { name: 'Developer' })
    expect(within(document.body).queryByRole('button', { name: 'Aprovar' })).toBeNull()
  })

  test('com alterações por salvar, agir salva antes', async () => {
    const { acted, library } = serve()
    show(`/settings/library?item=${PROPOSED}`)
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: 'Planejador' } })
    fireEvent.click(within(panel).getByRole('button', { name: 'Aprovar' }))
    await waitFor(() => expect(acted).toHaveLength(1))
    expect(library.shown(PROPOSED).title).toBe('Planejador')
  })
})
