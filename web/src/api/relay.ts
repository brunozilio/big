import type { SignInRelay } from './contract'

const WAIT_MS = 60000

interface Ready {
  source: string
  type: 'relay-sign-in-ready'
  id: string
  ok: boolean
}

function isReady(data: unknown, marker: string, id: string): data is Ready {
  if (!data || typeof data !== 'object') return false
  const answer = data as Partial<Ready>
  return answer.source === marker && answer.type === 'relay-sign-in-ready' && answer.id === id
}

export function askRelay(relay: SignInRelay, waitMs = WAIT_MS): Promise<boolean> {
  const marker = globalThis.document?.documentElement?.dataset.signInRelay
  if (!marker) return Promise.resolve(false)
  const id = globalThis.crypto.randomUUID()
  return new Promise((resolve) => {
    const finish = (ok: boolean) => {
      clearTimeout(timer)
      globalThis.removeEventListener('message', listen)
      resolve(ok)
    }
    const listen = (event: MessageEvent) => {
      if (event.source !== globalThis.window) return
      if (isReady(event.data, marker, id)) finish(event.data.ok === true)
    }
    const timer = setTimeout(() => finish(false), waitMs)
    globalThis.addEventListener('message', listen)
    globalThis.postMessage({ source: marker, type: 'relay-sign-in', id, from: relay.from, to: relay.to }, globalThis.location.origin)
  })
}
