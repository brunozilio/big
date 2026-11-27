import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { MOCK_HOME } from '../src/api/mock/applications'
import { boot, getState, stop } from '../src/api/store'
import { ApplicationsView, withFolders } from '../src/screens/settings/Applications'
import { matching, suggested } from '../src/screens/settings/bindings'
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

const blank = { id: '', directory: '', kind: 'application', run: [], verify: [], e2e: [], e2eDirectory: '', bindings: {} }

describe('vários repositórios de uma vez', () => {
  test('as pastas marcadas preenchem o repositório vazio e viram repositórios novos, sem repetir', () => {
    const kept = { ...blank, id: 'r1', directory: '/src/web' }
    const next = withFolders([kept, blank], ['/src/api', '/src/web', '/src/cfg', '/src/api'])
    expect(next.map((r) => r.directory)).toEqual(['/src/web', '/src/api', '/src/cfg'])
    expect(next[0]).toBe(kept)
  })

  test('o diálogo devolve todas as pastas marcadas', async () => {
    setApi(fresh())
    const many: string[][] = []
    const view = render(<TooltipProvider><FolderDialog open start={`${MOCK_HOME}/projetos`} onClose={() => undefined} onChoose={() => undefined} onChooseMany={(paths) => many.push(paths)} /></TooltipProvider>)
    const list = await view.findByRole('list', { name: 'Pastas' })
    await waitFor(() => expect(within(list).getByText('checkout')).toBeTruthy())
    fireEvent.click(within(list).getByRole('checkbox', { name: 'checkout' }))
    fireEvent.click(within(list).getByRole('checkbox', { name: 'web' }))
    fireEvent.click(view.getByRole('button', { name: 'Adicionar (2)' }))
    expect(many).toEqual([[`${MOCK_HOME}/projetos/checkout`, `${MOCK_HOME}/projetos/web`]])
  })

  test('adicionar repositório abre a pasta, cria um por pasta e sugere código, segurança e qualidade pelo nome', async () => {
    await started()
    const view = render(<MemoryRouter><TooltipProvider><ApplicationsView /></TooltipProvider></MemoryRouter>)
    fireEvent.click(view.getByRole('button', { name: 'Nova aplicação' }))
    fireEvent.click(await view.findByRole('button', { name: 'Adicionar repositório' }))
    const list = await view.findByRole('list', { name: 'Pastas' })
    fireEvent.click(await within(list).findByText('projetos'))
    fireEvent.click(await within(list).findByRole('checkbox', { name: 'checkout' }))
    fireEvent.click(within(list).getByRole('checkbox', { name: 'infra' }))
    fireEvent.click(view.getByRole('button', { name: 'Adicionar (2)' }))
    await waitFor(() => expect(view.getByTestId('repository-1')).toBeTruthy())
    const name = (await view.findByLabelText('Nome')) as HTMLInputElement
    expect(name.value).toBe('checkout')
    const first = within(view.getByTestId('repository-0'))
    await waitFor(() => expect(first.getByRole('combobox', { name: 'Veracode' }).textContent).toContain('Checkout'), { timeout: 3000 })
    await waitFor(() => expect(first.getByRole('combobox', { name: 'SonarQube' }).textContent).toContain('acme_checkout'), { timeout: 3000 })
    fireEvent.click(within(view.getByTestId('repository-1')).getByRole('button', { expanded: false }))
    fireEvent.click(await within(view.getByTestId('repository-1')).findByRole('switch', { name: 'Somente leitura' }))
    expect(within(view.getByTestId('repository-1')).queryByRole('combobox', { name: 'Fluxo de branch' })).toBeNull()
    fireEvent.click(view.getByRole('button', { name: 'Salvar' }))
    await waitFor(() => expect(getState().applications?.length).toBe(2))
    const made = getState().applications?.[1]
    expect(made?.repositories.map((r) => r.directory)).toEqual([`${MOCK_HOME}/projetos/checkout`, `${MOCK_HOME}/projetos/infra`])
    expect(made?.repositories.map((r) => r.readOnly === true)).toEqual([false, true])
    expect(made?.repositories[0].bindings).toMatchObject({ code: { value: 'acme/checkout' }, security: { connection: 'c-veracode' }, quality: { value: 'acme_checkout' } })
  })
})

describe('sugestão pelo nome', () => {
  test('acha a opção pelo nome inteiro ou por um pedaço do valor ou do rótulo', () => {
    const options = [
      { value: '7f00-guid', label: 'Checkout API' },
      { value: 'acme_payments', label: 'acme_payments' },
      { value: 'acme/web', label: 'acme/web' },
      { value: 'x', label: 'Infra · 1234' },
    ]
    expect(matching(options, ['checkout-api'])?.value).toBe('7f00-guid')
    expect(matching(options, ['payments'])?.value).toBe('acme_payments')
    expect(matching(options, ['Web'])?.value).toBe('acme/web')
    expect(matching(options, ['infra'])?.value).toBe('x')
    expect(matching(options, ['pay'])).toBeUndefined()
  })

  test('busca pelo nome curto do repositório quando o remote não acha nada', async () => {
    const veracode = { key: 'veracode', name: 'Veracode', group: 'security', description: '', logo: '', signIn: 'token' as const, fields: [], multiple: true }
    const connection = { id: 'c-v', provider: 'veracode', label: 'Veracode', account: '', capabilities: ['security'], status: 'ok' as const }
    const fields = [{ capability: 'security', label: 'Veracode', sources: [{ connection, provider: veracode }] }]
    const listed = [{ value: 'guid-1', label: 'checkout-api' }, { value: 'guid-2', label: 'checkout-api-legacy' }]
    const asked: string[] = []
    setApi({ ...fresh(), call: (async (_c: string, _a: string, args: { query?: string }) => {
      asked.push(args.query ?? '')
      return listed.filter((o) => o.label.includes(args.query ?? ''))
    }) as Api['call'] })
    expect(await suggested('acme/checkout-api', fields, {})).toEqual({ security: { connection: 'c-v', value: 'guid-1', label: 'checkout-api' } })
    expect(asked).toEqual(['acme/checkout-api', 'checkout-api'])
  })

  test('o conector que declara a própria sugestão recebe os nomes e decide sozinho', async () => {
    const sonar = { key: 'sonar', name: 'SonarQube', group: 'quality', description: '', logo: '', signIn: 'token' as const, fields: [], multiple: true, bindings: [{ capability: 'quality', scope: 'repository' as const, label: 'SonarQube', suggests: true }] }
    const connection = { id: 'c-s', provider: 'sonar', label: 'SonarQube', account: '', capabilities: ['quality'], status: 'ok' as const }
    const fields = [{ capability: 'quality', label: 'SonarQube', sources: [{ connection, provider: sonar }] }]
    const asked: unknown[] = []
    setApi({ ...fresh(), call: (async (_c: string, _a: string, args: unknown) => {
      asked.push(args)
      return [{ value: 'acme_checkout-api', label: 'Checkout API' }]
    }) as Api['call'] })
    expect(await suggested('acme/checkout-api', fields, {}, ['checkout-api'])).toEqual({ quality: { connection: 'c-s', value: 'acme_checkout-api', label: 'Checkout API' } })
    expect(asked).toEqual([{ query: '', names: ['acme/checkout-api', 'checkout-api'] }])
  })
})
