export interface Space {
  connection: string
  key: string
}

export interface SearchTarget {
  connection: string
  args: { query: string; project?: string }
}

export function configuredSpaces(applications: { tracker: { connection: string; project: string } | null }[], connections: string[]): Space[] {
  const found: Space[] = []
  for (const a of applications) {
    const t = a.tracker
    if (!t || !connections.includes(t.connection)) continue
    if (!found.some((s) => s.connection === t.connection && s.key === t.project)) found.push({ connection: t.connection, key: t.project })
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
