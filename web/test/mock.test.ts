import { describe, expect, test } from 'bun:test'
import type { Block, LiveEvent } from '../src/api/contract'
import { isBlock, isItem, isLiveEvent, isSession, isStepView } from '../src/api/guards'
import { mockApi } from '../src/api/mock'
import { store, tick } from '../src/api/mock/sim'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })
const byKey = (key: string) => {
  const it = store.items.find((x) => x.key === key)
  if (!it) throw new Error(key)
  return String(it.id)
}

describe('adaptador mock', () => {
  test('itens seguem o contrato e caem nos três grupos', async () => {
    const api = fresh()
    const items = await api.items()
    expect(items.length).toBeGreaterThan(5)
    expect(items.every(isItem)).toBe(true)
    expect(new Set(items.map((i) => i.group))).toEqual(new Set(['you', 'running', 'delivered']))
  })

  test('toda view de etapa é feita de blocos válidos', async () => {
    const api = fresh()
    const items = await api.items()
    for (const it of items) {
      for (const s of it.steps) {
        const view = await api.step(it.id, s.id, null)
        expect(isStepView(view)).toBe(true)
        view.blocks.forEach((b: Block) => expect(isBlock(b)).toBe(true))
      }
    }
  })

  test('plano pronto espera aprovação no chat e segue depois de aprovado', async () => {
    const api = fresh()
    const id = byKey('APP-253')
    const before = (await api.items()).find((i) => i.id === id)
    expect(before?.group).toBe('you')
    const view = await api.step(id, 'plan', null)
    expect(view.blocks[0]).toMatchObject({ type: 'chat', composer: true })
    const session = await api.session(`s-${id}`)
    expect(isSession(session)).toBe(true)
    const plan = session.events.find((e) => e.type === 'plan')
    expect(plan).toMatchObject({ resolution: 'pending' })
    await api.respond(session.id, plan?.id ?? '', { kind: 'plan', decision: 'approve' })
    const after = (await api.items()).find((i) => i.id === id)
    expect(after?.steps.find((s) => s.id === 'plan')?.state).toBe('passed')
    expect((await api.step(id, 'plan', null)).blocks[0]).toMatchObject({ type: 'chat', composer: false })
  })

  test('pergunta do agente pausa o develop e a resposta retoma', async () => {
    const api = fresh()
    const id = byKey('APP-240')
    tick()
    const session = await api.session(`s-${id}`)
    const ask = session.events.find((e) => e.type === 'ask')
    expect(ask).toMatchObject({ answers: null })
    expect((await api.items()).find((i) => i.id === id)?.group).toBe('you')
    await api.respond(session.id, ask?.id ?? '', { kind: 'ask', answers: ['3 tentativas'] })
    const answered = (await api.session(`s-${id}`)).events.find((e) => e.type === 'ask')
    expect(answered).toMatchObject({ answers: ['3 tentativas'] })
    expect((await api.items()).find((i) => i.id === id)?.group).toBe('running')
  })

  test('permissão sinalizada pelo revisor aparece no modo auto', async () => {
    const api = fresh()
    const id = byKey('APP-238')
    tick()
    const session = await api.session(`s-${id}`)
    expect(session.mode).toBe('auto')
    expect(session.events.find((e) => e.type === 'permission')).toMatchObject({ resolution: 'pending' })
  })

  test('review aprovado leva ao merge e o merge avança', async () => {
    const api = fresh()
    const id = byKey('APP-228')
    await api.input(id, 'review', { action: 'review', payload: { verdict: 'approve', body: '', notes: [] } })
    let it = (await api.items()).find((i) => i.id === id)
    expect(it?.current).toBe('merge')
    expect(it?.attention?.action?.step).toBe('merge')
    await api.input(id, 'merge', { action: 'merge', payload: { method: 'squash' } })
    it = (await api.items()).find((i) => i.id === id)
    expect(it?.steps.find((s) => s.id === 'merge')?.state).toBe('passed')
  })

  test('chamadas de conexão devolvem issue, mudança e execução', async () => {
    const api = fresh()
    const id = byKey('APP-228')
    const item = (await api.items()).find((i) => i.id === id)
    const issue = await api.call('c-jira', 'tracker.issue', { key: 'APP-228' })
    expect(issue.subtasks.length).toBe(3)
    const change = await api.call('c-github', 'code.change', { change: item?.change?.key ?? '' })
    expect(change.files.length).toBeGreaterThan(0)
    const run = await api.call('c-github', 'ci.run', { run: `ci:${id}:1` })
    expect(run.jobs.length).toBeGreaterThan(0)
    expect((await api.call('c-jira', 'tracker.search', { query: '' })).length).toBeGreaterThan(0)
  })

  test('um servidor MCP pede login só quando não é local e recusa endereço inseguro', async () => {
    const api = fresh()
    expect((await api.startSignIn('mcp', { url: 'https://mcp.linear.app/mcp', label: 'Linear' })).kind).toBe('redirect')
    await expect(api.startSignIn('mcp', { url: 'http://docs.acme/mcp', label: 'Docs' })).rejects.toThrow('https')
    const start = await api.startSignIn('mcp', { url: 'http://localhost:9/mcp', label: '' })
    expect(start.kind).toBe('token')
    let poll = await api.pollSignIn({ flow: start.flow })
    while (poll.state === 'pending') poll = await api.pollSignIn({ flow: start.flow })
    expect(poll.state === 'done' && poll.connection.label).toBe('localhost')
    expect(poll.state === 'done' && poll.connection.capabilities).toEqual(['mcp'])
    expect((await api.call('c-mcp', 'mcp.tools', {})).map((t) => t.name)).toEqual(['search', 'fetch'])
  })

  test('conectar a nuvem destrava o item parado e avisa pelo feed', async () => {
    const api = fresh()
    const events: LiveEvent[] = []
    api.live((e) => events.push(e), () => undefined)
    const id = byKey('APP-231')
    expect((await api.items()).find((i) => i.id === id)?.impediment?.connect?.provider).toBe('aws')
    expect((await api.startSignIn('aws', {})).kind).toBe('redirect')
    const start = await api.startSignIn('aws', { start_url: 'https://x', region: 'us-east-1' }, 'sso')
    expect(start.kind).toBe('device')
    let poll = await api.pollSignIn({ flow: start.flow })
    while (poll.state === 'pending') poll = await api.pollSignIn({ flow: start.flow })
    expect(poll.state).toBe('choose')
    poll = await api.pollSignIn({ flow: start.flow, choice: 'prod' })
    expect(poll.state).toBe('done')
    expect((await api.items()).find((i) => i.id === id)?.impediment).toBeNull()
    expect(events.some((e) => e.type === 'connections')).toBe(true)
    expect(events.every(isLiveEvent)).toBe(true)
  })

  test('conexões declaram a seção dos vínculos e os tipos de repositório', async () => {
    const listed = await fresh().connections()
    const specs = listed.providers.flatMap((p) => p.bindings ?? [])
    expect(specs.length).toBeGreaterThan(0)
    expect(specs.every((s) => typeof s.section === 'string' && s.section !== '' && typeof s.order === 'number')).toBe(true)
    const quality = specs.filter((s) => ['quality', 'security', 'tests'].includes(s.capability)).map((s) => s.section)
    expect(new Set(quality).size).toBe(1)
    expect(listed.repositoryKinds?.map((k) => k.key)).toEqual(['application', 'library', 'configuration', 'infrastructure', 'tests'])
    expect(listed.repositoryKinds?.every((k) => k.label !== '')).toBe(true)
    expect(isLiveEvent({ type: 'connections', connections: listed })).toBe(true)
    expect(isLiveEvent({ type: 'connections', connections: { ...listed, repositoryKinds: [{ key: 'x' }] } })).toBe(false)
  })

  test('criar item a partir de issue pula a etapa do tracker', async () => {
    const api = fresh()
    const it = await api.createItem({ text: '', issue: { connection: 'c-jira', key: 'APP-251' }, model: 'claude-opus', effort: 'medium', fast: false, attachments: [] })
    expect(it.steps.find((s) => s.id === 'ticket')?.state).toBe('skipped')
  })

  test('modelos trazem esforços e somem sem provedor de IA', async () => {
    const api = fresh()
    const listed = await api.models()
    expect(listed.models[0]).toMatchObject({ id: 'claude-opus', fast: false })
    expect(listed.models.every((m) => Array.isArray(m.efforts))).toBe(true)
    await api.disconnect('c-claude')
    await api.disconnect('c-copilot')
    expect(await api.models()).toEqual({ models: [], default: null })
  })

  test('branding guarda o tema e o logo vem do servidor', async () => {
    const api = fresh()
    const saved = await api.saveBranding({ theme: 'dark' })
    expect(saved.theme).toBe('dark')
    expect(saved.logo).toBe('/api/branding/logo')
    expect(saved.language).toBe('pt-BR')
    expect((await api.saveBranding({ language: 'es' })).language).toBe('es')
  })
})
