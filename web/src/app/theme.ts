import { at } from '@/api/base'
import { useEffect } from 'react'
import { useStore } from '@/api/store'
import type { Theme } from '@/api/contract'

const dark = () => globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false

export function applyTheme(theme: Theme) {
  if (typeof document === 'undefined') return
  const isDark = theme === 'dark' || (theme === 'system' && dark())
  document.documentElement.dataset.theme = isDark ? 'dark' : 'light'
  document.querySelectorAll('meta[name="theme-color"]').forEach((m) => m.setAttribute('content', isDark ? '#161617' : '#ffffff'))
  try {
    localStorage.setItem('theme', theme)
  } catch {
    return
  }
}

export function useThemeSync() {
  const theme = useStore((s) => s.branding?.theme)
  useEffect(() => {
    if (!theme) return
    applyTheme(theme)
    const media = globalThis.matchMedia?.('(prefers-color-scheme: dark)')
    const onChange = () => applyTheme(theme)
    media?.addEventListener('change', onChange)
    return () => media?.removeEventListener('change', onChange)
  }, [theme])
}

export function useBrandTitle() {
  const branding = useStore((s) => s.branding)
  useEffect(() => {
    if (!branding) return
    document.title = branding.name
    const href = branding.favicon ? at(branding.favicon) : null
    let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
    if (!href) {
      link?.remove()
      return
    }
    if (!link) {
      link = document.createElement('link')
      link.rel = 'icon'
      document.head.appendChild(link)
    }
    link.href = href
  }, [branding])
}
