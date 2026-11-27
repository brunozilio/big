import { at, appPath, toLogin } from './base'
import { isLiveEvent } from './guards'
import { currentLanguage, translate } from '@/i18n'
import { ROUTES, type Api, type CallAction, type CallArgs, type CallResult, type LiveEvent } from './contract'

export class ApiError extends Error {
  constructor(readonly status: number, readonly code: string, message: string) {
    super(message)
  }
}

export type Fetcher = (input: string, init?: RequestInit) => Promise<Response>

export async function request<T>(method: string, path: string, body?: unknown, fetcher: Fetcher = fetch): Promise<T> {
  let res: Response
  try {
    res = await fetcher(at(path), {
      method,
      headers: body === undefined ? { accept: 'application/json' } : { accept: 'application/json', 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiError(0, 'offline', translate(currentLanguage(), 'app.offline'))
  }
  const text = await res.text()
  const data: unknown = text ? safeJson(text) : null
  if (!res.ok) {
    const error = (data as { error?: { code?: string; message?: string } } | null)?.error
    if (res.status === 401 && error?.code === 'signed_out') signedOut()
    throw new ApiError(res.status, error?.code ?? 'http', error?.message ?? translate(currentLanguage(), 'app.httpError', { status: res.status }))
  }
  return data as T
}

type Told = { type?: string; text?: string; message?: string }

function heard<T>(block: string, onReply: ((text: string) => void) | undefined): T | null {
  const data = block.split('\n').filter((line) => line.startsWith('data:')).map((line) => line.slice(5).trimStart()).join('\n')
  const told = data ? (safeJson(data) as Told | null) : null
  if (!told) return null
  if (told.type === 'reply') onReply?.(told.text ?? '')
  if (told.type === 'error') throw new ApiError(502, 'assist', told.message ?? translate(currentLanguage(), 'app.somethingWrong'))
  return told.type === 'done' ? (told as T) : null
}

export async function streamed<T>(fetcher: Fetcher, path: string, body: unknown, onReply?: (text: string) => void): Promise<T> {
  let res: Response
  try {
    res = await fetcher(path, { method: 'POST', headers: { accept: 'text/event-stream', 'content-type': 'application/json' }, body: JSON.stringify(body) })
  } catch {
    throw new ApiError(0, 'offline', translate(currentLanguage(), 'app.offline'))
  }
  if (!res.ok) {
    const error = (safeJson(await res.text()) as { error?: { code?: string; message?: string } } | null)?.error
    throw new ApiError(res.status, error?.code ?? 'http', error?.message ?? translate(currentLanguage(), 'app.httpError', { status: res.status }))
  }
  const reader = res.body?.getReader()
  const decoder = new TextDecoder()
  let pending = reader ? '' : await res.text()
  let done: T | null = null
  const drain = (all: boolean) => {
    const blocks = pending.split('\n\n')
    pending = all ? '' : (blocks.pop() ?? '')
    for (const block of blocks) done = heard<T>(block, onReply) ?? done
  }
  while (reader) {
    const chunk = await reader.read()
    if (chunk.done) break
    pending += decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, '\n')
    drain(false)
  }
  drain(true)
  if (!done) throw new ApiError(502, 'assist', translate(currentLanguage(), 'app.somethingWrong'))
  return done
}

function signedOut() {
  if (typeof window === 'undefined' || appPath() === '/login') return
  window.location.assign(toLogin())
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

export interface LiveSource {
  onmessage: ((e: { data: string; lastEventId?: string }) => void) | null
  onopen: (() => void) | null
  onerror: (() => void) | null
  close(): void
}

export function followLive(open: (url: string) => LiveSource, onEvent: (e: LiveEvent) => void, onState: (connected: boolean) => void): () => void {
  let source: LiveSource | null = null
  let delay = 500
  let timer: ReturnType<typeof setTimeout> | null = null
  let stopped = false
  let last = ''
  const connect = () => {
    source = open(at(last ? `${ROUTES.live}?after=${encodeURIComponent(last)}` : ROUTES.live))
    source.onopen = () => {
      delay = 500
      onState(true)
    }
    source.onmessage = (e) => {
      if (e.lastEventId) last = e.lastEventId
      const parsed = safeJson(e.data)
      if (isLiveEvent(parsed)) onEvent(parsed)
    }
    source.onerror = () => {
      source?.close()
      onState(false)
      if (stopped) return
      timer = setTimeout(connect, delay)
      delay = Math.min(delay * 2, 30_000)
    }
  }
  connect()
  return () => {
    stopped = true
    if (timer) clearTimeout(timer)
    source?.close()
  }
}

export function httpApi(fetcher: Fetcher = (input, init) => fetch(input, init), open: (url: string) => LiveSource = (url) => new EventSource(url) as unknown as LiveSource): Api {
  const call = <T>(method: string, path: string, body?: unknown) => request<T>(method, path, body, fetcher)
  return {
    items: () => call('GET', ROUTES.items),
    createItem: (input) => call('POST', ROUTES.items, input),
    rerun: (item, step) => call('POST', ROUTES.rerun(item), { step }),
    cancel: (item) => call('POST', ROUTES.cancel(item), {}),
    baseBranches: (item, repository) => call('GET', ROUTES.baseBranches(item, repository)),
    chooseBase: (item, choice) => call('POST', ROUTES.chooseBase(item), choice),
    openFolders: (item) => call('POST', ROUTES.openFolders(item), {}),
    archive: (item) => call('POST', ROUTES.archive(item), {}),
    unarchive: (item) => call('DELETE', ROUTES.archive(item)),
    step: (item, step, attempt) => call('GET', attempt === null ? ROUTES.step(item, step) : `${ROUTES.step(item, step)}?attempt=${attempt}`),
    input: (item, step, input) => call('POST', ROUTES.input(item, step), input),
    itemUsage: (item) => call('GET', ROUTES.itemUsage(item)),
    call: <A extends CallAction>(connection: string, action: A, args: CallArgs<A>) => call<CallResult<A>>('POST', ROUTES.call(connection), { action, args }),
    session: (id) => call('GET', ROUTES.session(id)),
    send: (session, message) => call('POST', ROUTES.messages(session), message),
    side: (session, question) => call('POST', ROUTES.side(session), question),
    btw: (question) => call('POST', ROUTES.btw, question),
    respond: (session, event, response) => call('POST', ROUTES.respond(session), { event, response }),
    stop: (session) => call('POST', ROUTES.stop(session), {}),
    stopJob: (session, job) => call('POST', ROUTES.stopJob(session, job), {}),
    openFile: (session, file) => call('POST', ROUTES.openFile(session), file),
    connections: () => call('GET', ROUTES.connections),
    startSignIn: (provider, fields, method) => call('POST', ROUTES.start(provider), { fields, method }),
    pollSignIn: (req) => call('POST', ROUTES.poll, req),
    disconnect: (connection) => call('DELETE', ROUTES.connection(connection)),
    models: () => call('GET', ROUTES.models),
    usage: (days) => call('GET', ROUTES.usage(days)),
    usageLimits: () => call('GET', ROUTES.usageLimits),
    branding: () => call('GET', ROUTES.branding),
    saveBranding: (patch) => call('PUT', ROUTES.branding, patch),
    onboarding: () => call('GET', ROUTES.onboarding),
    saveOnboarding: (state) => call('PUT', ROUTES.onboarding, state),
    applications: () => call('GET', ROUTES.applications),
    createApplication: (application) => call('POST', ROUTES.applications, application),
    saveApplication: (application) => call('PUT', ROUTES.application(application.id), application),
    deleteApplication: (id) => call('DELETE', ROUTES.application(id)),
    dirs: (path) => call('GET', ROUTES.dirs(path)),
    library: () => call('GET', ROUTES.library),
    libraryItem: (path) => call('GET', ROUTES.libraryItem(path)),
    saveLibraryItem: (path, change) => call('PUT', ROUTES.libraryItem(null), { path, ...change }),
    createLibraryItem: (section, name) => call('POST', ROUTES.libraryItem(null), { section, name }),
    restoreLibraryItem: (path) => call('DELETE', ROUTES.libraryItem(path)),
    createLibraryFile: (item, name) => call('POST', ROUTES.libraryItem(null), { item, name }),
    assistLibrary: (path, draft, messages, ask) => streamed(fetcher, ROUTES.libraryAssist, { path, ...draft, messages, model: ask?.model ?? null }, ask?.onReply),
    upload: async (file) => {
      const form = new FormData()
      form.append('file', file)
      const res = await fetcher(at(ROUTES.attachments), { method: 'POST', body: form })
      if (!res.ok) throw new ApiError(res.status, 'upload', translate(currentLanguage(), 'app.uploadFailed'))
      return (await res.json()) as { id: string; name: string; size: number }
    },
    live: (onEvent, onState) => followLive(open, onEvent, onState),
  }
}
