import type { SignInRelay } from './contract'

const WAIT_MS = 150000

interface Ready {
  source: string
  type: 'relay-sign-in-ready'
  id: string
  ok: boolean
  opened?: boolean
}

export interface Relayed {
  ok: boolean
  opened: boolean
}

function isReady(data: unknown, marker: string, id: string): data is Ready {
  if (!data || typeof data !== 'object') return false
  const answer = data as Partial<Ready>
  return answer.source === marker && answer.type === 'relay-sign-in-ready' && answer.id === id
}

export function askRelay(relay: SignInRelay, url: string, waitMs = WAIT_MS): Promise<Relayed> {
  const marker = globalThis.document?.documentElement?.dataset.signInRelay
  if (!marker) return Promise.resolve({ ok: false, opened: false })
  const id = globalThis.crypto.randomUUID()
  return new Promise((resolve) => {
    const finish = (answer: Relayed) => {
      clearTimeout(timer)
      globalThis.removeEventListener('message', listen)
      resolve(answer)
    }
    const listen = (event: MessageEvent) => {
      if (event.source !== globalThis.window) return
      if (isReady(event.data, marker, id)) finish({ ok: event.data.ok === true, opened: event.data.opened === true })
    }
    const timer = setTimeout(() => finish({ ok: false, opened: false }), waitMs)
    globalThis.addEventListener('message', listen)
    globalThis.postMessage({ source: marker, type: 'relay-sign-in', id, from: relay.from, to: relay.to, url }, globalThis.location.origin)
  })
}
