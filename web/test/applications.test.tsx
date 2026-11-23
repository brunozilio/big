import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, DirListing, NewItem as NewItemInput } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { MOCK_HOME } from '../src/api/mock/applications'
import { boot, getState, stop } from '../src/api/store'
import { NewItem } from '../src/screens/items/NewItem'
import { ApplicationsView, suggestedCode } from '../src/screens/settings/Applications'
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
    fireEvent.change(view.getByLabelText('Verificação'), { target: { value: 'bun test\n\nbun run lint' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.length).toBe(2))
    const made = getState().applications?.[1]
    expect(made?.repositories[0]).toMatchObject({ directory: `${MOCK_HOME}/projetos/web`, code: { connection: 'c-github', repo: 'acme/web' }, verify: ['bun test', 'bun run lint'] })
    await waitFor(() => expect(view.getByRole('button', { name: 'Remover web' })).toBeTruthy())
    fireEvent.click(view.getByRole('button', { name: 'Remover web' }))
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
    const text = await view.findByText('Ignorar pipelines do dependabot[bot]')
    const box = view.getByRole('checkbox', { name: 'Ignorar pipelines do dependabot[bot]' })
    expect(box.getAttribute('aria-checked')).toBe('true')
    fireEvent.click(text)
    await waitFor(() => expect(box.getAttribute('aria-checked')).toBe('false'))
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.[0]?.skipDependabot).toBe(false))
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
      await base.createApplication({ name: 'Infra', tracker: { connection: 'c-jira', project: 'OPS' }, repositories: [{ id: '', code: null, directory: `${MOCK_HOME}/projetos/infra`, run: '', verify: [] }] })
      await boot(spy)
    })
    const view = render(<MemoryRouter><TooltipProvider><NewItem onCreated={() => undefined} /></TooltipProvider></MemoryRouter>)
    const trigger = await view.findByRole('button', { name: /Começar de uma issue/ })
    const menu = await view.findByRole('button', { name: 'Aplicação: Checkout' })
    expect(trigger.textContent).toBe('')
    fireEvent.click(trigger)
    await waitFor(() => expect(searched.length).toBeGreaterThan(0))
    expect(searched.every((args) => (args as { project?: string }).project === 'APP')).toBe(true)
    expect(view.queryByRole('button', { name: /Projeto:/ })).toBeNull()
    fireEvent.keyDown(document, { key: 'Escape' })

    fireEvent.keyDown(menu, { key: 'Enter' })
    expect(view.queryByRole('menuitem', { name: /Todas/ })).toBeNull()
    fireEvent.click(await view.findByRole('menuitem', { name: 'Infra' }))
    await waitFor(() => expect(view.getByRole('button', { name: 'Aplicação: Infra' })).toBeTruthy())
    const infra = getState().applications?.find((a) => a.name === 'Infra')
    expect(localStorage.getItem('composer.application')).toBe(infra?.id ?? '')
    searched.length = 0
    fireEvent.click(view.getByRole('button', { name: /Começar de uma issue/ }))
    await waitFor(() => expect(searched.length).toBeGreaterThan(0))
    expect(searched.every((args) => (args as { project?: string }).project === 'OPS')).toBe(true)
    fireEvent.keyDown(document, { key: 'Escape' })
    const box = view.getByLabelText('Mensagem')
    fireEvent.change(box, { target: { value: 'Subir a fila' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    await waitFor(() => expect(created.length).toBe(1))
    expect(created[0].application).toBe(infra?.id ?? '')
    expect(Object.keys(created[0])).not.toContain('project')
  })
})
