import { AnimatePresence, animate, motion, useMotionValue, useTransform, type HTMLMotionProps } from 'framer-motion'
import { useEffect, useRef, type ReactNode } from 'react'
import { cn } from './cn'

export { AnimatePresence, motion }

export const dur = { stagger: 0.04, micro: 0.08, quick: 0.15, fast: 0.25, medium: 0.35, slow: 0.4 }
export const easeOut = [0.22, 1, 0.36, 1] as const
export const tween = { duration: dur.fast, ease: easeOut }
export const tweenClose = { duration: dur.quick, ease: easeOut }
export const springSnap = { type: 'spring', duration: 0.25, bounce: 0 } as const
export const springPop = { type: 'spring', duration: 0.35, bounce: 0.2 } as const
export const springSheet = { type: 'spring', duration: 0.4, bounce: 0 } as const
export const stagger = (i: number) => Math.min(i, 6) * dur.stagger

export function Page({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: dur.quick, ease: easeOut }} className={className}>
      {children}
    </motion.div>
  )
}

export function Stagger({ children, className, delay = 0, step = dur.stagger }: { children: ReactNode; className?: string; delay?: number; step?: number }) {
  return (
    <motion.div className={className} initial="hidden" animate="show" variants={{ hidden: {}, show: { transition: { staggerChildren: step, delayChildren: delay } } }}>
      {children}
    </motion.div>
  )
}

export const itemVariants = {
  hidden: { opacity: 0, y: 8, filter: 'blur(2px)' },
  show: { opacity: 1, y: 0, filter: 'blur(0px)', transition: tween },
}

export function StaggerItem({ children, className, ...props }: HTMLMotionProps<'div'>) {
  return (
    <motion.div variants={itemVariants} className={className} {...props}>
      {children}
    </motion.div>
  )
}

export function FadeIn({ children, className, delay = 0, y = 8 }: { children: ReactNode; className?: string; delay?: number; y?: number }) {
  return (
    <motion.div initial={{ opacity: 0, y }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay }} className={className}>
      {children}
    </motion.div>
  )
}

export const collapseMotion = {
  initial: { height: 0, opacity: 0 },
  animate: { height: 'auto', opacity: 1 },
  exit: { height: 0, opacity: 0 },
  transition: tween,
}

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

export function Pop({ children, className, k }: { children: ReactNode; className?: string; k?: string | number }) {
  return (
    <motion.span key={k} initial={{ scale: 0.9, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} transition={springPop} className={cn('inline-flex', className)}>
      {children}
    </motion.span>
  )
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
