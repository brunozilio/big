import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, DirListing, NewItem as NewItemInput } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { MOCK_HOME } from '../src/api/mock/applications'
import { boot, getState, stop } from '../src/api/store'
import { NewItem } from '../src/screens/items/NewItem'
import { ApplicationsView } from '../src/screens/settings/Applications'
import { fieldsOf, suggested, withBinding, withChoices } from '../src/screens/settings/bindings'
import { FolderDialog } from '../src/screens/settings/FolderDialog'
import { MemoryRouter } from 'react-router-dom'
import { TooltipProvider } from '../src/ui/overlay'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })

async function started(api: Api = fresh()) {
  setApi(api)
  await act(async () => {
    await boot(api)
  })
  return api
}

beforeEach(() => localStorage.clear())
afterEach(() => {
  cleanup()
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

  test('o vínculo do repositório é sugerido pelo remote do git entre as opções do conector', () => {
    const github = { key: 'github', name: 'GitHub', group: 'code', description: '', logo: '', signIn: 'device' as const, fields: [], multiple: true }
    const connection = { id: 'c-github', provider: 'github', label: 'acme', account: '', capabilities: ['code'], status: 'ok' as const }
    const fields = [{ capability: 'code', label: 'Repositório do GitHub', sources: [{ connection, provider: github }] }]
    const options = { code: { 'c-github': [{ value: 'acme/web', label: 'acme/web' }] } }
    expect(suggested('ACME/web', fields, options, {})).toEqual({ code: { connection: 'c-github', value: 'acme/web', label: 'acme/web' } })
    expect(suggested('acme/outro', fields, options, {})).toEqual({})
    const kept = { code: { connection: 'c-github', value: 'acme/api' } }
    expect(suggested('acme/web', fields, options, kept)).toEqual(kept)
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
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
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
    expect((await view.findByRole('combobox', { name: 'Repositório do GitHub' }, { timeout: 3000 })).textContent).toContain('acme/web')
    fireEvent.change(view.getByLabelText('Verificação 1'), { target: { value: 'bun test' } })
    fireEvent.click(view.getAllByRole('button', { name: 'Adicionar comando' })[1])
    fireEvent.change(await view.findByLabelText('Verificação 2'), { target: { value: ' bun run lint ' } })
    fireEvent.click(view.getAllByRole('button', { name: 'Adicionar comando' })[0])
    fireEvent.change(view.getByLabelText('Rodar 1'), { target: { value: 'bun api' } })
    fireEvent.change(await view.findByLabelText('Rodar 2'), { target: { value: 'bun web' } })
    fireEvent.change(view.getByLabelText('Testes E2E 1'), { target: { value: 'npx cypress run' } })
    fireEvent.change(view.getByLabelText('Pasta dos testes E2E'), { target: { value: ' e2e ' } })
    fireEvent.change(view.getByLabelText('Nome 1'), { target: { value: 'CYPRESS_BASE_URL' } })
    fireEvent.change(view.getByLabelText('Valor 1'), { target: { value: 'http://localhost:3000' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.length).toBe(2))
    const made = getState().applications?.[1]
    expect(made?.repositories[0]).toMatchObject({ directory: `${MOCK_HOME}/projetos/web`, bindings: { code: { connection: 'c-github', value: 'acme/web' } }, run: ['bun api', 'bun web'], verify: ['bun test', 'bun run lint'], e2e: ['npx cypress run'], e2eDirectory: 'e2e' })
    expect(made?.env).toEqual({ CYPRESS_BASE_URL: 'http://localhost:3000' })
    await waitFor(() => expect(view.getByRole('button', { name: 'Remover web' })).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Remover web' }))
    expect(getState().applications?.length).toBe(2)
    fireEvent.click(await view.findByRole('button', { name: 'Remover' }))
    await waitFor(() => expect(getState().applications?.length).toBe(1))
  })

  test('não salva sem nome e mostra o erro', async () => {
    await started()
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const name = (await view.findByLabelText('Nome')) as HTMLInputElement
    expect(name.value).toBe('Checkout')
    expect(view.getAllByLabelText('Diretório local').length).toBe(2)
    fireEvent.change(name, { target: { value: ' ' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    expect((await view.findByRole('alert')).textContent).toContain('nome')
  })

  test('cada repositório escolhe o fluxo de branch, automático por padrão', async () => {
    await started()
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    const selects = await view.findAllByRole('combobox', { name: 'Fluxo de branch' })
    expect(selects.length).toBe(2)
    expect(selects[0].textContent).toContain('Automático')
  })

  test('a opção do dependabot aparece escrita, vem ligada e desliga ao clicar no texto', async () => {
    await started()
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const text = await view.findByText('Ignorar dependabot')
    const box = view.getByRole('checkbox', { name: 'Ignorar dependabot' })
    expect(box.getAttribute('aria-checked')).toBe('true')
    fireEvent.click(text)
    await waitFor(() => expect(box.getAttribute('aria-checked')).toBe('false'))
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.skipDependabot).toBe(false))
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
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expect(view.queryByRole('combobox', { name: 'Conta e papel da AWS' })).toBeNull()
  })

  test('lista as contas e papéis da conexão e guarda a escolha na aplicação', async () => {
    const base = withCloud()
    const asked: string[] = []
    const spy = { ...base, call: ((connection, action, args) => (asked.push(`${connection} ${action}`), base.call(connection, action, args))) as typeof base.call }
    await started(spy)
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const field = await view.findByRole('combobox', { name: 'Conta e papel da AWS' })
    expect(field.textContent).toContain('Nenhum')
    await waitFor(() => expect(asked).toContain('c-aws cloud.options'))
    fireEvent.keyDown(field, { key: 'Enter' })
    const option = await view.findByRole('option', { name: 'acme-prod · Deploy' })
    expect(view.getByRole('option', { name: 'acme-dev · Developer' })).toBeTruthy()
    fireEvent.keyDown(option, { key: 'Enter' })
    await waitFor(() => expect(field.textContent).toContain('acme-prod · Deploy'))
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.bindings?.cloud).toEqual({ connection: 'c-aws', value: '111122223333/Deploy', label: 'acme-prod · Deploy' }))
  })
})

describe('aplicações no composer', () => {
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
