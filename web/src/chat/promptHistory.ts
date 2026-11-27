import { useRef } from 'react'

const KEY = 'composer.history'
const LIMIT = 200

export function storedPrompts(): string[] {
  try {
    const parsed: unknown = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? '[]')
    return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === 'string' && p.trim().length > 0) : []
  } catch {
    return []
  }
}

export function rememberPrompt(text: string): void {
  const prompt = text.trim()
  if (!prompt) return
  const kept = storedPrompts().filter((p) => p !== prompt)
  kept.push(prompt)
  try {
    globalThis.localStorage?.setItem(KEY, JSON.stringify(kept.slice(-LIMIT)))
  } catch {
    return
  }
}

export function onFirstLine(text: string, start: number, end: number): boolean {
  return start === end && !text.slice(0, start).includes('\n')
}

export function onLastLine(text: string, start: number, end: number): boolean {
  return start === end && !text.slice(end).includes('\n')
}

export interface Recall {
  text: string
}

export function usePromptHistory() {
  const cursor = useRef<number | null>(null)
  const draft = useRef('')
  const entries = useRef<string[]>([])

  const older = (current: string): Recall | null => {
    if (cursor.current === null) {
      entries.current = storedPrompts()
      if (!entries.current.length) return null
      draft.current = current
      cursor.current = entries.current.length
    }
    if (cursor.current === 0) return null
    cursor.current -= 1
    return { text: entries.current[cursor.current] ?? '' }
  }

  const newer = (): Recall | null => {
    if (cursor.current === null) return null
    cursor.current += 1
    if (cursor.current >= entries.current.length) {
      cursor.current = null
      return { text: draft.current }
    }
    return { text: entries.current[cursor.current] ?? '' }
  }

  const reset = () => {
    cursor.current = null
    draft.current = ''
  }

  const record = (text: string) => {
    rememberPrompt(text)
    reset()
  }

  return { older, newer, reset, record }
}
