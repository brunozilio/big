import { useEffect } from 'react'
import type { Language } from '@/api/contract'
import { useLanguage } from '.'

export function applyLanguage(language: Language) {
  if (typeof document === 'undefined') return
  document.documentElement.lang = language
  try {
    localStorage.setItem('language', language)
  } catch {
    return
  }
}

export function useLanguageSync() {
  const language = useLanguage()
  useEffect(() => applyLanguage(language), [language])
}
