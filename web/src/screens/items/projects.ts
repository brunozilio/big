import type { Bindings } from '@/api/contract'

export interface Space {
  connection: string
  key: string
}

export interface SearchTarget {
  connection: string
  args: { query: string; project?: string }
}

export function configuredSpaces(applications: { bindings?: Bindings }[], connections: string[]): Space[] {
  const found: Space[] = []
  for (const bound of applications.flatMap((a) => Object.values(a.bindings ?? {}))) {
    if (!connections.includes(bound.connection)) continue
    for (const key of [bound.value, ...(bound.more ?? []).map((c) => c.value)]) {
      if (!found.some((s) => s.connection === bound.connection && s.key === key)) found.push({ connection: bound.connection, key })
    }
  }
  return found
}

export function searchTargets(connections: string[], spaces: Space[], query: string): SearchTarget[] {
  const kept = spaces.filter((s) => connections.includes(s.connection))
  if (kept.length > 0) return kept.map((s) => ({ connection: s.connection, args: { query, project: s.key } }))
  return connections.map((connection) => ({ connection, args: { query } }))
}

const APPLICATION = 'composer.application'

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null
  } catch {
    return null
  }
}

export function rememberedApplication(): string | null {
  try {
    return storage()?.getItem(APPLICATION) || null
  } catch {
    return null
  }
}

export function rememberApplication(id: string | null): void {
  try {
    const kept = storage()
    if (!kept) return
    if (id) kept.setItem(APPLICATION, id)
    else kept.removeItem(APPLICATION)
  } catch {
    return
  }
}
