import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, within } from '@testing-library/react'
import { setApi } from '../src/api'
import { mockApi } from '../src/api/mock'
import { boot, stop } from '../src/api/store'
import { ConnectDialog } from '../src/screens/settings/ConnectDialog'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
  localStorage.removeItem('mock:signIn')
})

async function opened(provider: string, resume?: (api: ReturnType<typeof mockApi>) => Promise<string>, onClose: () => void = () => undefined) {
  const api = mockApi({ tickMs: 0, delayMs: 0 })
  setApi(api)
  await act(() => boot(api))
  const flow = resume ? await resume(api) : null
  const view = render(
    <TooltipProvider>
      <ConnectDialog provider={provider} resume={flow} onClose={onClose} />
    </TooltipProvider>,
  )
  return within(view.baseElement.querySelector('[role="dialog"]') as HTMLElement)
}

const wait = { timeout: 6000 }

describe('diálogo de conexão', () => {
  test('código de dispositivo leva à escolha de conta e ao estado conectado', async () => {
    const dialog = await opened('github')
    expect(dialog.getByText('Sua conta GitHub')).toBeTruthy()
    expect(dialog.getByText('Contas conectadas')).toBeTruthy()
    fireEvent.click(dialog.getByRole('button', { name: /Entrar com GitHub/ }))
    expect(await dialog.findByLabelText('Código de verificação', {}, wait)).toBeTruthy()
    expect(dialog.getByRole('button', { name: 'Cancelar' })).toBeTruthy()
    fireEvent.click(await dialog.findByRole('radio', { name: /acme/ }, wait))
    fireEvent.click(dialog.getByRole('button', { name: 'Salvar' }))
    expect(await dialog.findByText('Conectado', {}, wait)).toBeTruthy()
    expect(dialog.getByRole('button', { name: 'Concluir' })).toBeTruthy()
  }, 10000)

  test('provedor de IA usa o mesmo passo do navegador e o mesmo fim', async () => {
    const dialog = await opened('chatgpt')
    fireEvent.click(dialog.getByRole('button', { name: /Entrar com ChatGPT/ }))
    const pasted = await dialog.findByLabelText('Código ou endereço de retorno', {}, wait)
    fireEvent.change(pasted, { target: { value: 'http://localhost:1455/callback?code=abc' } })
    fireEvent.click(dialog.getByRole('button', { name: 'Concluir' }))
    expect(await dialog.findByText('Conectado', {}, wait)).toBeTruthy()
  }, 10000)

  test('o API token conecta sem navegador e o campo esconde o token', async () => {
    const dialog = await opened('zephyr')
    fireEvent.click(dialog.getByRole('button', { name: 'Usar API token' }))
    const token = (await dialog.findByLabelText(/^API token do Zephyr/, {}, wait)) as HTMLInputElement
    expect(token.type).toBe('password')
    expect(dialog.getByText(/Zephyr API Access Tokens/)).toBeTruthy()
    expect((dialog.getByRole('button', { name: /Conectar/ }) as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(token, { target: { value: 'zt-123' } })
    fireEvent.click(dialog.getByRole('button', { name: /Conectar/ }))
    expect(await dialog.findByText('Conectado', {}, wait)).toBeTruthy()
  }, 10000)

  test('um servidor MCP sem login conecta com o endereço e o nome e mostra as ferramentas', async () => {
    const dialog = await opened('mcp')
    const url = dialog.getByLabelText(/^Endereço do servidor MCP/) as HTMLInputElement
    expect(url.type).toBe('text')
    expect((dialog.getByRole('button', { name: /Conectar/ }) as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(url, { target: { value: 'ftp://docs.acme' } })
    fireEvent.change(dialog.getByLabelText(/^Nome/), { target: { value: 'Docs' } })
    fireEvent.click(dialog.getByRole('button', { name: /Conectar/ }))
    expect((await dialog.findByRole('alert', {}, wait)).textContent).toContain('https')
    fireEvent.change(url, { target: { value: 'http://localhost:3000/mcp' } })
    fireEvent.click(dialog.getByRole('button', { name: /Conectar/ }))
    expect(await dialog.findByText('Conectado', {}, wait)).toBeTruthy()
  }, 10000)

  test('a conexão MCP já feita lista as suas ferramentas', async () => {
    const dialog = await opened('mcp', async (api) => {
      const start = await api.startSignIn('mcp', { url: 'http://localhost:9/mcp', label: 'Docs' })
      for (let i = 0; i < 3; i++) await api.pollSignIn({ flow: start.flow })
      return ''
    })
    expect(await dialog.findByText('Docs', { exact: false }, wait)).toBeTruthy()
    expect(await dialog.findByText('· search, fetch', {}, wait)).toBeTruthy()
  }, 10000)

  test('a recusa volta ao início com o erro no mesmo lugar', async () => {
    localStorage.setItem('mock:signIn', 'failed')
    const dialog = await opened('jira')
    fireEvent.click(dialog.getByRole('button', { name: /Continuar na Atlassian/ }))
    expect(await dialog.findByText(/Aguardando a autorização/, {}, wait)).toBeTruthy()
    expect((await dialog.findByRole('alert', {}, wait)).textContent).toContain('recusada')
    expect(dialog.getByRole('button', { name: /Continuar na Atlassian/ })).toBeTruthy()
  }, 10000)

  test('a volta do login na mesma aba conclui a entrada e fecha o diálogo', async () => {
    let closed = false
    const dialog = await opened('chatgpt', async (api) => (await api.startSignIn('chatgpt', {})).flow, () => {
      closed = true
    })
    expect(dialog.getByText(/Concluindo a conexão/)).toBeTruthy()
    for (let i = 0; i < 60 && !closed; i++) await act(() => new Promise((r) => setTimeout(r, 100)))
    expect(closed).toBe(true)
  }, 10000)
})
