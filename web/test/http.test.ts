import { describe, expect, test } from 'bun:test'
import { ROUTES, type LiveEvent } from '../src/api/contract'
import { followLive, httpApi, type LiveSource } from '../src/api/http'

function recorder(answer: unknown, status = 200) {
  const calls: { url: string; method: string; body: unknown }[] = []
  const fetcher = async (url: string, init?: RequestInit) => {
    calls.push({ url, method: init?.method ?? 'GET', body: init?.body ? JSON.parse(String(init.body)) : undefined })
    return new Response(JSON.stringify(answer), { status })
  }
  return { calls, fetcher }
}

describe('cliente HTTP', () => {
  test('cada ação vai para a rota do contrato', async () => {
    const { calls, fetcher } = recorder({})
    const api = httpApi(fetcher)
    await api.items()
    await api.rerun('7', 'ci')
    await api.step('7', 'ci', 2)
    await api.input('7', 'review', { action: 'review', payload: { verdict: 'approve' } })
    await api.call('c1', 'ci.run', { run: 'r1' })
    await api.respond('s1', 'e1', { kind: 'plan', decision: 'approve' })
    await api.startSignIn('github', {})
    await api.pollSignIn({ flow: 'f1' })
    await api.saveBranding({ theme: 'dark' })
    await api.call('c-cloud', 'cloud.accounts_and_roles', {})
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      `GET ${ROUTES.items}`,
      'POST /api/items/7/rerun',
      'GET /api/items/7/steps/ci?attempt=2',
      'POST /api/items/7/steps/review/input',
      'POST /api/connections/c1/call',
      'POST /api/sessions/s1/respond',
      'POST /api/connections/github/start',
      'POST /api/connections/device/poll',
      'PUT /api/branding',
      'POST /api/connections/c-cloud/call',
    ])
    expect(calls[4].body).toEqual({ action: 'ci.run', args: { run: 'r1' } })
    expect(calls[5].body).toEqual({ event: 'e1', response: { kind: 'plan', decision: 'approve' } })
    expect(calls[9].body).toEqual({ action: 'cloud.accounts_and_roles', args: {} })
  })

  test('pergunta à parte vai para a rota side com texto e modelo', async () => {
    const { calls, fetcher } = recorder({ answer: 'Está rodando os testes.' })
    const answer = await httpApi(fetcher).side('s 1', { text: 'O que falta?', model: null })
    expect(answer).toEqual({ answer: 'Está rodando os testes.' })
    expect(calls).toEqual([{ url: '/api/sessions/s%201/side', method: 'POST', body: { text: 'O que falta?', model: null } }])
    expect(ROUTES.side('s1')).toBe('/api/sessions/s1/side')
  })

  test('erro do servidor vira ApiError com a mensagem dele', async () => {
    const { fetcher } = recorder({ error: { code: 'missing', message: 'Item não existe' } }, 404)
    await expect(httpApi(fetcher).items()).rejects.toMatchObject({ status: 404, code: 'missing', message: 'Item não existe' })
  })

  test('o feed ignora eventos fora do contrato e pede resync ao reconectar', () => {
    const sources: LiveSource[] = []
    const got: LiveEvent[] = []
    const open = () => {
      const s: LiveSource = { onmessage: null, onopen: null, onerror: null, close: () => undefined }
      sources.push(s)
      return s
    }
    const stop = followLive(open, (e) => got.push(e), () => undefined)
    sources[0].onopen?.()
    sources[0].onmessage?.({ data: JSON.stringify({ type: 'item.removed', id: '3' }) })
    sources[0].onmessage?.({ data: JSON.stringify({ type: 'nada' }) })
    sources[0].onmessage?.({ data: 'não é json' })
    expect(got).toEqual([{ type: 'item.removed', id: '3' }])
    sources[0].onopen?.()
    expect(got[1]).toEqual({ type: 'resync' })
    stop()
  })
})
