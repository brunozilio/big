import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import type { LiveSource } from '../src/api/http'
import { mockApi } from '../src/api/mock'
import { AUTH } from '../src/api/auth'
import { Login } from '../src/screens/login/Login'
import { Run } from '../src/screens/run/Run'
import { INSTALLED_KEY } from '../src/screens/onboarding/Shell'
import { EMPTY_RUN, localize, parseRunEvent, runPhase, runReducer, RUNNER, type RunInput, type RunState } from '../src/screens/run/runner'

const realFetch = globalThis.fetch

afterEach(() => {
  cleanup()
  globalThis.fetch = realFetch
})

interface Call {
  method: string
  path: string
  body: unknown
}

function serve(routes: Record<string, { status: number; body: unknown }>): Call[] {
  const calls: Call[] = []
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    calls.push({ method, path: String(path), body: init?.body ? JSON.parse(String(init.body)) : undefined })
    const reply = routes[`${method} ${String(path)}`] ?? { status: 200, body: {} }
    return new Response(reply.status === 204 ? null : JSON.stringify(reply.body), { status: reply.status })
  }) as typeof fetch
  return calls
}

function feed(...inputs: RunInput[]): RunState {
  return inputs.reduce(runReducer, EMPTY_RUN)
}

const form = (id: string, title = 'Conta'): RunInput => ({
  type: 'form',
  id,
  title,
  fields: [
    { key: 'region', label: 'Região', kind: 'select', value: 'us-east-1', options: [{ value: 'us-east-1', label: 'Virgínia', hint: 'mais barata' }, { value: 'sa-east-1', label: 'São Paulo' }] },
    { key: 'name', label: 'Nome', kind: 'text', required: true },
    { key: 'secret', label: 'Segredo', kind: 'password' },
    { key: 'account', label: 'Conta', kind: 'info', value: '1234' },
  ],
  actions: [{ id: 'back', label: 'Voltar' }, { id: 'go', label: 'Instalar', primary: true }],
})

describe('eventos do instalador', () => {
  test('um passo com o mesmo id atualiza no lugar', () => {
    const state = feed(
      { type: 'step', id: 'a', title: 'Rede', state: 'running' },
      { type: 'step', id: 'b', title: 'Banco', state: 'waiting' },
      { type: 'step', id: 'a', title: 'Rede', state: 'done' },
    )
    expect(state.steps).toEqual([{ id: 'a', title: 'Rede', state: 'done' }, { id: 'b', title: 'Banco', state: 'waiting' }])
  })

  test('um formulário novo substitui o anterior e libera a resposta', () => {
    const answered = feed(form('f1'), { type: 'answered', form: 'f1' })
    expect(answered.answered).toBe(true)
    const next = runReducer(answered, form('f2', 'Outra'))
    expect(next.form?.id).toBe('f2')
    expect(next.form?.title).toBe('Outra')
    expect(next.answered).toBe(false)
    expect(runReducer(next, { type: 'answered', form: 'f1' }).answered).toBe(false)
  })

  test('o log é ignorado', () => {
    expect(parseRunEvent('{"type":"log","text":"olá"}')).toBeNull()
    expect('logs' in EMPTY_RUN).toBe(false)
  })

  test('textos vêm simples ou por idioma, com o inglês de reserva', () => {
    const titled = { 'pt-BR': 'Rede', en: 'Network', es: 'Red' }
    expect(localize(titled, 'pt-BR')).toBe('Rede')
    expect(localize(titled, 'es')).toBe('Red')
    expect(localize({ en: 'Network' }, 'pt-BR')).toBe('Network')
    expect(localize({ es: 'Red' }, 'pt-BR')).toBe('Red')
    expect(localize('Rede', 'en')).toBe('Rede')
    expect(localize(undefined, 'en')).toBe('')
    expect(parseRunEvent(JSON.stringify({ type: 'step', id: 'a', title: titled, state: 'done' }))).toEqual({ type: 'step', id: 'a', title: titled, state: 'done' })
    const parsed = parseRunEvent(JSON.stringify({ type: 'form', id: 'f', title: { en: 'Account' }, fields: [{ key: 'r', kind: 'select', label: { en: 'Region' }, options: [{ value: 'x', label: { en: 'X', 'pt-BR': 'Xis' } }] }], actions: [{ id: 'go', label: { en: 'Go' }, primary: true }] }))
    expect(parsed).toMatchObject({ title: { en: 'Account' }, fields: [{ label: { en: 'Region' }, options: [{ label: { en: 'X', 'pt-BR': 'Xis' } }] }], actions: [{ label: { en: 'Go' } }] })
  })

  test('finish vence o exit e o exit sem finish com erro falha', () => {
    const finished = feed(form('f1'), { type: 'finish', next: '/login?setup=T', message: 'Pronto' }, { type: 'exit', code: 1 })
    expect(runPhase(finished)).toBe('finished')
    expect(finished.form).toBeNull()
    expect(runPhase(feed({ type: 'exit', code: 2 }))).toBe('failed')
    expect(runPhase(feed({ type: 'exit', code: 0 }))).toBe('ended')
    expect(runPhase(EMPTY_RUN)).toBe('running')
    expect(feed({ type: 'exit', code: 0 }, { type: 'lost' }).lost).toBe(false)
    expect(feed({ type: 'lost' }).lost).toBe(true)
  })

  test('mensagens inválidas são ignoradas', () => {
    expect(parseRunEvent('nada')).toBeNull()
    expect(parseRunEvent('{"type":"step","id":"a","state":"weird"}')).toBeNull()
    expect(parseRunEvent('{"type":"exit","code":3}')).toEqual({ type: 'exit', code: 3 })
  })
})

