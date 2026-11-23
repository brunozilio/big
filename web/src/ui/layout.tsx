import { Fragment, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { cn } from './cn'
import { Icon, type IconName } from './icon'

export interface Crumb {
  label: ReactNode
  to?: string
  icon?: IconName | ReactNode
}

export function PageHeader({ crumbs, actions, className }: { crumbs: Crumb[]; actions?: ReactNode; className?: string }) {
  return (
    <header className={cn('sticky top-0 z-20 flex h-[52px] shrink-0 items-center gap-3 border-b bg-bg/85 px-5 backdrop-blur-md', className)}>
      <nav className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden text-md [&>a]:min-w-0">
        {crumbs.map((c, i) => {
          const last = i === crumbs.length - 1
          const icon = typeof c.icon === 'string' ? <Icon name={c.icon as IconName} size={18} className="text-muted" /> : c.icon
          const inner = (
            <span className={cn('flex min-w-0 items-center gap-2', last ? 'text-fg' : 'text-muted transition-colors hover:text-fg')}>
              {icon}
              <span className="truncate" title={typeof c.label === 'string' ? c.label : undefined}>{c.label}</span>
            </span>
          )
          return (
            <Fragment key={i}>
              {i > 0 && <span className="text-faint">/</span>}
              {c.to && !last ? <Link to={c.to}>{inner}</Link> : inner}
            </Fragment>
          )
        })}
      </nav>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  )
}

export function PageBody({ children, className, width = 'max-w-[1000px]' }: { children: ReactNode; className?: string; width?: string }) {
  return (
    <div className={cn('mx-auto w-full px-5 py-10 md:px-10', width, className)}>
      {children}
    </div>
  )
}

export const pageTitleClass = 'text-2xl font-medium tracking-tighter2 text-fg text-balance'
export const sectionTitleClass = 'text-lg font-medium text-fg'

export function PageTitle({ title, subtitle, actions, icon, className }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode; icon?: ReactNode; className?: string }) {
  return (
    <div className={cn('mb-8 flex flex-wrap items-start justify-between gap-4', className)}>
      <div className="min-w-0">
        {icon && <div className="mb-4">{icon}</div>}
        <h1 className={pageTitleClass}>{title}</h1>
        {subtitle && <p className="mt-1.5 text-md text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  )
}

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
