import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Api, LibraryChange, LibraryItem } from '../src/api/contract'
import { mockLibrary } from '../src/api/mock/library'
import { LibraryFields } from '../src/screens/settings/LibraryFields'
import { LibraryView } from '../src/screens/settings/Library'
import { changed, draftOf, matching, toggled } from '../src/screens/settings/libraryData'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  setApi(null)
  document.body.innerHTML = ''
})

function serve() {
  const library = mockLibrary()
  const saves: { path: string; change: LibraryChange }[] = []
  const api = {
    library: async () => library.list(),
    libraryItem: async (path: string) => library.shown(path),
    saveLibraryItem: async (path: string, change: LibraryChange) => {
      saves.push({ path, change })
      return library.save(path, change)
    },
    createLibraryItem: async (section: string, name: string) => library.create(section, name),
    restoreLibraryItem: async (path: string) => library.restore(path),
  } as unknown as Api
  setApi(api)
  return { library, saves }
}

const show = (at = '/settings/library') => render(<MemoryRouter initialEntries={[at]}><TooltipProvider><LibraryView /></TooltipProvider></MemoryRouter>)

describe('biblioteca', () => {
  test('lista as seções declaradas e os itens da seção escolhida', async () => {
    serve()
    const view = show()
    const agents = await view.findByRole('list', { name: 'Agentes' })
    expect(within(agents).getAllByRole('listitem').map((row) => row.textContent)).toEqual([expect.stringContaining('Planner'), expect.stringContaining('Developer'), expect.stringContaining('Reviewer')])
    fireEvent.click(view.getByRole('button', { name: /Skills/ }))
    const skills = await view.findByRole('list', { name: 'Skills' })
    expect(within(skills).getAllByRole('listitem')).toHaveLength(2)
    expect(view.queryByRole('list', { name: 'Agentes' })).toBeNull()
  })

  test('a busca filtra por título, descrição e caminho', async () => {
    serve()
    const view = show()
    await view.findByRole('list', { name: 'Agentes' })
    fireEvent.change(view.getByRole('textbox', { name: 'Buscar' }), { target: { value: 'colleague' } })
    expect(within(view.getByRole('list', { name: 'Agentes' })).getAllByRole('listitem')).toHaveLength(1)
    fireEvent.change(view.getByRole('textbox', { name: 'Buscar' }), { target: { value: 'nada disso' } })
    expect(view.queryByRole('list', { name: 'Agentes' })).toBeNull()
    expect(view.getByText('Nada por aqui')).toBeTruthy()
  })

  test('abrir um agente mostra o formulário, e salvar envia só os campos e o prompt', async () => {
    const { saves } = serve()
    const view = show('/settings/library?item=agents/developer.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Developer' })
    const save = within(panel).getByRole('button', { name: /Salvar/ })
    expect(save).toHaveProperty('disabled', true)
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: 'Builder' } })
    expect(save).toHaveProperty('disabled', false)
    fireEvent.click(save)
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0].path).toBe('agents/developer.ts')
    expect(saves[0].change).toMatchObject({ values: { label: 'Builder', tools: ['Read', 'Grep', 'Glob', 'Bash', 'Edit', 'Write'] } })
    expect('source' in saves[0].change).toBe(false)
    await within(document.body).findByRole('dialog', { name: 'Builder' })
    await waitFor(() => expect(within(document.body).getAllByText('Editado')).toHaveLength(2))
    expect(within(view.getByRole('list', { name: 'Agentes' })).getByText('Builder')).toBeTruthy()
  })

  test('sair com alterações pede confirmação antes de descartar', async () => {
    serve()
    show('/settings/library?item=agents/planner.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: 'Outro' } })
    fireEvent.click(within(panel).getByRole('button', { name: 'Voltar' }))
    const ask = within(panel).getByRole('alertdialog')
    expect(within(document.body).queryByRole('dialog')).not.toBeNull()
    fireEvent.click(within(ask).getByRole('button', { name: 'Descartar' }))
    await waitFor(() => expect(within(document.body).queryByRole('dialog')).toBeNull())
  })

  test('restaurar um item editado volta ao original', async () => {
    const { library } = serve()
    library.save('agents/planner.ts', { values: { label: 'Mudado' }, body: null })
    show('/settings/library?item=agents/planner.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Mudado' })
    fireEvent.click(within(panel).getByRole('button', { name: 'Restaurar padrão' }))
    fireEvent.click(within(within(panel).getByRole('alertdialog')).getByRole('button', { name: 'Restaurar padrão' }))
    await within(document.body).findByRole('dialog', { name: 'Planner' })
    expect(library.shown('agents/planner.ts').origin).toBe('shipped')
  })
})

describe('campos declarados pelo conteúdo', () => {
  const fields = [
    { key: 'name', label: 'Nome', kind: 'text' as const, value: 'a' },
    { key: 'only', label: 'Só pelo nome', kind: 'switch' as const, value: false },
    { key: 'color', label: 'Cor', kind: 'color' as const, value: '#123456' },
    { key: 'model', label: 'Modelo', kind: 'text' as const, value: '', more: true },
  ]

  test('cada tipo vira o seu controle e os extras ficam recolhidos', () => {
    const seen: [string, unknown][] = []
    const view = render(<TooltipProvider><LibraryFields fields={fields} values={{ name: 'a', only: false, color: '#123456', model: '' }} disabled={false} onChange={(key, value) => seen.push([key, value])} /></TooltipProvider>)
    fireEvent.click(view.getByRole('switch', { name: 'Só pelo nome' }))
    expect(view.getByRole('radio', { name: '#123456' }).getAttribute('aria-checked')).toBe('true')
    fireEvent.click(view.getAllByRole('radio')[0])
    expect(seen[0]).toEqual(['only', true])
    expect(seen[1][0]).toBe('color')
    expect(view.queryByRole('textbox', { name: 'Modelo' })).toBeNull()
    fireEvent.click(view.getByRole('button', { name: 'Mais opções' }))
    expect(view.getByRole('textbox', { name: 'Modelo' })).toBeTruthy()
  })

  test('o rascunho só conta como alterado quando difere do que veio', () => {
    const item = { mode: 'form', source: 's', fields: [{ key: 'tools', label: 'T', kind: 'chips', value: ['a', 'b'] }], body: { label: 'P', text: 'x', language: 'markdown' } } as unknown as LibraryItem
    const draft = draftOf(item)
    expect(changed(item, draft)).toBe(false)
    expect(changed(item, { ...draft, values: { tools: ['a', 'b'] } })).toBe(false)
    expect(changed(item, { ...draft, values: { tools: toggled(['a', 'b'], 'b') } })).toBe(true)
    expect(changed(item, { ...draft, body: 'y' })).toBe(true)
    expect(changed({ ...item, mode: 'code' }, { ...draft, body: 'y' })).toBe(false)
    expect(matching([{ path: 'agents/a.ts', title: 'Alfa', description: null, avatar: null, origin: 'shipped', files: [] }], 'agents/')).toHaveLength(1)
  })
})
