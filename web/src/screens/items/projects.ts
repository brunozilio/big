export interface ProjectChoice {
  connection: string
  key: string
}

const PREFIX = 'composer.project.'

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null
  } catch {
    return null
  }
}

export function rememberedProject(connection: string): string | null {
  try {
    return storage()?.getItem(PREFIX + connection) || null
  } catch {
    return null
  }
}

export function rememberProject(connections: string[], choice: ProjectChoice | null): void {
  try {
    const kept = storage()
    if (!kept) return
    for (const connection of connections) {
      if (choice && connection === choice.connection) kept.setItem(PREFIX + connection, choice.key)
      else kept.removeItem(PREFIX + connection)
    }
  } catch {
    return
  }
}

export function initialProject(connections: string[]): ProjectChoice | null {
  for (const connection of connections) {
    const key = rememberedProject(connection)
    if (key) return { connection, key }
  }
  return null
}

export interface SearchTarget {
  connection: string
  args: { query: string; project?: string }
}

export function searchTargets(connections: string[], choice: ProjectChoice | null, query: string): SearchTarget[] {
  if (choice && connections.includes(choice.connection)) return [{ connection: choice.connection, args: { query, project: choice.key } }]
  return connections.map((connection) => ({ connection, args: { query } }))
}

export function sameProject(a: ProjectChoice | null, b: ProjectChoice | null): boolean {
  return a?.connection === b?.connection && a?.key === b?.key
}
