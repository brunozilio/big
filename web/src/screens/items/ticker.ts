import { useSyncExternalStore } from 'react'
import { minutesSince } from '@/api/clock'

const TICK_MS = 5_000

const subscribers = new Set<() => void>()
let timer: ReturnType<typeof setInterval> | null = null

function subscribe(listener: () => void) {
  subscribers.add(listener)
  if (timer === null) timer = setInterval(() => subscribers.forEach((l) => l()), TICK_MS)
  return () => {
    subscribers.delete(listener)
    if (subscribers.size === 0 && timer !== null) {
      clearInterval(timer)
      timer = null
    }
  }
}

const idle = () => () => undefined

export function ticking(): boolean {
  return timer !== null
}

export function useMinutesSince(iso: string | null | undefined): number {
  const read = () => (iso ? minutesSince(iso) : 0)
  return useSyncExternalStore(iso ? subscribe : idle, read, read)
}
