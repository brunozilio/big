import { AnimatePresence, animate, motion, useMotionValue, useTransform } from 'framer-motion'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { currentLocale } from '@/i18n'
import { cn } from './cn'

export { AnimatePresence, motion }

export const dur = { stagger: 0.04, micro: 0.08, quick: 0.15, fast: 0.25, medium: 0.35, slow: 0.4 }
export const easeOut = [0.22, 1, 0.36, 1] as const
export const tween = { duration: dur.fast, ease: easeOut }
export const tweenClose = { duration: dur.quick, ease: easeOut }
export const springSnap = { type: 'spring', duration: 0.25, bounce: 0 } as const
export const springPop = { type: 'spring', duration: 0.35, bounce: 0.2 } as const
export const stagger = (i: number) => Math.min(i, 6) * dur.stagger

export function Collapse({ open, children, className }: { open: boolean; children: ReactNode; className?: string }) {
  const [mounted, setMounted] = useState(open)
  const [expanded, setExpanded] = useState(open)
  const [settled, setSettled] = useState(open)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (open) {
      setMounted(true)
      box.current?.getBoundingClientRect()
      setExpanded(true)
      const id = setTimeout(() => setSettled(true), dur.fast * 1000)
      return () => clearTimeout(id)
    }
    setSettled(false)
    setExpanded(false)
    const id = setTimeout(() => setMounted(false), dur.fast * 1000)
    return () => clearTimeout(id)
  }, [open])
  if (!open && !mounted) return null
  return (
    <div ref={box} className="fold" data-open={open && expanded} data-settled={open && settled}>
      <div className={className}>{children}</div>
    </div>
  )
}

export function Counter({ value, format = (v: number) => Math.round(v).toLocaleString(currentLocale()), className }: { value: number; format?: (v: number) => string; className?: string }) {
  const mv = useMotionValue(value)
  const text = useTransform(mv, (v) => format(v))
  const shown = useRef(value)
  useEffect(() => {
    if (shown.current === value) return
    shown.current = value
    const controls = animate(mv, value, tween)
    return () => controls.stop()
  }, [mv, value])
  return <motion.span className={cn('tabular-nums', className)}>{text}</motion.span>
}

const iconHidden = { opacity: 0, scale: 0.25 }
const iconShown = { opacity: 1, scale: 1 }

export function IconSwap({ k, children, className }: { k: string | number; children: ReactNode; className?: string }) {
  return (
    <span className={cn('relative inline-flex items-center justify-center', className)}>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={k} initial={iconHidden} animate={iconShown} exit={iconHidden} transition={{ duration: dur.fast, ease: 'easeInOut' }} className="inline-flex items-center justify-center">
          {children}
        </motion.span>
      </AnimatePresence>
    </span>
  )
}

export function FadeIn({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('animate-in fade-in-0 duration-150 ease-smooth-out', className)}>{children}</div>
}
