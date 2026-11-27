import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Api, LibraryChange, LibraryDraft, LibraryItem, LibraryTurn } from '../src/api/contract'
import { ApiError, streamed } from '../src/api/http'
import { mockLibrary, stepsOf } from '../src/api/mock/library'
import { LibraryFields } from '../src/screens/settings/LibraryFields'
import { LibraryView } from '../src/screens/settings/Library'
import { askOnOpen, blocked, changed, draftOf, matching, problemOf, talkOf, toggled } from '../src/screens/settings/libraryData'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  setApi(null)
  localStorage.clear()
  document.body.innerHTML = ''
})

function serve() {
  const library = mockLibrary()
  const saves: { path: string; change: LibraryChange }[] = []
  const asks: { path: string; draft: LibraryDraft; messages: LibraryTurn[] }[] = []
  const api = {
    library: async () => library.list(),
    libraryItem: async (path: string) => library.shown(path),
    saveLibraryItem: async (path: string, change: LibraryChange) => {
      saves.push({ path, change })
      return library.save(path, change)
    },
    createLibraryItem: async (section: string, name: string) => library.create(section, name),
    restoreLibraryItem: async (path: string) => library.restore(path),
    createLibraryFile: async (item: string, name: string) => library.createFile(item, name),
    outlineLibrary: async (_path: string, source: string) => stepsOf(source),
    assistLibrary: async (path: string, draft: LibraryDraft, messages: LibraryTurn[]) => {
      asks.push({ path, draft, messages })
      return library.assist(path, draft, messages)
    },
  } as unknown as Api
  setApi(api)
  return { library, saves, asks }
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

describe('etapas do workflow', () => {
  test('o workflow mostra as etapas na ordem, com o agente e para onde volta ao falhar', async () => {
    serve()
    show('/settings/library?section=workflows&item=workflows/default.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'entrega' })
    const steps = within(within(panel).getByText('Etapas').parentElement as HTMLElement).getAllByRole('listitem')
    expect(steps.map((step) => step.textContent)).toEqual(['1planPlanner', '2developDeveloper', '3checksverifier · ↩ develop ×3', '4pr', '5reviewReviewer · ↩ develop', '6merge'])
    expect(within(steps[0]).getByLabelText('Planner')).toBeTruthy()
    fireEvent.click(within(steps[2]).getByRole('button'))
    expect(stepsOf('export default 1')).toEqual([])
  })
})

describe('assistente ao lado do editor', () => {
  test('um pedido vai com o rascunho, a resposta entra no rascunho e pode ser desfeita', async () => {
    const { asks } = serve()
    show('/settings/library?item=agents/planner.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    const chat = within(panel).getByRole('region', { name: 'Assistente' })
    const save = within(panel).getByRole('button', { name: /Salvar/ })
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: 'Planejador' } })
    const box = within(chat).getByRole('textbox', { name: 'Peça uma mudança ou tire uma dúvida' })
    fireEvent.change(box, { target: { value: 'deixa menor' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    await within(chat).findByText('Pronto: enxuguei o texto e mantive a primeira parte.')
    expect(asks[0].path).toBe('agents/planner.ts')
    expect(asks[0].draft.values.label).toBe('Planejador')
    expect(asks[0].messages).toEqual([{ role: 'user', text: 'deixa menor' }])
    expect(within(chat).getByText('Aplicado ao rascunho')).toBeTruthy()
    expect(save).toHaveProperty('disabled', false)
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: 'Planner' } })
    expect(save).toHaveProperty('disabled', false)
    fireEvent.click(within(chat).getByRole('button', { name: 'Desfazer' }))
    expect(within(chat).getByText('Desfeito')).toBeTruthy()
  })

  test('uma pergunta só responde, sem mexer no rascunho, e a sugestão envia direto', async () => {
    const { asks } = serve()
    show('/settings/library?item=agents/planner.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    const chat = within(panel).getByRole('region', { name: 'Assistente' })
    fireEvent.click(within(chat).getByRole('button', { name: 'Explique o que isto faz' }))
    await within(chat).findByText(/Este item está claro/)
    expect(asks).toHaveLength(1)
    expect(within(chat).queryByText('Aplicado ao rascunho')).toBeNull()
    expect(within(panel).getByRole('button', { name: /Salvar/ })).toHaveProperty('disabled', true)
    fireEvent.click(within(panel).getByRole('button', { name: 'Assistente' }))
    expect(within(panel).queryByRole('region', { name: 'Assistente' })).toBeNull()
    fireEvent.click(within(panel).getByRole('button', { name: 'Assistente' }))
  })
  test('a conversa fica guardada por item e um pedido deixado ao criar é enviado ao abrir', async () => {
    const { asks } = serve()
    askOnOpen('agents/reviewer.ts', 'cuida das migrações de banco')
    const first = show('/settings/library?item=agents/reviewer.ts')
    const chat = within(await within(document.body).findByRole('dialog', { name: 'Reviewer' })).getByRole('region', { name: 'Assistente' })
    await within(chat).findByText('Pronto: enxuguei o texto e mantive a primeira parte.')
    expect(asks[0].messages).toEqual([{ role: 'user', text: 'cuida das migrações de banco' }])
    expect(talkOf('agents/reviewer.ts').map((turn) => turn.role)).toEqual(['user', 'assistant'])
    first.unmount()
    document.body.innerHTML = ''
    show('/settings/library?item=agents/reviewer.ts')
    const again = within(await within(document.body).findByRole('dialog', { name: 'Reviewer' })).getByRole('region', { name: 'Assistente' })
    expect(within(again).getByText('cuida das migrações de banco')).toBeTruthy()
    expect(asks).toHaveLength(1)
    fireEvent.click(within(again).getByRole('button', { name: 'Nova conversa' }))
    expect(talkOf('agents/reviewer.ts')).toEqual([])
  })

  test('a resposta em fluxo mostra o texto parcial e termina com o resultado', async () => {
    const body = 'data: {"type":"reply","text":"Fiz"}\n\ndata: {"type":"reply","text":"Fiz a mudança."}\n\ndata: {"type":"done","reply":"Fiz a mudança.","values":{},"body":"x","source":null}\n\n'
    const seen: string[] = []
    const done = await streamed<{ reply: string; body: string }>(async () => new Response(body), '/api/library/assist', {}, (text) => seen.push(text))
    expect(seen).toEqual(['Fiz', 'Fiz a mudança.'])
    expect(done.body).toBe('x')
    const failed = streamed(async () => new Response('data: {"type":"error","message":"sem modelo"}\n\n'), '/x', {})
    expect(failed).rejects.toThrow('sem modelo')
    const refused = streamed(async () => new Response('{"error":{"code":"bad_request","message":"Escreva o que você quer."}}', { status: 400 }), '/x', {})
    expect(refused).rejects.toBeInstanceOf(ApiError)
  })

  test('uma skill ganha um arquivo novo pela lista de arquivos', async () => {
    const { library } = serve()
    show('/settings/library?section=skills&item=skills/verify/SKILL.md')
    const panel = await within(document.body).findByRole('dialog', { name: 'verify' })
    fireEvent.click(within(panel).getByRole('button', { name: 'Novo arquivo' }))
    const input = within(panel).getByRole('textbox', { name: 'Novo arquivo' })
    fireEvent.change(input, { target: { value: 'examples/web.md' } })
    fireEvent.submit(input)
    await waitFor(() => expect(library.shown('skills/verify/SKILL.md').files).toContain('skills/verify/examples/web.md'))
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

  test('um campo com limite mostra a contagem, avisa quando passa e não deixa salvar', async () => {
    serve()
    show('/settings/library?item=agents/planner.ts')
    const panel = await within(document.body).findByRole('dialog', { name: 'Planner' })
    const save = within(panel).getByRole('button', { name: /Salvar/ })
    expect(within(panel).getByText(/^\d+\/1024$/).textContent).toBe(`${'Turns an item into a delivery plan the Developer can carry out.'.length}/1024`)
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Descrição' }), { target: { value: 'x'.repeat(1025) } })
    expect(within(panel).getByText('1025/1024')).toBeTruthy()
    expect(save).toHaveProperty('disabled', true)
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Descrição' }), { target: { value: 'ok' } })
    expect(save).toHaveProperty('disabled', false)
    fireEvent.change(within(panel).getByRole('textbox', { name: 'Nome' }), { target: { value: ' ' } })
    expect(within(panel).getByRole('alert').textContent).toBe('Não pode ficar vazio')
    expect(save).toHaveProperty('disabled', true)
  })

  test('as regras do campo: obrigatório, tamanho em caracteres e formato', () => {
    const name = { key: 'name', label: 'Nome', kind: 'text' as const, value: 'a', required: true, max: 4, pattern: '^[a-z0-9]+(-[a-z0-9]+)*$', hint: 'minúsculas' }
    expect(problemOf(name, 'ab-c')).toBeNull()
    expect(problemOf(name, '')).toEqual({ kind: 'required' })
    expect(problemOf(name, 'abcde')).toEqual({ kind: 'long', size: 5, max: 4 })
    expect(problemOf(name, 'a--b')).toEqual({ kind: 'shape', hint: 'minúsculas' })
    expect(problemOf({ ...name, max: 2, pattern: undefined }, 'çã')).toBeNull()
    const item = { mode: 'form', source: '', fields: [{ ...name, value: 'a--b' }], body: null } as unknown as LibraryItem
    expect(blocked(item, { values: { name: 'a--b' }, body: null, source: '' })).toBe(false)
    expect(blocked(item, { values: { name: 'a--c' }, body: null, source: '' })).toBe(true)
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