function fakeSource(): { open: (url: string) => LiveSource; urls: string[]; emit: (event: unknown) => void; fail: () => void } {
  const urls: string[] = []
  let current: LiveSource | null = null
  const open = (url: string) => {
    urls.push(url)
    const source: LiveSource = { onmessage: null, onopen: null, onerror: null, close: () => undefined }
    current = source
    return source
  }
  const emit = (event: unknown) => act(() => current?.onmessage?.({ data: JSON.stringify(event) }))
  const fail = () => act(() => current?.onerror?.())
  return { open, urls, emit, fail }
}

const wait = { timeout: 8000 }

describe('tela do instalador', () => {
  test('responder o formulário envia a ação e os valores e trava até o próximo', async () => {
    const calls = serve({ [`POST ${RUNNER.answer}`]: { status: 204, body: null } })
    const source = fakeSource()
    const view = render(<Run open={source.open} />)
    expect(source.urls).toEqual([RUNNER.events])
    source.emit({ type: 'step', id: 's', title: 'Criar rede', state: 'running' })
    expect(view.getByText('Criar rede')).toBeTruthy()
    expect(view.getByLabelText('Em andamento')).toBeTruthy()
    source.emit({ type: 'log', text: 'olá' })
    source.emit(form('f1'))
    expect(view.queryByText('olá')).toBeNull()
    expect(view.queryByRole('log')).toBeNull()
    expect(view.getByRole('heading', { name: 'Conta' })).toBeTruthy()
    expect(view.getByText('1234')).toBeTruthy()
    const go = view.getByRole('button', { name: 'Instalar' }) as HTMLButtonElement
    expect(go.disabled).toBe(true)
    fireEvent.change(view.getByLabelText('Nome'), { target: { value: 'prod' } })
    fireEvent.change(view.getByLabelText('Segredo'), { target: { value: 's3' } })
    expect(go.disabled).toBe(false)
    fireEvent.click(go)
    expect(await view.findByText('Recebido. Seguindo…', {}, wait)).toBeTruthy()
    expect(calls.find((c) => c.method === 'POST' && c.path === RUNNER.answer)?.body).toEqual({ form: 'f1', action: 'go', values: { region: 'us-east-1', name: 'prod', secret: 's3' } })
    expect((view.getByRole('button', { name: 'Instalar' }) as HTMLButtonElement).disabled).toBe(true)
    source.emit(form('f2', 'Confirmar'))
    expect(view.getByText('Confirmar')).toBeTruthy()
    expect((view.getByRole('button', { name: 'Voltar' }) as HTMLButtonElement).disabled).toBe(false)
  }, 20000)

  test('o andamento mostra cada passo sem log, em português ou no idioma do evento', () => {
    const source = fakeSource()
    const view = render(<Run open={source.open} />)
    expect(view.getByText('Começando…')).toBeTruthy()
    source.emit({ type: 'step', id: 'a', title: { 'pt-BR': 'Rede', en: 'Network' }, state: 'done' })
    source.emit({ type: 'step', id: 'b', title: { en: 'Database' }, state: 'running' })
    source.emit({ type: 'step', id: 'c', title: 'Página', state: 'waiting' })
    source.emit({ type: 'log', text: 'terraform apply' })
    expect(view.getByRole('heading', { name: 'Preparando o servidor' })).toBeTruthy()
    expect(view.getByText('Rede')).toBeTruthy()
    expect(view.getByText('Database')).toBeTruthy()
    expect(view.getByLabelText('Concluída')).toBeTruthy()
    expect(view.getByLabelText('Em andamento')).toBeTruthy()
    expect(view.getByLabelText('Na fila')).toBeTruthy()
    expect(view.queryByText('terraform apply')).toBeNull()
    expect(document.querySelector('[data-step="current"]')?.textContent).toContain('Servidor')
    source.emit({ type: 'step', id: 'b', title: { en: 'Database' }, state: 'failed' })
    expect(view.getByLabelText('Falhou')).toBeTruthy()
  })

  test('a falha mostra a mensagem e pede para rodar de novo', () => {
    const source = fakeSource()
    const view = render(<Run open={source.open} />)
    source.emit({ type: 'error', message: { 'pt-BR': 'Sem permissão na conta', en: 'No permission' } })
    expect(view.getByText('Sem permissão na conta')).toBeTruthy()
    source.emit({ type: 'exit', code: 1 })
    expect(view.getByRole('heading', { name: 'Não deu para terminar' })).toBeTruthy()
    expect(view.getByText('Corrija o que for preciso e rode o comando de novo.')).toBeTruthy()
  })

  test('o fim marca a instalação e segue sozinho para o próximo passo', async () => {
    sessionStorage.clear()
    const went: string[] = []
    const go = (url: string) => went.push(url)
    const source = fakeSource()
    const view = render(<Run open={source.open} go={go} />)
    source.emit({ type: 'finish', next: '/login?setup=T', message: 'Servidor no ar' })
    source.emit({ type: 'exit', code: 0 })
    expect(view.getByText('Servidor no ar')).toBeTruthy()
    expect(sessionStorage.getItem(INSTALLED_KEY)).toBe('1')
    expect(view.queryByText('Conexão com o instalador perdida. Confira o terminal onde ele está rodando.')).toBeNull()
    await waitFor(() => expect(went).toEqual(['/login?setup=T']), wait)
    fireEvent.click(view.getByRole('button', { name: 'Continuar' }))
    expect(went).toEqual(['/login?setup=T', '/login?setup=T'])
    sessionStorage.clear()
  }, 20000)

  test('a conexão perdida aparece', () => {
    const source = fakeSource()
    const view = render(<Run open={source.open} />)
    source.fail()
    expect(view.getByText('A conexão com o instalador caiu. Confira o terminal onde ele está rodando.')).toBeTruthy()
  })
})

