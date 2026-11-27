import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { boot, getState, stop } from '../src/api/store'
import { needsOnboarding, Onboarding, returnedTo, stageOf } from '../src/screens/onboarding/Onboarding'
import { INSTALLED_KEY } from '../src/screens/onboarding/Shell'
import { TooltipProvider } from '../src/ui/overlay'

async function started() {
  const base = mockApi({ tickMs: 0, delayMs: 0 })
  const api: Api = { ...base, applications: async () => [] }
  setApi(api)
  await act(async () => {
    await boot(api)
  })
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
})
afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

describe('primeira vez', () => {
  test('sem aplicação o app abre o onboarding, e pular vale até existir uma', () => {
    expect(needsOnboarding(true, [], null)).toBe(true)
    expect(needsOnboarding(true, [], { skipped: false })).toBe(true)
    expect(needsOnboarding(false, [], null)).toBe(false)
    expect(needsOnboarding(true, null, null)).toBe(false)
    expect(needsOnboarding(true, [{ id: 'a', name: 'A', repositories: [] }], null)).toBe(false)
    expect(needsOnboarding(true, [], { skipped: true })).toBe(false)
  })

  test('a volta do login cai no onboarding, no passo do provedor', () => {
    expect(returnedTo('/settings/connections', '?connect=jira&flow=f1')).toEqual({ provider: 'jira', flow: 'f1' })
    expect(returnedTo('/settings/connections', '')).toBeNull()
    expect(returnedTo('/', '?connect=jira')).toBeNull()
    const providers = [{ key: 'jira' }, { key: 'claude-subscription', ai: true }]
    expect(stageOf('jira', providers)).toBe('connections')
    expect(stageOf('claude-subscription', providers)).toBe('ai')
  })

  test('sem a marca do instalador as etapas são as de sempre', async () => {
    await started()
    render(<TooltipProvider><Onboarding onDone={() => undefined} /></TooltipProvider>)
    const steps = Array.from(document.querySelectorAll<HTMLElement>('[data-step]')).map((el) => [el.textContent, el.dataset.step])
    expect(steps).toEqual([['1Conexões', 'current'], ['2IA', 'next'], ['3Aplicação', 'next']])
  })

  test('depois da instalação o servidor e o administrador aparecem concluídos', async () => {
    sessionStorage.setItem(INSTALLED_KEY, '1')
    await started()
    render(<TooltipProvider><Onboarding onDone={() => undefined} /></TooltipProvider>)
    const steps = Array.from(document.querySelectorAll<HTMLElement>('[data-step]')).map((el) => [el.textContent, el.dataset.step])
    expect(steps).toEqual([['Servidor', 'done'], ['Administrador', 'done'], ['3Conexões', 'current'], ['4IA', 'next'], ['5Aplicação', 'next']])
    fireEvent.click(screen.getByText('Continuar'))
    await screen.findByText('Conecte um provedor de IA')
    expect(document.querySelector('[data-step="current"]')?.textContent).toBe('4IA')
  })

  test('primeiro as conexões, depois a aplicação, que só nasce com nome e pasta', async () => {
    await started()
    let done = 0
    render(<TooltipProvider><Onboarding onDone={() => (done += 1)} /></TooltipProvider>)
    expect(screen.getByText('Conecte suas ferramentas')).toBeTruthy()
    const shown = () => Array.from(document.querySelectorAll<HTMLElement>('[data-provider]')).map((el) => el.dataset.provider)
    expect(shown()).toEqual(['github', 'jira', 'aws', 'zephyr', 'veracode', 'sonar', 'changes'])
    fireEvent.click(screen.getByText('Continuar'))
    expect(await screen.findByText('Conecte um provedor de IA')).toBeTruthy()
    expect(shown().length).toBeGreaterThan(0)
    expect(shown().every((key) => !['github', 'jira', 'aws'].includes(key ?? ''))).toBe(true)
    fireEvent.click(screen.getByText('Continuar'))
    expect(await screen.findByText('Crie sua aplicação')).toBeTruthy()
    fireEvent.change(screen.getByLabelText('Nome'), { target: { value: 'Loja' } })
    expect((screen.getByLabelText('Nome') as HTMLInputElement).value).toBe('Loja')
    expect((screen.getByText('Criar aplicação').closest('button') as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(screen.getByText('Pular por agora'))
    await waitFor(() => expect(done).toBe(1))
    expect(getState().onboarding).toEqual({ skipped: true })
    expect(localStorage.getItem('onboarding:skipped')).toBeNull()
  })

  test('a aplicação do onboarding nasce no mesmo editor das configurações, com a pasta escolhida no lugar', async () => {
    await started()
    let done = 0
    render(<TooltipProvider><Onboarding onDone={() => (done += 1)} /></TooltipProvider>)
    fireEvent.click(screen.getByText('Continuar'))
    await screen.findByText('Conecte um provedor de IA')
    fireEvent.click(screen.getByText('Continuar'))
    await screen.findByText('Crie sua aplicação')
    expect(screen.getByTestId('application-editor')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Cancelar' })).toBeNull()
    const create = () => screen.getByText('Criar aplicação').closest('button') as HTMLButtonElement
    expect(create().disabled).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'Diretório local' }))
    const list = await screen.findByRole('list', { name: 'Pastas' })
    fireEvent.click(await within(list).findByText('projetos'))
    fireEvent.click(await within(list).findByText('web'))
    await waitFor(() => expect(within(list).getByText('src')).toBeTruthy())
    fireEvent.click(screen.getByRole('button', { name: 'Escolher' }))
    await waitFor(() => expect((screen.getByLabelText('Nome') as HTMLInputElement).value).toBe('web'))
    expect(create().disabled).toBe(false)
    fireEvent.click(create())
    await waitFor(() => expect(done).toBe(1))
    expect(getState().applications?.map((a) => a.name)).toEqual(['web'])
    expect(getState().applications?.[0]?.repositories[0]?.directory.endsWith('/projetos/web')).toBe(true)
  })
})

describe('portão do onboarding', () => {
  test('enquanto não sabe se há aplicações, não decide pela caixa de entrada', async () => {
    const { deciding } = await import('../src/app/gate')
    expect(deciding(false, null)).toBe(true)
    expect(deciding(true, null)).toBe(true)
    expect(deciding(true, [])).toBe(false)
  })
})
