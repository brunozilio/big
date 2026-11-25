import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, DirListing, NewItem as NewItemInput } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { MOCK_HOME } from '../src/api/mock/applications'
import { boot, getState, stop } from '../src/api/store'
import { NewItem } from '../src/screens/items/NewItem'
import { ApplicationsView, cloudChoice, cloudKey, cloudOptions, suggestedCode } from '../src/screens/settings/Applications'
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

  test('o repositório é sugerido pelo remote do git', () => {
    const repos = { 'c-github': [{ id: '1', ref: 'acme/web', name: 'web', url: null, defaultBranch: 'main', private: true }] }
    expect(suggestedCode('ACME/web', repos)).toEqual({ connection: 'c-github', repo: 'acme/web' })
    expect(suggestedCode('acme/outro', repos)).toBeNull()
    expect(suggestedCode(null, repos)).toBeNull()
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
    expect((await view.findByRole('combobox', { name: 'Repositório' }, { timeout: 3000 })).textContent).toContain('acme/web')
    fireEvent.change(view.getByLabelText('Verificação 1'), { target: { value: 'bun test' } })
    fireEvent.click(view.getAllByRole('button', { name: 'Adicionar comando' })[1])
    fireEvent.change(await view.findByLabelText('Verificação 2'), { target: { value: ' bun run lint ' } })
    fireEvent.click(view.getAllByRole('button', { name: 'Adicionar comando' })[0])
    fireEvent.change(view.getByLabelText('Rodar 1'), { target: { value: 'bun api' } })
    fireEvent.change(await view.findByLabelText('Rodar 2'), { target: { value: 'bun web' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.length).toBe(2))
    const made = getState().applications?.[1]
    expect(made?.repositories[0]).toMatchObject({ directory: `${MOCK_HOME}/projetos/web`, code: { connection: 'c-github', repo: 'acme/web' }, run: ['bun api', 'bun web'], verify: ['bun test', 'bun run lint'] })
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

  test('cada papel de cada conta vira uma opção, e a conta sem papel vira uma só', () => {
    const accounts = [{ id: '111', name: 'prod', email: '', roles: ['Admin', 'Deploy'] }, { id: '222', name: '', email: '', roles: [] }]
    expect(cloudOptions(accounts)).toEqual([
      { key: '111/Admin', label: 'prod · Admin' },
      { key: '111/Deploy', label: 'prod · Deploy' },
      { key: '222', label: '222' },
    ])
    expect(cloudChoice('c-aws::111/Deploy')).toEqual({ connection: 'c-aws', account: '111', role: 'Deploy' })
    expect(cloudChoice('c-aws::222')).toEqual({ connection: 'c-aws', account: '222', role: '' })
    expect(cloudChoice('none')).toBeNull()
    expect(cloudKey({ connection: 'c-aws', account: '111', role: 'Deploy' })).toBe('c-aws::111/Deploy')
  })

  test('sem conexão de nuvem o campo não aparece', async () => {
    await started()
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    await view.findByLabelText('Nome')
    expect(view.queryByRole('combobox', { name: 'Conta e papel na nuvem' })).toBeNull()
  })

  test('lista as contas e papéis da conexão e guarda a escolha na aplicação', async () => {
    const base = withCloud()
    const asked: string[] = []
    const spy = { ...base, call: ((connection, action, args) => (asked.push(`${connection} ${action}`), base.call(connection, action, args))) as typeof base.call }
    await started(spy)
    const view = render(<TooltipProvider><ApplicationsView /></TooltipProvider>)
    fireEvent.click(view.getByRole('button', { name: 'Editar Checkout' }))
    const field = await view.findByRole('combobox', { name: 'Conta e papel na nuvem' })
    expect(field.textContent).toContain('Padrão da conexão')
    await waitFor(() => expect(asked).toContain('c-aws cloud.accounts_and_roles'))
    fireEvent.keyDown(field, { key: 'Enter' })
    const option = await view.findByRole('option', { name: 'acme-prod · Deploy' })
    expect(view.getByRole('option', { name: 'acme-dev · Developer' })).toBeTruthy()
    fireEvent.keyDown(option, { key: 'Enter' })
    await waitFor(() => expect(field.textContent).toContain('acme-prod · Deploy'))
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.cloud).toEqual({ connection: 'c-aws', account: '111122223333', role: 'Deploy' }))
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
      await base.createApplication({ name: 'Infra', tracker: { connection: 'c-jira', project: 'OPS' }, repositories: [{ id: '', code: null, directory: `${MOCK_HOME}/projetos/infra`, run: [], verify: [] }] })
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
