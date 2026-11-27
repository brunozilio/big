import { useSyncExternalStore } from 'react'
import type { Me } from './auth'

let current: Me | null = null
const listeners = new Set<() => void>()

export function setMe(me: Me | null) {
  current = me
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function getMe(): Me | null {
  return current
}

export function useMe(): Me | null {
  return useSyncExternalStore(subscribe, getMe, getMe)
}

export function isShared(me: Me | null): boolean {
  return me?.mode === 'shared'
}

export function isAdmin(me: Me | null): boolean {
  return !isShared(me) || me?.admin === true
}

export function useShared(): boolean {
  return isShared(useMe())
}

export function useAdmin(): boolean {
  return isAdmin(useMe())
}
