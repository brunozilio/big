import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react'

export type Span = { start: number; end: number; before: number; after: number }

type View = { top: number; height: number }

type Options = { estimate: number; overscan?: number; enabled?: boolean }

const OVERSCAN = 6
const KEY = 'virtualKey'

const total = (sizes: number[], from: number, to: number) => {
  let sum = 0
  for (let at = from; at < to; at++) sum += sizes[at]
  return sum
}

export function spanOf(sizes: number[], top: number, height: number, overscan = OVERSCAN): Span {
  let offset = 0
  let start = 0
  while (start < sizes.length && offset + sizes[start] <= top) offset += sizes[start++]
  let end = start
  while (end < sizes.length && offset < top + height) offset += sizes[end++]
  const from = Math.max(0, start - overscan)
  const to = Math.min(sizes.length, Math.max(end, from + 1) + overscan)
  return { start: from, end: to, before: total(sizes, 0, from), after: total(sizes, to, sizes.length) }
}

export function scrollerOf(start: Element | null): HTMLElement | null {
  for (let node = start; node && node !== document.body; node = node.parentElement) {
    const { overflowY } = getComputedStyle(node)
    if (node instanceof HTMLElement && (overflowY === 'auto' || overflowY === 'scroll')) return node
  }
  return null
}

const windowHeight = () => (typeof window === 'undefined' ? 800 : window.innerHeight)

function useViewport<T extends HTMLElement>(list: RefObject<T>, enabled: boolean) {
  const [view, setView] = useState<View>(() => ({ top: 0, height: windowHeight() }))
  const read = useRef<() => void>(() => undefined)
  useLayoutEffect(() => {
    const el = list.current
    if (!enabled || !el) return
    const scroller = scrollerOf(el.parentElement)
    let frame = 0
    const now = () => {
      frame = 0
      const top = (scroller ? scroller.getBoundingClientRect().top : 0) - el.getBoundingClientRect().top
      const height = scroller ? scroller.clientHeight || windowHeight() : windowHeight()
      setView((prev) => (prev.top === top && prev.height === height ? prev : { top, height }))
    }
    const later = () => {
      if (!frame) frame = requestAnimationFrame(now)
    }
    read.current = later
    now()
    const target: HTMLElement | Window = scroller ?? window
    target.addEventListener('scroll', later, { passive: true })
    window.addEventListener('resize', later)
    const content = scroller?.firstElementChild
    const observer = content && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(later) : null
    if (content) observer?.observe(content)
    return () => {
      target.removeEventListener('scroll', later)
      window.removeEventListener('resize', later)
      observer?.disconnect()
      cancelAnimationFrame(frame)
      read.current = () => undefined
    }
  }, [list, enabled])
  return { view, reread: () => read.current() }
}

function useSizes(enabled: boolean) {
  const sizes = useRef(new Map<string, number>())
  const [version, setVersion] = useState(0)
  const keep = useCallback((node: Element) => {
    const key = node instanceof HTMLElement ? node.dataset[KEY] : undefined
    const height = node.getBoundingClientRect().height
    if (!key || height <= 0 || sizes.current.get(key) === height) return
    sizes.current.set(key, height)
    setVersion((v) => v + 1)
  }, [])
  const observer = useMemo(
    () => (enabled && typeof ResizeObserver !== 'undefined' ? new ResizeObserver((entries) => entries.forEach((entry) => keep(entry.target))) : null),
    [enabled, keep],
  )
  useEffect(() => () => observer?.disconnect(), [observer])
  const measure = useCallback(
    (node: HTMLElement | null) => {
      if (!node) return
      keep(node)
      observer?.observe(node)
    },
    [keep, observer],
  )
  return { sizes, version, measure }
}

export function useVirtualRows<T extends HTMLElement>(keys: string[], { estimate, overscan = OVERSCAN, enabled = true }: Options) {
  const list = useRef<T>(null)
  const { view, reread } = useViewport(list, enabled)
  const { sizes, version, measure } = useSizes(enabled)
  const span = useMemo(() => {
    if (!enabled) return { start: 0, end: keys.length, before: 0, after: 0 }
    const known = keys.map((key) => sizes.current.get(key) ?? estimate)
    return spanOf(known, view.top, view.height, overscan)
  }, [enabled, keys, estimate, overscan, view, version, sizes])
  useLayoutEffect(reread)
  return { list, measure, span, keyProp: (key: string) => ({ 'data-virtual-key': key }) }
}
