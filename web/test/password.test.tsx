import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import { AUTH, changePassword, signInWithPassword } from '../src/api/auth'
import { mockApi } from '../src/api/mock'
import { AccountPanel } from '../src/screens/login/Account'
import { Login } from '../src/screens/login/Login'
import { PEOPLE, PeopleView } from '../src/screens/settings/People'

const realFetch = globalThis.fetch

interface Call {
  method: string
  path: string
  body: unknown
}

type Reply = { status: number; body: unknown }

function serve(routes: Record<string, Reply>): Call[] {
  const calls: Call[] = []
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    calls.push({ method, path: String(path), body: init?.body ? JSON.parse(String(init.body)) : undefined })
    const reply = routes[`${method} ${String(path)}`] ?? { status: 200, body: {} }
    return new Response(JSON.stringify(reply.body), { status: reply.status })
  }) as typeof fetch
  return calls
}

function sent(calls: Call[], method: string, path: string): Call | undefined {
  return calls.find((call) => call.method === method && call.path === path)
}

const wait = { timeout: 8000 }

const shared = (methods: string[]): Reply => ({ status: 200, body: { mode: 'shared', methods, ssoLabel: '' } })

beforeEach(() => setApi(mockApi({ tickMs: 0, delayMs: 0 })))

afterEach(() => {
  cleanup()
  setApi(null)
  globalThis.fetch = realFetch
})

