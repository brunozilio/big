import { useCallback } from 'react'
import type { Language } from '@/api/contract'
import { getState, useStore } from '@/api/store'
import { MESSAGES, type MessageKey } from './messages'

export type { Language } from '@/api/contract'
export type { MessageKey } from './messages'

export type Vars = Readonly<Record<string, string | number>>
export type T = (key: MessageKey, vars?: Vars) => string

export const DEFAULT_LANGUAGE: Language = 'pt-BR'

export const LANGUAGES: readonly { value: Language; label: string }[] = [
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
  { value: 'pt-BR', label: 'Português' },
]

const LOCALES: Readonly<Record<Language, string>> = { en: 'en-US', es: 'es-ES', 'pt-BR': 'pt-BR' }

export function isLanguage(value: unknown): value is Language {
  return value === 'en' || value === 'es' || value === 'pt-BR'
}

export function languageOf(tag: string | null | undefined): Language | null {
  if (!tag) return null
  if (isLanguage(tag)) return tag
  const base = tag.toLowerCase().split('-')[0]
  return base === 'pt' ? 'pt-BR' : isLanguage(base) ? base : null
}

function remembered(): Language | null {
  try {
    return languageOf(localStorage.getItem('language'))
  } catch {
    return null
  }
}

export function initialLanguage(): Language {
  return remembered() ?? languageOf(globalThis.navigator?.language) ?? DEFAULT_LANGUAGE
}

export function currentLanguage(): Language {
  const now = getState()
  return now.branding?.language ?? now.language ?? initialLanguage()
}

export function translate(language: Language, key: MessageKey, vars?: Vars): string {
  const text = MESSAGES[language][key] ?? MESSAGES[DEFAULT_LANGUAGE][key] ?? key
  return vars ? interpolate(text, vars) : text
}

export function interpolate(text: string, vars: Vars): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole))
}

export function localeOf(language: Language): string {
  return LOCALES[language]
}

export function currentLocale(): string {
  return localeOf(currentLanguage())
}

export function useLanguage(): Language {
  return useStore((s) => s.branding?.language ?? s.language) ?? initialLanguage()
}

export function useT(): T {
  const language = useLanguage()
  return useCallback<T>((key, vars) => translate(language, key, vars), [language])
}

export function useLocale(): string {
  return localeOf(useLanguage())
}

export function plural(count: number): 'one' | 'other' {
  return Math.abs(count) === 1 ? 'one' : 'other'
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions, locale: string = currentLocale()): string {
  return new Intl.NumberFormat(locale, options).format(value)
}

export function formatDate(value: Date | string | number, options?: Intl.DateTimeFormatOptions, locale: string = currentLocale()): string {
  return new Date(value).toLocaleDateString(locale, options)
}
