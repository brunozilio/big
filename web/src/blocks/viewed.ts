import { useEffect, useState } from 'react'

function load(key: string): string[] {
  try {
    const parsed: unknown = JSON.parse(globalThis.localStorage?.getItem(key) ?? '[]')
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : []
  } catch {
    return []
  }
}

function save(key: string, value: string[]): void {
  try {
    const kept = globalThis.localStorage
    if (!kept) return
    if (value.length > 0) kept.setItem(key, JSON.stringify(value))
    else kept.removeItem(key)
  } catch {
    return
  }
}

export function useViewed(key: string) {
  const [viewed, setViewed] = useState<string[]>(() => load(key))
  useEffect(() => setViewed(load(key)), [key])
  const update = (next: string[]) => {
    setViewed(next)
    save(key, next)
  }
  const toggle = (id: string) => update(viewed.includes(id) ? viewed.filter((x) => x !== id) : [...viewed, id])
  const clear = () => update([])
  return { viewed, toggle, clear }
}
