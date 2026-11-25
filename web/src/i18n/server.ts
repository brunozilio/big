import type { Said } from '@/api/contract'
import { MESSAGES } from './messages'
import type { MessageKey, T } from '.'

function known(key: string): key is MessageKey {
  return key in MESSAGES['pt-BR']
}

export function spoken(t: T, said: Said | null | undefined, fallback: string): string {
  if (!said) return fallback
  const key = `server.${said.key}`
  return known(key) ? t(key, said.vars) : fallback
}

export function groupName(t: T, group: string): string {
  const key = `server.group_${group}`
  return known(key) ? t(key) : group
}

export function styleName(t: T, style: { key: string; name: string }): string {
  const key = `server.style_${style.key}`
  return known(key) ? t(key) : style.name
}

export function styleDescription(t: T, style: { key: string; description: string }, name: string): string {
  const key = `server.style_${style.key}_description`
  return known(key) ? t(key, { name }) : style.description
}

export function titleOf(t: T, item: { title: string; untitled?: boolean }): string {
  return item.untitled ? t('server.untitled') : item.title
}
