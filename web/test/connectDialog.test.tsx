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

async function opened(provider: string) {
  const api = mockApi({ tickMs: 0, delayMs: 0 })
  setApi(api)
  await act(() => boot(api))
  const view = render(
    <TooltipProvider>
      <ConnectDialog provider={provider} onClose={() => undefined} />
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

  test('a recusa volta ao início com o erro no mesmo lugar', async () => {
    localStorage.setItem('mock:signIn', 'failed')
    const dialog = await opened('jira')
    fireEvent.click(dialog.getByRole('button', { name: /Continuar na Atlassian/ }))
    expect(await dialog.findByText(/Aguardando a autorização/, {}, wait)).toBeTruthy()
    expect((await dialog.findByRole('alert', {}, wait)).textContent).toContain('recusada')
    expect(dialog.getByRole('button', { name: /Continuar na Atlassian/ })).toBeTruthy()
  }, 10000)
})