describe('senha', () => {
  test('os atalhos de senha chamam as rotas do contrato', async () => {
    const calls = serve({ [`POST ${AUTH.password}`]: { status: 200, body: { user: 'u', email: 'ana', name: 'Ana' } } })
    const person = await signInWithPassword('ana', 'segredo123')
    expect(person.email).toBe('ana')
    await changePassword('velha1234', 'nova12345')
    expect(sent(calls, 'POST', '/api/auth/password')?.body).toEqual({ login: 'ana', password: 'segredo123' })
    expect(sent(calls, 'PUT', '/api/account/password')?.body).toEqual({ current: 'velha1234', password: 'nova12345' })
  }, 20000)

  test('com senha ligada o login pede usuário e senha e não mostra o e-mail', async () => {
    const calls = serve({
      [`GET ${AUTH.methods}`]: shared(['password', 'email', 'passkey']),
      [`POST ${AUTH.password}`]: { status: 400, body: { error: { code: 'bad', message: 'Usuário ou senha incorretos.' } } },
    })
    const view = render(<MemoryRouter initialEntries={['/login']}><Login /></MemoryRouter>)
    const login = (await view.findByLabelText('Usuário', {}, wait)) as HTMLInputElement
    const password = view.getByLabelText('Senha') as HTMLInputElement
    expect(login.autocomplete).toBe('username')
    expect(password.autocomplete).toBe('current-password')
    expect(view.queryByLabelText(/^E-mail/)).toBeNull()
    expect(view.getByText('ou continue com')).toBeTruthy()
    expect(view.getByRole('button', { name: /Passkey/ })).toBeTruthy()
    fireEvent.change(login, { target: { value: ' ana ' } })
    fireEvent.change(password, { target: { value: 'errada123' } })
    fireEvent.click(view.getByRole('button', { name: 'Entrar' }))
    expect(await view.findByText('Usuário ou senha incorretos.', {}, wait)).toBeTruthy()
    expect(sent(calls, 'POST', '/api/auth/password')?.body).toEqual({ login: 'ana', password: 'errada123' })
  }, 20000)

  test('sem senha ligada o e-mail continua aparecendo', async () => {
    serve({ [`GET ${AUTH.methods}`]: shared(['email']) })
    const view = render(<MemoryRouter initialEntries={['/login']}><Login /></MemoryRouter>)
    expect(await view.findByLabelText(/^E-mail/, {}, wait)).toBeTruthy()
    expect(view.queryByLabelText('Usuário')).toBeNull()
  }, 20000)

  test('a conta confere a nova senha antes de trocar', async () => {
    const calls = serve({
      [`GET ${AUTH.methods}`]: shared(['password', 'passkey']),
      [`GET ${AUTH.me}`]: { status: 200, body: { mode: 'shared', user: { user: 'u', email: 'ana', name: 'Ana' }, admin: false } },
      [`GET ${AUTH.passkeys}`]: { status: 200, body: [] },
    })
    const view = render(<MemoryRouter><AccountPanel /></MemoryRouter>)
    const current = await view.findByLabelText('Senha atual', {}, wait)
    fireEvent.change(current, { target: { value: 'velha1234' } })
    fireEvent.change(view.getByLabelText('Nova senha'), { target: { value: 'curta' } })
    fireEvent.change(view.getByLabelText('Confirme a nova senha'), { target: { value: 'curta' } })
    fireEvent.click(view.getByRole('button', { name: 'Trocar senha' }))
    expect(await view.findByText('A senha precisa ter pelo menos 8 caracteres.', {}, wait)).toBeTruthy()
    fireEvent.change(view.getByLabelText('Nova senha'), { target: { value: 'nova12345' } })
    fireEvent.click(view.getByRole('button', { name: 'Trocar senha' }))
    expect(await view.findByText('As senhas não são iguais.', {}, wait)).toBeTruthy()
    fireEvent.change(view.getByLabelText('Confirme a nova senha'), { target: { value: 'nova12345' } })
    fireEvent.click(view.getByRole('button', { name: 'Trocar senha' }))
    expect(await view.findByText('Senha trocada.', {}, wait)).toBeTruthy()
    expect(sent(calls, 'PUT', AUTH.accountPassword)?.body).toEqual({ current: 'velha1234', password: 'nova12345' })
  }, 20000)

  test('o admin adiciona pessoa e define a senha de alguém', async () => {
    const row = { id: 'p1', email: 'bia', name: 'Bia', created: 1, seen: 0, sessions: 0, admin: false }
    const calls = serve({
      [`GET ${AUTH.methods}`]: shared(['password']),
      [`GET ${PEOPLE}`]: { status: 200, body: [row] },
      [`POST ${PEOPLE}`]: { status: 400, body: { error: { code: 'exists', message: 'Esse usuário já existe.' } } },
    })
    const view = render(<PeopleView />)
    expect(await view.findByText('ainda não entrou', { exact: false }, wait)).toBeTruthy()
    fireEvent.change(await view.findByLabelText('Usuário', {}, wait), { target: { value: 'bia' } })
    fireEvent.change(view.getByLabelText('Nome'), { target: { value: 'Bia' } })
    fireEvent.change(view.getByLabelText('Senha inicial'), { target: { value: 'inicial123' } })
    fireEvent.click(view.getByRole('button', { name: /Adicionar pessoa/ }))
    expect(await view.findByText('Esse usuário já existe.', {}, wait)).toBeTruthy()
    expect(sent(calls, 'POST', PEOPLE)?.body).toEqual({ login: 'bia', name: 'Bia', password: 'inicial123' })
    fireEvent.click(view.getByRole('button', { name: 'Definir senha' }))
    fireEvent.change(view.getByLabelText('Nova senha'), { target: { value: 'outra1234' } })
    fireEvent.click(view.getByRole('button', { name: 'Salvar senha' }))
    await waitFor(() => expect(view.queryByLabelText('Nova senha')).toBeNull(), wait)
    expect(sent(calls, 'PUT', `${PEOPLE}/p1/password`)?.body).toEqual({ password: 'outra1234' })
  }, 20000)

  test('sem senha ligada o admin não vê os controles de senha', async () => {
    serve({
      [`GET ${AUTH.methods}`]: shared(['email']),
      [`GET ${PEOPLE}`]: { status: 200, body: [{ id: 'p1', email: 'a@b.co', name: '', created: 1, seen: 1, sessions: 1, admin: true }] },
    })
    const view = render(<PeopleView />)
    expect(await view.findByText('a@b.co', {}, wait)).toBeTruthy()
    expect(view.queryByRole('button', { name: 'Definir senha' })).toBeNull()
    expect(view.queryByText('Adicionar pessoa')).toBeNull()
  }, 20000)
})