describe('primeiro administrador', () => {
  beforeEach(() => setApi(mockApi({ tickMs: 0, delayMs: 0 })))
  afterEach(() => setApi(null))

  test('com o token o login cria o administrador', async () => {
    const calls = serve({
      [`GET ${AUTH.methods}`]: { status: 200, body: { mode: 'shared', methods: ['password'], ssoLabel: '', setup: true } },
      [`POST ${AUTH.setup}`]: { status: 200, body: { user: 'u', email: 'admin', name: '' } },
    })
    const view = render(<MemoryRouter initialEntries={['/login?setup=TOK']}><Login /></MemoryRouter>)
    sessionStorage.clear()
    expect(await view.findByRole('heading', { name: 'Crie o administrador' }, wait)).toBeTruthy()
    expect(document.querySelector('[data-step="current"]')?.textContent).toContain('Administrador')
    expect(document.querySelectorAll('[data-step="done"]').length).toBe(1)
    expect((view.getByLabelText('Usuário') as HTMLInputElement).value).toBe('admin')
    fireEvent.change(view.getByLabelText('Senha'), { target: { value: 'curta' } })
    fireEvent.change(view.getByLabelText('Confirme a senha'), { target: { value: 'curta' } })
    fireEvent.click(view.getByRole('button', { name: 'Criar e entrar' }))
    expect(await view.findByText('A senha precisa ter pelo menos 8 caracteres.', {}, wait)).toBeTruthy()
    fireEvent.change(view.getByLabelText('Senha'), { target: { value: 'segredo123' } })
    fireEvent.change(view.getByLabelText('Confirme a senha'), { target: { value: 'segredo124' } })
    fireEvent.click(view.getByRole('button', { name: 'Criar e entrar' }))
    expect(await view.findByText('As senhas não são iguais.', {}, wait)).toBeTruthy()
    fireEvent.change(view.getByLabelText('Confirme a senha'), { target: { value: 'segredo123' } })
    fireEvent.click(view.getByRole('button', { name: 'Criar e entrar' }))
    await waitFor(() => expect(calls.find((c) => c.method === 'POST' && c.path === AUTH.setup)?.body).toEqual({ token: 'TOK', login: 'admin', password: 'segredo123' }), wait)
    await waitFor(() => expect(sessionStorage.getItem(INSTALLED_KEY)).toBe('1'), wait)
    sessionStorage.clear()
  }, 20000)

  test('sem o token o login avisa que o servidor espera o instalador', async () => {
    serve({ [`GET ${AUTH.methods}`]: { status: 200, body: { mode: 'shared', methods: ['password'], ssoLabel: '', setup: true } } })
    const view = render(<MemoryRouter initialEntries={['/login']}><Login /></MemoryRouter>)
    expect(await view.findByText(/aguardando o primeiro administrador/, {}, wait)).toBeTruthy()
    expect(view.queryByRole('button', { name: 'Entrar' })).toBeNull()
  }, 20000)
})
