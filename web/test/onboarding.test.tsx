import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { boot, getState, stop } from '../src/api/store'
import { needsOnboarding, Onboarding, returnedTo, stageOf } from '../src/screens/onboarding/Onboarding'
import { TooltipProvider } from '../src/ui/overlay'

async function started() {
  const base = mockApi({ tickMs: 0, delayMs: 0 })
  const api: Api = { ...base, applications: async () => [] }
  setApi(api)
  await act(async () => {
    await boot(api)
  })
}

beforeEach(() => localStorage.clear())
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
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Fluxo de branch' }).textContent).toContain('gitflow'))
    fireEvent.click(screen.getByText('Pular por agora'))
    await waitFor(() => expect(done).toBe(1))
    expect(getState().onboarding).toEqual({ skipped: true })
    expect(localStorage.getItem('onboarding:skipped')).toBeNull()
  })
})
