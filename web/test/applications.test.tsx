import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, Application, ApplicationDraft, Connections, DirListing, NewItem as NewItemInput } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { MOCK_HOME } from '../src/api/mock/applications'
import { boot, getState, stop } from '../src/api/store'
import { NewItem } from '../src/screens/items/NewItem'
import { ApplicationsView } from '../src/screens/settings/Applications'
import { fieldsOf, groupedBySection, suggested, withBinding, withChoices } from '../src/screens/settings/bindings'
import { FolderDialog } from '../src/screens/settings/FolderDialog'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { TooltipProvider } from '../src/ui/overlay'
import { clearToasts, Toaster } from '../src/ui/toast'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })

async function started(api: Api = fresh()) {
  setApi(api)
  await act(async () => {
    await boot(api)
  })
  return api
}

function Where() {
  return <span data-testid="where">{useLocation().search}</span>
}

function listed(path = '/settings/applications') {
  return render(<MemoryRouter initialEntries={[path]}><TooltipProvider><Where /><Toaster closeLabel="Fechar aviso" /><ApplicationsView /></TooltipProvider></MemoryRouter>)
}

function expandRepositories(view: ReturnType<typeof listed>) {
  const closed = Array.from(view.container.querySelectorAll<HTMLElement>('[data-testid^="repository-"] button[aria-expanded="false"]'))
  closed.forEach((button) => fireEvent.click(button))
}

function openSection(view: ReturnType<typeof listed>, name: RegExp) {
  const header = view.getByRole('button', { name })
  if (header.getAttribute('aria-expanded') === 'false') fireEvent.click(header)
}

function withConnections(change: (found: Connections) => Connections): Api {
  const base = fresh()
  return { ...base, connections: async () => change(await base.connections()) }
}

beforeEach(() => localStorage.clear())
afterEach(() => {
  cleanup()
  clearToasts()
  stop()
  setApi(null)
})

