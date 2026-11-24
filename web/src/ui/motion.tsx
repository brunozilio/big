import { AnimatePresence, animate, motion, useMotionValue, useTransform } from 'framer-motion'
import { useEffect, useRef, type ReactNode } from 'react'
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
  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div initial={{ height: 0, opacity: 0, overflow: 'hidden' }} animate={{ height: 'auto', opacity: 1, transitionEnd: { overflow: 'visible' } }} exit={{ height: 0, opacity: 0, overflow: 'hidden' }} transition={tween} className={className}>
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  )
}

export function Counter({ value, format = (v: number) => Math.round(v).toLocaleString('pt-BR'), className }: { value: number; format?: (v: number) => string; className?: string }) {
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

const iconHidden = { opacity: 0, scale: 0.25, filter: 'blur(2px)' }
const iconShown = { opacity: 1, scale: 1, filter: 'blur(0px)' }

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
