import { forwardRef, useEffect, useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { cn } from './cn'
import { Icon } from './icon'
import { Tooltip } from './overlay'

type ToolButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> & { label: string; active?: boolean; caret?: boolean; children: ReactNode }

export const ToolButton = forwardRef<HTMLButtonElement, ToolButtonProps>(function ToolButton({ label, active, caret, className, children, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      onMouseDown={(e) => e.preventDefault()}
      className={cn(
        'hit-44 relative flex h-8 shrink-0 items-center justify-center gap-0.5 rounded-control-sm text-muted transition-colors duration-150 hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-35 data-[state=open]:bg-active data-[state=open]:text-fg',
        caret ? 'pl-1.5 pr-1' : 'w-8',
        active && 'bg-active text-fg',
        className,
      )}
      {...rest}
    >
      {children}
      {caret && <Icon name="down" size={12} className="text-faint" />}
    </button>
  )
})

export function tipText(label: string, shortcut?: string): ReactNode {
  return shortcut ? (
    <span className="flex items-center gap-2">
      {label}
      <span className="opacity-60">{shortcut}</span>
    </span>
  ) : (
    label
  )
}

export function Tool({ label, shortcut, ...props }: ToolButtonProps & { shortcut?: string }) {
  return (
    <Tooltip content={tipText(label, shortcut)}>
      <ToolButton label={label} {...props} />
    </Tooltip>
  )
}

export function ToolSep() {
  return <span aria-hidden className="mx-1 h-5 w-px shrink-0 bg-line" />
}

export function Toolbar({ label, children, end, className }: { label: string; children: ReactNode; end?: ReactNode; className?: string }) {
  return (
    <div role="toolbar" aria-label={label} className={cn('flex min-h-11 items-center border-b px-1.5 py-1.5', className)}>
      <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto [scrollbar-width:none]">{children}</div>
      {end && <div className="flex shrink-0 items-center gap-0.5 border-l pl-1.5 ml-1">{end}</div>}
    </div>
  )
}

export function useNarrow(query = '(max-width: 767px)') {
  const [narrow, setNarrow] = useState(() => typeof window !== 'undefined' && !!window.matchMedia?.(query).matches)
  useEffect(() => {
    const m = window.matchMedia?.(query)
    if (!m) return
    const update = () => setNarrow(m.matches)
    update()
    m.addEventListener('change', update)
    return () => m.removeEventListener('change', update)
  }, [query])
  return narrow
}
