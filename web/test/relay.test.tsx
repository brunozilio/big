import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render } from '@testing-library/react'
import { askRelay } from '../src/api/relay'
import { BrowserWait } from '../src/screens/settings/SignInSteps'

const relay = { from: 'http://127.0.0.1:7420/oauth/callback', to: 'http://localhost/app/oauth/callback' }
const marker = 'teste:relay'

interface Asked {
  id: string
  from: string
  to: string
}

function extension(answer: (asked: Asked) => unknown) {
  const listen = (event: MessageEvent) => {
    const data = event.data as { source?: string; type?: string } & Asked
    if (data?.source !== marker || data.type !== 'relay-sign-in') return
    const reply = answer(data)
    window.dispatchEvent(new MessageEvent('message', { data: reply, source: window }))
  }
  window.addEventListener('message', listen)
  return () => window.removeEventListener('message', listen)
}

afterEach(() => {
  cleanup()
  delete document.documentElement.dataset.signInRelay
})

describe('repasse do login pela extensão', () => {
  test('sem a extensão responde não na hora', async () => {
    expect(await askRelay(relay)).toBe(false)
  })

  test('a resposta pronta com o mesmo id confirma o repasse', async () => {
    document.documentElement.dataset.signInRelay = marker
    let seen: Asked | null = null
    const stop = extension((asked) => {
      seen = asked
      return { source: marker, type: 'relay-sign-in-ready', id: asked.id, ok: true }
    })
    expect(await askRelay(relay)).toBe(true)
    expect(seen).toMatchObject({ from: relay.from, to: relay.to })
    stop()
  })

  test('uma resposta com outro id é ignorada', async () => {
    document.documentElement.dataset.signInRelay = marker
    const stop = extension(() => ({ source: marker, type: 'relay-sign-in-ready', id: 'outro', ok: true }))
    expect(await askRelay(relay, 200)).toBe(false)
    stop()
  })

  test('a recusa da extensão responde não', async () => {
    document.documentElement.dataset.signInRelay = marker
    const stop = extension((asked) => ({ source: marker, type: 'relay-sign-in-ready', id: asked.id, ok: false, reason: 'declined' }))
    expect(await askRelay(relay)).toBe(false)
    stop()
  })

  test('a espera mostra a confirmação na extensão e mantém o campo para colar', () => {
    const start = { flow: 'f', kind: 'redirect' as const, url: 'https://login.example.com/authorize', paste: true, relay }
    const view = render(<BrowserWait site="do Exemplo" start={start} relaying pasted="" onPasted={() => undefined} />)
    expect(view.getByText('Confirme na extensão do navegador…')).toBeTruthy()
    expect(view.getByText(/Se a página não voltar sozinha/)).toBeTruthy()
    expect(view.getByLabelText('Código ou endereço de retorno')).toBeTruthy()
  })

  test('sem repasse não aparece a dica', () => {
    const start = { flow: 'f', kind: 'redirect' as const, url: 'https://login.example.com/authorize', paste: true }
    const view = render(<BrowserWait site="do Exemplo" start={start} pasted="" onPasted={() => undefined} />)
    expect(view.queryByText(/Se a página não voltar sozinha/)).toBeNull()
    expect(view.queryByText('Confirme na extensão do navegador…')).toBeNull()
  })
})
