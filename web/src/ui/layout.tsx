import { type ReactNode } from 'react'
import { cn } from './cn'

export const pageTitleClass = 'text-2xl font-medium tracking-tighter2 text-fg text-balance'
export const sectionTitleClass = 'text-lg font-medium text-fg'

export function SectionTitle({ children, actions, className }: { children: ReactNode; actions?: ReactNode; className?: string }) {
  if (!actions) return <h2 className={cn(sectionTitleClass, className)}>{children}</h2>
  return (
    <div className={cn('flex items-center justify-between gap-3', className)}>
      <h2 className={sectionTitleClass}>{children}</h2>
      {actions}
    </div>
  )
}

export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('overflow-hidden rounded-card bg-surface shadow-card', className)}>{children}</div>
}
