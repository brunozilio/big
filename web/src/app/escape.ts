import { useEffect, useRef } from 'react'

const OVERLAYS = '[role="dialog"], [role="menu"], [data-radix-popper-content-wrapper]'

export function typing(el: Element | null): boolean {
  if (!el) return false
  if (el instanceof HTMLElement && el.isContentEditable) return true
  return !!el.closest('input, textarea, select, [contenteditable="true"], [contenteditable=""], [role="textbox"], [role="combobox"]')
}

export function escapeGoesBack(e: { key: string; defaultPrevented: boolean }, doc: Document): boolean {
  if (e.key !== 'Escape' || e.defaultPrevented) return false
  if (doc.querySelector(OVERLAYS)) return false
  return !typing(doc.activeElement)
}

export function useEscapeBack(active: boolean, back: () => void) {
  const latest = useRef(back)
  latest.current = back
  useEffect(() => {
    if (!active) return
    const onKey = (e: KeyboardEvent) => {
      if (escapeGoesBack(e, document)) latest.current()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [active])
}

export function useHotkey(key: string, run: () => void) {
  const latest = useRef(run)
  latest.current = run
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.altKey || e.shiftKey || !(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== key) return
      e.preventDefault()
      latest.current()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [key])
}

export function hotkeyLabel(key: string): string {
  const mac = typeof navigator !== 'undefined' && /mac|iphone|ipad/i.test(navigator.platform ?? '')
  return mac ? `⌘${key.toUpperCase()}` : `Ctrl ${key.toUpperCase()}`
}