describe('diálogo de pasta', () => {
  test('começa no início, entra nas pastas, mostra os repositórios e devolve a escolhida', async () => {
    setApi(fresh())
    const chosen: DirListing[] = []
    const view = render(<TooltipProvider><FolderDialog open start={null} onClose={() => undefined} onChoose={(l) => chosen.push(l)} /></TooltipProvider>)
    const list = await view.findByRole('list', { name: 'Pastas' })
    await waitFor(() => expect(within(list).getByText('projetos')).toBeTruthy())
    expect(view.getByTestId('folder-path').textContent).toBe(MOCK_HOME)
    fireEvent.click(within(list).getByText('projetos'))
    await waitFor(() => expect(within(list).getByText('checkout')).toBeTruthy())
    fireEvent.click(within(list).getByText('web'))
    await waitFor(() => expect(within(list).getByText('src')).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Pasta de cima' }))
    await waitFor(() => expect(within(list).getByText('rascunhos')).toBeTruthy())
    expect(view.getByTestId('folder-path').textContent).toBe(`${MOCK_HOME}/projetos`)
    fireEvent.click(await within(list).findByText('web'))
    await waitFor(() => expect(within(list).getByText('src')).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Escolher' }))
    expect(chosen[0].path).toBe(`${MOCK_HOME}/projetos/web`)
    expect(chosen[0].remote.ref).toBe('acme/web')
  })

  test('os valores extras de um vínculo não repetem o principal nem entre si', () => {
    const binding = { connection: 'c', value: 'APP', label: 'App' }
    expect(withChoices(binding, [{ value: 'OPS' }, { value: 'APP' }, { value: 'OPS' }])).toEqual({ ...binding, more: [{ value: 'OPS' }] })
    expect(withChoices({ ...binding, more: [{ value: 'OPS' }] }, [])).toEqual(binding)
  })

  test('o vínculo do repositório é sugerido pelo remote do git entre as opções do conector', async () => {
    const github = { key: 'github', name: 'GitHub', group: 'code', description: '', logo: '', signIn: 'device' as const, fields: [], multiple: true }
    const connection = { id: 'c-github', provider: 'github', label: 'acme', account: '', capabilities: ['code'], status: 'ok' as const }
    const fields = [{ capability: 'code', label: 'GitHub', sources: [{ connection, provider: github }] }]
    const listed = [{ value: 'acme/web', label: 'acme/web' }]
    setApi({ ...fresh(), call: (async (_connection: string, _action: string, args: { query?: string }) => listed.filter((o) => o.value === (args.query ?? '').toLowerCase())) as Api['call'] })
    expect(await suggested('ACME/web', fields, {})).toEqual({ code: { connection: 'c-github', value: 'acme/web', label: 'acme/web' } })
    expect(await suggested('acme/outro', fields, {})).toEqual({})
    const kept = { code: { connection: 'c-github', value: 'acme/api' } }
    expect(await suggested('acme/web', fields, kept)).toEqual(kept)
    expect(withBinding(kept, 'code', null)).toEqual({})
  })

  test('só há campo de vínculo quando um conector conectado o declara, no escopo que ele pede', () => {
    const veracode = { key: 'veracode', name: 'Veracode', group: 'security', description: '', logo: '', signIn: 'token' as const, fields: [], multiple: true, bindings: [{ capability: 'security', scope: 'repository' as const, label: 'Veracode' }] }
    const connection = { id: 'c-v', provider: 'veracode', label: 'Veracode', account: '', capabilities: ['security'], status: 'ok' as const }
    expect(fieldsOf([connection], [veracode], 'repository').map((f) => f.label)).toEqual(['Veracode'])
    expect(fieldsOf([connection], [veracode], 'application')).toEqual([])
    expect(fieldsOf([], [veracode], 'repository')).toEqual([])
  })
})

describe('aba Aplicações', () => {
  test('lista, cria pela pasta com o repositório sugerido e remove', async () => {
    await started()
    const view = listed()
    expect(view.getByText('Checkout')).toBeTruthy()
    fireEvent.click(view.getByRole('button', { name: 'Nova aplicação' }))
    fireEvent.click(await view.findByRole('button', { name: 'Diretório local' }))
    const list = await view.findByRole('list', { name: 'Pastas' })
    fireEvent.click(await within(list).findByText('projetos'))
    fireEvent.click(await within(list).findByText('web'))
    await waitFor(() => expect(within(list).getByText('src')).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Escolher' }))
    const name = (await view.findByLabelText('Nome')) as HTMLInputElement
    await waitFor(() => expect(name.value).toBe('web'))
    expect((await view.findByRole('combobox', { name: 'GitHub' }, { timeout: 3000 })).textContent).toContain('acme/web')
    expect(view.queryByLabelText('Rodar 1')).toBeNull()
    expect(view.getByRole('combobox', { name: 'Tipo' }).textContent).toContain('Aplicação')
    fireEvent.click(view.getByRole('button', { name: 'Variáveis de ambiente' }))
    fireEvent.change(await view.findByLabelText('Nome 1'), { target: { value: 'CYPRESS_BASE_URL' } })
    fireEvent.change(view.getByLabelText('Valor 1'), { target: { value: 'http://localhost:3000' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.length).toBe(2))
    const made = getState().applications?.[1]
    expect(made?.repositories[0]).toMatchObject({ directory: `${MOCK_HOME}/projetos/web`, bindings: { code: { connection: 'c-github', value: 'acme/web' } }, kind: 'application', run: [], verify: [], e2e: [] })
    expect(made?.env).toEqual({ CYPRESS_BASE_URL: 'http://localhost:3000' })
    await waitFor(() => expect(view.getByRole('button', { name: 'Remover web' })).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Remover web' }))
    expect(getState().applications?.length).toBe(2)
    fireEvent.click(await view.findByRole('button', { name: 'Remover' }))
    await waitFor(() => expect(getState().applications?.length).toBe(1))
  })

  test('não salva sem nome e mostra o erro', async () => {
    await started()
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const name = (await view.findByLabelText('Nome')) as HTMLInputElement
    expect(name.value).toBe('Checkout')
    expect(view.queryByRole('dialog')).toBeNull()
    expect(view.queryAllByLabelText('Diretório local').length).toBe(0)
    expandRepositories(view)
    expect(view.getAllByLabelText('Diretório local').length).toBe(2)
    fireEvent.change(name, { target: { value: ' ' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    expect((await view.findByRole('alert')).textContent).toContain('nome')
  })

  test('cada repositório escolhe o fluxo de branch, com o padrão já selecionado', async () => {
    await started()
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expandRepositories(view)
    const selects = await view.findAllByRole('combobox', { name: 'Fluxo de branch' })
    expect(selects.length).toBe(2)
    await waitFor(() => expect(selects[0].textContent).toContain('gitflow'))
    expect(selects[1].textContent).toContain('gitflow')
  })

  test('o dependabot não aparece: é sempre ignorado; e repositório sem comando não diz nada', async () => {
    await started()
    const view = listed()
    expect(view.queryByText(/dependabot/i)).toBeNull()
    expect(view.queryByText('Sem comandos')).toBeNull()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expect(view.queryByText(/dependabot/i)).toBeNull()
  })
})

describe('conta e papel na nuvem', () => {
  const withCloud = (): Api => {
    const base = fresh()
    const aws = { id: 'c-aws', provider: 'aws', label: 'AWS', account: 'acme', capabilities: ['cloud'], status: 'ok' as const }
    return { ...base, connections: async () => {
      const found = await base.connections()
      return { ...found, connections: [...found.connections, aws] }
    } }
  }

  test('sem conexão de nuvem o campo não aparece', async () => {
    await started()
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expect(view.queryByTestId('binding-cloud')).toBeNull()
  })

  test('a aplicação escolhe a conta e o papel de cada ambiente', async () => {
    const base = withCloud()
    const asked: string[] = []
    const chosenFor: Record<string, unknown> = {}
    const spy = { ...base, call: ((connection, action, args) => {
      const given = args as { field?: string; chosen?: Record<string, string> }
      asked.push(`${connection} ${action} ${given.field ?? ''}`)
      if (given.field) chosenFor[given.field] = given.chosen
      return base.call(connection, action, args)
    }) as typeof base.call }
    await started(spy)
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    expect(within(cloud).getByText('AWS')).toBeTruthy()
    fireEvent.click(within(cloud).getByRole('button', { name: 'Adicionar Ambiente' }))
    const pick = async (name: string, typed: string, option: string) => {
      fireEvent.keyDown(await within(cloud).findByRole('combobox', { name }), { key: 'Enter' })
      fireEvent.change(await view.findByRole('textbox', { name: `${name}…` }), { target: { value: typed } })
      fireEvent.keyDown(await view.findByRole('option', { name: option }), { key: 'Enter' })
    }
    await pick('Ambiente', 'prod', 'Produção')
    expect(asked).toContain('c-aws cloud.options environment')
    await pick('Conta', 'prod', 'acme-prod')
    expect(asked).toContain('c-aws cloud.options account')
    await pick('Papel', 'dep', 'Deploy')
    expect(chosenFor.role).toEqual({ account: '111122223333', environment: 'prod' })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.bindings?.cloud).toEqual({
      connection: 'c-aws',
      value: '',
      profiles: [{ environment: { value: 'prod', label: 'Produção' }, account: { value: '111122223333', label: 'acme-prod' }, role: { value: 'Deploy', label: 'Deploy' } }],
    }))
  })

  test('a nuvem é opcional: ambiente adicionado e deixado vazio não impede salvar', async () => {
    const base = withCloud()
    let saved = 0
    await started({ ...base, saveApplication: (draft) => ((saved += 1), base.saveApplication(draft)) })
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    fireEvent.click(within(cloud).getByRole('button', { name: 'Adicionar Ambiente' }))
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(saved).toBe(1))
    expect(view.queryByRole('alert')).toBeNull()
  })

  test('ambiente e conta listam ao abrir, sem digitar, e a falha do conector aparece', async () => {
    const base = withCloud()
    const failing = { ...base, call: ((connection, action, args) => ((args as { field?: string }).field === 'account' ? Promise.reject(new Error('Sessão da AWS expirou')) : base.call(connection, action, args))) as typeof base.call }
    await started(failing)
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    fireEvent.click(within(cloud).getByRole('button', { name: 'Adicionar Ambiente' }))
    fireEvent.keyDown(await within(cloud).findByRole('combobox', { name: 'Ambiente' }), { key: 'Enter' })
    expect(await view.findByRole('option', { name: 'Produção' })).toBeTruthy()
    fireEvent.keyDown(view.getByRole('option', { name: 'Produção' }), { key: 'Escape' })
    fireEvent.keyDown(await within(cloud).findByRole('combobox', { name: 'Conta' }), { key: 'Enter' })
    expect(await view.findByText('Sessão da AWS expirou')).toBeTruthy()
  })

  test('a ligação com uma conexão que não existe mais usa a conexão atual', async () => {
    const base = withCloud()
    const asked: string[] = []
    const stale = { connection: 'c-antiga', value: '', profiles: [{ environment: { value: 'prod', label: 'Produção' } }] }
    const spy = {
      ...base,
      applications: async () => (await base.applications()).map((a, i) => (i === 0 ? { ...a, bindings: { ...a.bindings, cloud: stale } } : a)),
      call: ((connection, action, args) => (asked.push(connection), base.call(connection, action, args))) as typeof base.call,
    }
    await started(spy)
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    fireEvent.keyDown(await within(cloud).findByRole('combobox', { name: 'Conta' }), { key: 'Enter' })
    expect(await view.findByRole('option', { name: 'acme-prod' })).toBeTruthy()
    expect(asked).not.toContain('c-antiga')
  })
})

describe('edição no lugar', () => {
  const ASK = 'Descartar as alterações não salvas?'
  const cloudy = () => withConnections((found) => ({ ...found, connections: [...found.connections, { id: 'c-aws', provider: 'aws', label: 'AWS', account: 'acme', capabilities: ['cloud'], status: 'ok' as const }] }))
  const nameOf = (view: ReturnType<typeof listed>) => (view.getByLabelText('Nome') as HTMLInputElement).value

  test('o link ?edit= reabre o editor sem diálogo, e fechar tira só esse parâmetro', async () => {
    await started()
    const id = getState().applications?.[0]?.id ?? ''
    const view = listed(`/settings/applications?group=code&edit=${id}`)
    await view.findByLabelText('Nome')
    expect(nameOf(view)).toBe('Checkout')
    expect(view.queryByRole('dialog')).toBeNull()
    expect(view.queryByTestId(`application-${id}`)).toBeNull()
    fireEvent.click(view.getByRole('button', { name: 'Cancelar' }))
    await waitFor(() => expect(view.getByTestId('where').textContent).toBe('?group=code'))
    expect(view.queryByLabelText('Nome')).toBeNull()
    expect(view.getByTestId(`application-${id}`)).toBeTruthy()
    fireEvent.click(view.getByRole('button', { name: 'Nova aplicação' }))
    await waitFor(() => expect(view.getByTestId('where').textContent).toBe('?group=code&edit=new'))
    expect(nameOf(view)).toBe('')
  })

  test('alterações não salvas pedem confirmação ao cancelar, no Esc e ao trocar de cartão', async () => {
    await started()
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    const leaving = () => {
      const event = new Event('beforeunload', { cancelable: true })
      window.dispatchEvent(event)
      return event.defaultPrevented
    }
    expect(view.queryByText('Alterações não salvas')).toBeNull()
    expect(leaving()).toBe(false)
    fireEvent.change(view.getByLabelText('Nome'), { target: { value: 'Loja' } })
    expect(view.getByText('Alterações não salvas')).toBeTruthy()
    expect(leaving()).toBe(true)
    fireEvent.click(view.getByRole('button', { name: 'Cancelar' }))
    expect(view.getByText(ASK)).toBeTruthy()
    fireEvent.click(view.getByRole('button', { name: 'Continuar editando' }))
    expect(view.queryByText(ASK)).toBeNull()
    expect(nameOf(view)).toBe('Loja')
    fireEvent.keyDown(view.getByLabelText('Nome'), { key: 'Escape' })
    expect(view.getByText(ASK)).toBeTruthy()
    fireEvent.keyDown(view.getByLabelText('Nome'), { key: 'Escape' })
    expect(view.queryByText(ASK)).toBeNull()
    fireEvent.click(view.getByRole('button', { name: 'Nova aplicação' }))
    expect(view.getByText(ASK)).toBeTruthy()
    expect(nameOf(view)).toBe('Loja')
    fireEvent.click(view.getByRole('button', { name: 'Descartar' }))
    await waitFor(() => expect(view.getByTestId('where').textContent).toBe('?edit=new'))
    expect(nameOf(view)).toBe('')
    expect(getState().applications?.[0]?.name).toBe('Checkout')
    expect(leaving()).toBe(false)
    fireEvent.click(view.getByRole('button', { name: 'Cancelar' }))
    await waitFor(() => expect(view.queryByLabelText('Nome')).toBeNull())
    expect(view.queryByText(ASK)).toBeNull()
  })

  test('vínculo incompleto aparece junto do campo, reabre a seção e não salva', async () => {
    const base = cloudy()
    let saved = 0
    await started({ ...base, saveApplication: (draft) => ((saved += 1), base.saveApplication(draft)) })
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    fireEvent.click(within(cloud).getByRole('button', { name: 'Adicionar Ambiente' }))
    fireEvent.keyDown(await within(cloud).findByRole('combobox', { name: 'Ambiente' }), { key: 'Enter' })
    fireEvent.keyDown(await view.findByRole('option', { name: 'Produção' }), { key: 'Enter' })
    await waitFor(() => expect(within(cloud).getByRole('combobox', { name: 'Ambiente' }).textContent).toContain('Produção'))
    fireEvent.click(view.getByRole('button', { name: /^Nuvem/ }))
    expect(view.getByRole('button', { name: /^Nuvem/ }).getAttribute('aria-expanded')).toBe('false')
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    expect(view.getByRole('button', { name: /^Nuvem/ }).getAttribute('aria-expanded')).toBe('true')
    const flagged = await view.findByTestId('binding-cloud')
    expect(flagged.getAttribute('data-invalid')).toBe('true')
    expect(within(flagged).getByTestId('binding-error').textContent).toContain('Conta (Produção)')
    expect(view.getByRole('alert').textContent).toContain('Conta (Produção)')
    expect(saved).toBe(0)
    expect(getState().applications?.[0]?.bindings?.cloud).toBeUndefined()
  })

  test('salvar é otimista: o cartão muda na hora e volta atrás, com aviso e o rascunho, se o servidor recusar', async () => {
    const base = fresh()
    let refuse: (reason: Error) => void = () => undefined
    await started({ ...base, saveApplication: () => new Promise<Application>((_, reject) => (refuse = reject)) })
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    fireEvent.change(await view.findByLabelText('Nome'), { target: { value: 'Loja' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.name).toBe('Loja'))
    expect(view.queryByLabelText('Nome')).toBeNull()
    expect(view.getByText('Salvando…')).toBeTruthy()
    expect(view.queryByRole('button', { name: 'Remover Loja' })).toBeNull()
    await act(async () => refuse(new Error('Servidor fora do ar')))
    await waitFor(() => expect(getState().applications?.[0]?.name).toBe('Checkout'))
    expect((await view.findByRole('alert')).textContent).toContain('Servidor fora do ar')
    await view.findByLabelText('Nome')
    expect(nameOf(view)).toBe('Loja')
    expect(view.getByText('Alterações não salvas')).toBeTruthy()
  })

  test('criar e remover também são otimistas e desfazem quando falham', async () => {
    const base = fresh()
    const failing: Api = { ...base, createApplication: (_draft: ApplicationDraft) => Promise.reject(new Error('Sem permissão')), deleteApplication: () => Promise.reject(new Error('Não deu para remover')) }
    await started(failing)
    const view = listed('/settings/applications?edit=new')
    fireEvent.change(await view.findByLabelText('Nome'), { target: { value: 'Nova' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    expect((await view.findByRole('alert')).textContent).toContain('Sem permissão')
    expect(getState().applications?.map((a) => a.name)).toEqual(['Checkout'])
    await view.findByLabelText('Nome')
    expect(nameOf(view)).toBe('Nova')
    fireEvent.click(view.getByRole('button', { name: 'Remover Checkout' }))
    fireEvent.click(await view.findByRole('button', { name: 'Remover' }))
    expect(await view.findByText('Não deu para remover')).toBeTruthy()
    expect(getState().applications?.map((a) => a.name)).toEqual(['Checkout'])
  })

  test('os tipos de repositório são os que o servidor serve; sem eles vale a lista padrão', async () => {
    await started(withConnections((found) => ({ ...found, repositoryKinds: [{ key: 'service', label: 'Serviço' }, { key: 'job', label: 'Rotina' }] })))
    const view = listed('/settings/applications?edit=new')
    const kind = await view.findByRole('combobox', { name: 'Tipo' })
    expect(kind.textContent).toContain('Serviço')
    fireEvent.keyDown(kind, { key: 'Enter' })
    expect(await view.findByRole('option', { name: /Rotina/ })).toBeTruthy()
    expect(view.queryByRole('option', { name: /Biblioteca/ })).toBeNull()
    cleanup()
    stop()
    await started(withConnections(({ repositoryKinds: _, ...found }) => found))
    const plain = listed('/settings/applications?edit=new')
    const fallback = await plain.findByRole('combobox', { name: 'Tipo' })
    expect(fallback.textContent).toContain('Aplicação')
    fireEvent.keyDown(fallback, { key: 'Enter' })
    expect(await plain.findByRole('option', { name: /Biblioteca/ })).toBeTruthy()
  })

  test('os vínculos se agrupam nas seções que o conector declara, na ordem pedida', async () => {
    const provider = { key: 'p', name: 'P', group: 'code', description: '', logo: '', signIn: 'token' as const, fields: [], multiple: true, bindings: [
      { capability: 'x', scope: 'application' as const, label: 'X', section: 'Entrega', order: 2 },
      { capability: 'y', scope: 'application' as const, label: 'Y', section: 'Entrega', order: 1 },
      { capability: 'z', scope: 'application' as const, label: 'Z' },
    ] }
    const connection = { id: 'c-p', provider: 'p', label: 'P', account: '', capabilities: ['x', 'y', 'z'], status: 'ok' as const }
    const groups = groupedBySection(fieldsOf([connection], [provider], 'application'))
    expect(groups.map((g) => [g.section, g.fields.map((f) => f.label)])).toEqual([[null, ['Z']], ['Entrega', ['Y', 'X']]])
    const placed: Record<string, { section: string; order: number }> = { jira: { section: 'Planejamento', order: 1 }, github: { section: 'Código', order: 1 }, veracode: { section: 'Qualidade', order: 2 }, sonar: { section: 'Qualidade', order: 1 } }
    await started(withConnections((found) => ({ ...found, providers: found.providers.map((p) => ({ ...p, bindings: p.bindings?.map(({ section: _, order: __, ...b }) => ({ ...b, ...(placed[p.key] ?? {}) })) })) })))
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expect(within(view.getByTestId('section-general')).queryByRole('combobox', { name: 'Jira' })).toBeNull()
    expect(within(view.getByTestId('section-declared:Planejamento')).getByRole('combobox', { name: 'Jira' })).toBeTruthy()
    expandRepositories(view)
    const repository = view.getByTestId('repository-0')
    expect(within(within(repository).getByTestId('repository-section-Código')).getByRole('combobox', { name: 'GitHub' })).toBeTruthy()
    const quality = within(within(repository).getByTestId('repository-section-Qualidade')).getAllByRole('combobox')
    expect(quality.map((el) => el.getAttribute('aria-label'))).toEqual(['SonarQube', 'Veracode'])
  })

  test('as opções só são buscadas ao abrir o select, e reabrir usa o que já veio', async () => {
    const base = cloudy()
    const asked: string[] = []
    await started({ ...base, call: ((connection, action, args) => (asked.push(`${action} ${(args as { field?: string }).field ?? ''}`), base.call(connection, action, args))) as typeof base.call })
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expandRepositories(view)
    openSection(view, /^Nuvem/)
    const cloud = await view.findByTestId('binding-cloud')
    fireEvent.click(within(cloud).getByRole('button', { name: 'Adicionar Ambiente' }))
    await within(cloud).findByRole('combobox', { name: 'Ambiente' })
    expect(asked.filter((a) => a.includes('.options'))).toEqual([])
    const open = async () => {
      fireEvent.keyDown(within(cloud).getByRole('combobox', { name: 'Ambiente' }), { key: 'Enter' })
      const option = await view.findByRole('option', { name: 'Produção' })
      fireEvent.keyDown(option, { key: 'Escape' })
      await waitFor(() => expect(view.queryByRole('option', { name: 'Produção' })).toBeNull())
    }
    await open()
    await open()
    expect(asked.filter((a) => a === 'cloud.options environment').length).toBe(1)
  })

  test('sem aplicação há um único convite para criar a primeira', async () => {
    const base = fresh()
    await started({ ...base, applications: async () => [] })
    const view = listed()
    expect(view.getByText('Nenhuma aplicação ainda')).toBeTruthy()
    expect(view.getAllByRole('button', { name: 'Nova aplicação' }).length).toBe(1)
    fireEvent.click(view.getByRole('button', { name: 'Nova aplicação' }))
    await view.findByLabelText('Nome')
    expect(view.queryByText('Nenhuma aplicação ainda')).toBeNull()
  })

  test('enquanto as aplicações carregam aparecem cartões de espera', () => {
    const base = fresh()
    const waiting: Api = { ...base, applications: () => new Promise<Application[]>(() => undefined) }
    setApi(waiting)
    void boot(waiting)
    const view = listed()
    expect(view.getByRole('status', { name: 'Carregando aplicações' })).toBeTruthy()
  })

  test('os comandos do repositório não são campos do formulário e seguem intactos ao salvar', async () => {
    const base = fresh()
    const sent: ApplicationDraft[] = []
    await started({ ...base, saveApplication: (draft) => (sent.push(draft), base.saveApplication(draft)) })
    const view = listed()
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const name = await view.findByLabelText('Nome')
    expandRepositories(view)
    const first = within(view.getByTestId('repository-0'))
    expect(first.queryByLabelText('Rodar 1')).toBeNull()
    expect(first.queryByLabelText('Pasta dos testes E2E')).toBeNull()
    fireEvent.change(name, { target: { value: 'Checkout 2' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(sent.length).toBe(1))
    expect(sent[0].repositories[0]).toMatchObject({ run: ['bun dev'], verify: ['bun test', 'bun run lint'], e2e: ['npx cypress run'], e2eDirectory: 'e2e' })
  })
})

describe('aplicações no composer', () => {
  test('adicionar aplicação leva para as configurações com a nova aplicação já aberta', async () => {
    await started(fresh())
    const Where = () => {
      const location = useLocation()
      return <output data-testid="where">{location.pathname + location.search}</output>
    }
    const view = render(<MemoryRouter><TooltipProvider><NewItem onCreated={() => undefined} /><Where /></TooltipProvider></MemoryRouter>)
    fireEvent.keyDown(await view.findByRole('combobox', { name: 'Aplicação: Checkout' }), { key: 'Enter' })
    fireEvent.click(await view.findByRole('option', { name: 'Adicionar aplicação' }))
    await waitFor(() => expect(view.getByTestId('where').textContent).toBe('/settings/applications?edit=new'))
    expect(view.queryByLabelText('Nome')).toBeNull()
  })

  test('sem espaço no composer; sempre uma aplicação, e o espaço dela filtra a busca e o item', async () => {
    const base = fresh()
    const created: NewItemInput[] = []
    const searched: unknown[] = []
    const spy = {
      ...base,
      createItem: (input: NewItemInput) => (created.push(input), base.createItem(input)),
      call: ((connection, action, args) => (action === 'tracker.search' && searched.push(args), base.call(connection, action, args))) as typeof base.call,
    }
    await started(spy)
    await act(async () => {
      await base.createApplication({ name: 'Infra', bindings: { tracker: { connection: 'c-jira', value: 'OPS' } }, repositories: [{ id: '', directory: `${MOCK_HOME}/projetos/infra`, run: [], verify: [] }] })
      await boot(spy)
    })
    const view = render(<MemoryRouter><TooltipProvider><NewItem onCreated={() => undefined} /></TooltipProvider></MemoryRouter>)
    const openIssues = async () => {
      fireEvent.keyDown(await view.findByRole('button', { name: 'Adicionar' }), { key: 'Enter' })
      fireEvent.click(await view.findByRole('menuitem', { name: /Importar issue do/ }))
    }
    const menu = await view.findByRole('combobox', { name: 'Aplicação: Checkout' })
    await openIssues()
    await view.findByRole('dialog', { name: /Importar issue do/ })
    await waitFor(() => expect(searched.length).toBeGreaterThan(0))
    expect(searched.every((args) => (args as { project?: string }).project === 'APP')).toBe(true)
    expect(view.queryByRole('button', { name: /Projeto:/ })).toBeNull()
    fireEvent.keyDown(view.getByPlaceholderText(/Buscar issue do/), { key: 'Escape' })

    fireEvent.keyDown(menu, { key: 'Enter' })
    expect(view.queryByRole('option', { name: /Todas/ })).toBeNull()
    expect(await view.findByRole('option', { name: 'Adicionar aplicação' })).toBeTruthy()
    fireEvent.click(await view.findByRole('option', { name: 'Infra' }))
    await waitFor(() => expect(view.getByRole('combobox', { name: 'Aplicação: Infra' })).toBeTruthy())
    const infra = getState().applications?.find((a) => a.name === 'Infra')
    expect(localStorage.getItem('composer.application')).toBe(infra?.id ?? '')
    searched.length = 0
    await openIssues()
    await waitFor(() => expect(searched.length).toBeGreaterThan(0))
    expect(searched.every((args) => (args as { project?: string }).project === 'OPS')).toBe(true)
    fireEvent.click(view.getByRole('button', { name: 'Fechar' }))
    const box = view.getByLabelText('Mensagem')
    fireEvent.change(box, { target: { value: 'Subir a fila' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    await waitFor(() => expect(created.length).toBe(1))
    expect(created[0].application).toBe(infra?.id ?? '')
    expect(Object.keys(created[0])).not.toContain('project')
  })
})
