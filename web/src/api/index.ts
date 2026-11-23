import type { Api } from './contract'
import { httpApi } from './http'

let current: Api | null = null

export function isMock(): boolean {
  return import.meta.env.VITE_MOCK === '1'
}

export async function loadApi(): Promise<Api> {
  if (current) return current
  if (isMock()) {
    const { mockApi } = await import('./mock')
    current = mockApi()
  } else current = httpApi()
  return current
}

export function setApi(next: Api | null) {
  current = next
}

export function api(): Api {
  if (!current) throw new Error('api não carregada')
  return current
}
