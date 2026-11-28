import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import { ROUTES, type Api, type Usage } from '../src/api/contract'
import { httpApi } from '../src/api/http'
import { mockApi } from '../src/api/mock'
import { usageFixture } from '../src/api/mock/fixtures'
import { boot, stop } from '../src/api/store'
import { UsageView } from '../src/screens/settings/Usage'
import { limitTone } from '../src/screens/settings/UsageLimits'
import { seriesOf } from '../src/screens/settings/usageCards'
import { TooltipProvider } from '../src/ui/overlay'

async function started(api: Api) {
  setApi(api)
  await act(async () => {
    await boot(api)
  })
  return render(
    <MemoryRouter>
      <TooltipProvider>
        <UsageView />
      </TooltipProvider>
    </MemoryRouter>,
  )
}

beforeEach(() => localStorage.clear())
afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

describe('tela de uso', () => {
  test('mostra limites por conta, totais e rankings do mock', async () => {
    const view = await started(mockApi({ tickMs: 0, delayMs: 0 }))
    await waitFor(() => expect(view.getAllByTestId('limit-card').length).toBe(7))
    expect(view.getByText('Este provedor não informa limites')).toBeTruthy()
    expect(view.getByText('A sessão expirou. Entre de novo em Conexões.')).toBeTruthy()
    expect(view.getAllByRole('meter', { name: 'Sessão (5 h)' }).map((m) => m.getAttribute('aria-valuenow'))).toEqual(['42', '97', '18'])
    expect(view.getAllByText(/reinicia em /).length).toBeGreaterThan(0)
    await waitFor(() => expect(view.getByText('Custo equivalente')).toBeTruthy())
    expect(view.getByText('Por assinatura')).toBeTruthy()
    expect(view.getByText('Por conta')).toBeTruthy()
    expect(within(view.getByText('Por modelo').closest('div') as HTMLElement).getByText('claude-opus-4-5')).toBeTruthy()
  })

  test('o custo com chamadas sem preço mostra o parcial e quantas ficaram de fora', async () => {
    const base = mockApi({ tickMs: 0, delayMs: 0 })
    const partial = async (d: Parameters<Api['usage']>[0]) => {
      const found = await base.usage(d)
      const [first, ...rest] = found.byModel
      return { ...found, totals: { ...found.totals, unpricedCalls: 2 }, byModel: first ? [{ ...first, unpricedCalls: 2 }, ...rest] : rest }
    }
    const view = await started({ ...base, usage: partial })
    await waitFor(() => expect(view.getByText('+ 2 sem preço')).toBeTruthy())
    expect(view.getAllByText(/ \+ 2 sem preço$/).length).toBeGreaterThan(0)
  })

  test('troca a janela e atualiza os limites', async () => {
    const seen: number[] = []
    let limits = 0
    const base = mockApi({ tickMs: 0, delayMs: 0 })
    const view = await started({ ...base, usage: (d) => (seen.push(d), base.usage(d)), usageLimits: () => (limits++, base.usageLimits()) })
    await waitFor(() => expect(view.getAllByTestId('limit-card').length).toBe(7))
    fireEvent.click(view.getByText('90 dias'))
    await waitFor(() => expect(seen).toEqual([30, 90]))
    fireEvent.click(view.getByRole('button', { name: 'Atualizar limites' }))
    await waitFor(() => expect(limits).toBe(2))
  })

  test('sem assinatura de IA conectada aponta para as conexões', async () => {
    const base = mockApi({ tickMs: 0, delayMs: 0 })
    const empty: Usage = { ...usageFixture(30, new Date()), totals: { costUsd: 0, input: 0, output: 0, cacheRead: 0, cacheWrite: 0, calls: 0, sessions: 0 }, byProvider: [], byModel: [], byAccount: [] }
    const view = await started({
      ...base,
      connections: async () => {
        const c = await base.connections()
        return { ...c, connections: c.connections.filter((x) => !c.providers.find((p) => p.key === x.provider)?.ai) }
      },
      usage: async () => empty,
      usageLimits: async () => ({ accounts: [] }),
    })
    const link = await view.findByRole('link', { name: 'Ir para conexões' })
    expect(link.getAttribute('href')).toBe('/settings/connections')
    await waitFor(() => expect(view.getByText('Sem uso nos últimos 30 dias')).toBeTruthy())
    expect(view.queryByText('Limites do plano')).toBeNull()
  })

  test('tom da barra, séries agrupadas e rotas', async () => {
    expect([50, 80, 94.9, 95].map(limitTone)).toEqual(['normal', 'warning', 'warning', 'critical'])
    const usage = usageFixture(7, new Date(2026, 10, 23))
    expect(usage.daily.length).toBe(7)
    expect(usage.to).toBe('2026-11-23')
    const rows = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((id) => ({ id, name: id }))
    const { series, data } = seriesOf(rows, [{ date: '2026-11-23', costUsd: 7, tokens: 0, calls: 7, byProvider: {}, byModel: { a: 1, f: 2, g: 4 } }], (d) => d.byModel)
    expect(series.map((s) => s.name)).toEqual(['a', 'b', 'c', 'd', 'e', 'Outros'])
    expect(data[0].rest).toBe(6)
    const calls: string[] = []
    const http = httpApi(async (url) => (calls.push(url), new Response('{}')))
    await http.usage(90)
    await http.usageLimits()
    expect(calls).toEqual(['/api/usage?days=90', ROUTES.usageLimits])
  })
})
