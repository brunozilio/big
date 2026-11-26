import { useSyncExternalStore } from 'react'
import { cn } from './cn'
import { Icon } from './icon'
import { AnimatePresence, motion, tween, tweenClose } from './motion'

export type ToastTone = 'error' | 'info' | 'success'

export interface Toast {
  id: number
  tone: ToastTone
  text: string
}

const LIFE_MS: Record<ToastTone, number> = { error: 7000, info: 4000, success: 3000 }
const MAX = 4

let toasts: Toast[] = []
let next = 1
const listeners = new Set<() => void>()

function emit(list: Toast[]) {
  toasts = list
  listeners.forEach((l) => l())
}

export function dismissToast(id: number) {
  emit(toasts.filter((x) => x.id !== id))
}

export function showToast(tone: ToastTone, text: string): number {
  const id = next++
  emit([...toasts.filter((x) => x.text !== text), { id, tone, text }].slice(-MAX))
  setTimeout(() => dismissToast(id), LIFE_MS[tone])
  return id
}

export const toast = {
  error: (text: string) => showToast('error', text),
  info: (text: string) => showToast('info', text),
  success: (text: string) => showToast('success', text),
}

export function clearToasts() {
  emit([])
}

function subscribe(l: () => void) {
  listeners.add(l)
  return () => listeners.delete(l)
}

const TONE: Record<ToastTone, { icon: 'alert' | 'info' | 'check'; cls: string }> = {
  error: { icon: 'alert', cls: 'text-danger-fg' },
  info: { icon: 'info', cls: 'text-info-fg' },
  success: { icon: 'check', cls: 'text-success-fg' },
}

export function Toaster({ closeLabel }: { closeLabel: string }) {
  const list = useSyncExternalStore(subscribe, () => toasts, () => toasts)
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-[70] flex flex-col items-center gap-2 px-4">
      <AnimatePresence initial={false}>
        {list.map((x) => (
          <motion.div
            key={x.id}
            role={x.tone === 'error' ? 'alert' : 'status'}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, transition: tweenClose }}
            transition={tween}
            className="pointer-events-auto flex max-w-md items-start gap-2.5 rounded-card bg-surface px-3.5 py-2.5 text-base text-fg shadow-pop"
          >
            <Icon name={TONE[x.tone].icon} size={16} className={cn('mt-0.5 shrink-0', TONE[x.tone].cls)} />
            <span className="min-w-0 flex-1 break-words">{x.text}</span>
            <button type="button" aria-label={closeLabel} onClick={() => dismissToast(x.id)} className="-mr-1 flex size-6 shrink-0 items-center justify-center rounded-control-sm text-muted transition-colors hover:bg-hover hover:text-fg">
              <Icon name="close" size={14} />
            </button>
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  )
}
