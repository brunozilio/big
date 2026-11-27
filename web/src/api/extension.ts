const WAIT_MS = 150000

export interface ExtensionAnswer {
  ok: boolean
  reason?: string
}

function marker(): string | null {
  return globalThis.document?.documentElement?.dataset.signInRelay ?? null
}

export function extensionBuild(): string | null {
  return globalThis.document?.documentElement?.dataset.browserExtension ?? null
}

export function askExtension(type: 'browser-connect' | 'browser-reload', payload: Record<string, string>, waitMs = WAIT_MS): Promise<ExtensionAnswer> {
  const source = marker()
  if (!source) return Promise.resolve({ ok: false, reason: 'missing' })
  const id = globalThis.crypto.randomUUID()
  return new Promise((resolve) => {
    const finish = (answer: ExtensionAnswer) => {
      clearTimeout(timer)
      globalThis.removeEventListener('message', listen)
      resolve(answer)
    }
    const listen = (event: MessageEvent) => {
      if (event.source !== globalThis.window) return
      const data = event.data as { source?: string; type?: string; id?: string; ok?: boolean; reason?: string } | null
      if (data?.source !== source || data.type !== `${type}-done` || data.id !== id) return
      finish({ ok: data.ok === true, reason: data.reason })
    }
    const timer = setTimeout(() => finish({ ok: false, reason: 'timeout' }), waitMs)
    globalThis.addEventListener('message', listen)
    globalThis.postMessage({ source, type, id, ...payload }, globalThis.location.origin)
  })
}

export function connectBrowser(server: string, code: string): Promise<ExtensionAnswer> {
  return askExtension('browser-connect', { server, code })
}

export function reloadExtension(): Promise<ExtensionAnswer> {
  return askExtension('browser-reload', {}, 5000)
}
