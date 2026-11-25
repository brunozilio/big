import { GlobalRegistrator } from '@happy-dom/global-registrator'

const keep = { fetch: globalThis.fetch, Request: globalThis.Request, Response: globalThis.Response, Headers: globalThis.Headers }
GlobalRegistrator.register({ url: 'http://localhost/' })
Object.assign(globalThis, keep)
Object.defineProperty(globalThis.navigator, 'language', { value: 'pt-BR', configurable: true })
Object.defineProperty(globalThis.navigator, 'languages', { value: ['pt-BR'], configurable: true })
