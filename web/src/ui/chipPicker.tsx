import { type ButtonHTMLAttributes, forwardRef, type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from 'react'
import { cn } from './cn'
import { Kbd } from './display'
import { Icon } from './icon'
import { Popover } from './overlay'

export interface ChipOption<T extends string = string> {
  value: T
  label: ReactNode
  icon?: ReactNode
  text?: string
}

export type PickerLook = 'chip' | 'field'

const SHORTCUTS = 9

function textOf(option: ChipOption<string>): string {
  if (option.text) return option.text
  return typeof option.label === 'string' || typeof option.label === 'number' ? String(option.label) : option.value
}

export function matching<T extends string>(options: ChipOption<T>[], query: string): ChipOption<T>[] {
  const wanted = query.trim().toLowerCase()
  return wanted ? options.filter((option) => textOf(option).toLowerCase().includes(wanted)) : options
}

function Choice<T extends string>({ option, at, chosen, lit, numbered, onPick, onHover }: { option: ChipOption<T>; at: number; chosen: boolean; lit: boolean; numbered: boolean; onPick: () => void; onHover: () => void }) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (lit) ref.current?.scrollIntoView?.({ block: 'nearest' })
  }, [lit])
  return (
    <div
      ref={ref}
      role="option"
      tabIndex={-1}
      aria-selected={chosen}
      onClick={onPick}
      onKeyDown={(e) => e.key === 'Enter' && onPick()}
      onMouseMove={onHover}
      className={cn('flex h-9 w-full shrink-0 cursor-pointer items-center gap-2.5 rounded-control-sm px-2.5 text-left text-base text-fg2 outline-none transition-colors', lit && 'bg-hover text-fg')}
    >
      {option.icon && <span aria-hidden className="flex w-4 shrink-0 justify-center">{option.icon}</span>}
      <span className="flex-1 truncate">{option.label}</span>
      {chosen && <Icon name="check" size={15} className="shrink-0 text-fg2" />}
      {numbered && <span aria-hidden className="w-4 shrink-0 text-right text-sm tabular-nums text-muted">{at + 1}</span>}
    </div>
  )
}

type TriggerProps = ButtonHTMLAttributes<HTMLButtonElement> & { look: PickerLook; current?: ChipOption<string>; label: string; leading?: ReactNode; empty?: string; size: 'sm' | 'md'; onOpen: () => void }

const Trigger = forwardRef<HTMLButtonElement, TriggerProps>(function Trigger({ look, current, label, leading, empty, className, size, onOpen, onKeyDown, ...props }, ref) {
  const shown = current ? current.label : <span className="text-muted">{empty}</span>
  const keyed = (e: KeyboardEvent<HTMLButtonElement>) => {
    onKeyDown?.(e)
    if (['Enter', ' ', 'ArrowDown', 'ArrowUp'].includes(e.key)) {
      e.preventDefault()
      onOpen()
    }
  }
  const shared = { ...props, ref, type: 'button' as const, role: 'combobox', 'aria-label': label, onKeyDown: keyed }
  if (look === 'chip') {
    return (
      <button {...shared} className={cn('inline-flex h-7 max-w-full shrink-0 items-center gap-1.5 rounded-full bg-inset px-2.5 text-sm font-medium text-fg2 shadow-[inset_0_0_0_1px_rgb(var(--border))] outline-none transition-colors hover:bg-hover hover:text-fg focus-visible:bg-hover focus-visible:text-fg focus-visible:outline-none data-[state=open]:bg-hover data-[state=open]:text-fg', className)}>
        {current?.icon ?? leading}
        <span className="truncate">{shown}</span>
      </button>
    )
  }
  return (
    <button {...shared} className={cn('inline-flex items-center justify-between gap-2 rounded-control bg-surface px-3 text-fg shadow-card outline-none transition-colors hover:bg-hover focus-visible:bg-hover focus-visible:outline-none data-[state=open]:bg-hover', size === 'sm' ? 'h-8 text-sm' : 'h-10 text-base', className)}>
      <span className="flex min-w-0 items-center gap-2 truncate">
        {current?.icon ?? leading}
        <span className="truncate">{shown}</span>
      </span>
      <Icon name="down" size={15} className="shrink-0 text-muted" />
    </button>
  )
})

export interface PickerProps<T extends string> {
  value: T
  options: ChipOption<T>[]
  onChange: (value: T) => void
  label: string
  placeholder?: string
  empty?: string
  look?: PickerLook
  size?: 'sm' | 'md'
  leading?: ReactNode
  className?: string
}

export function ChipPicker<T extends string>({ value, options, onChange, label, placeholder, empty, look = 'chip', size = 'sm', leading, className }: PickerProps<T>) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [lit, setLit] = useState(0)
  const current = options.find((option) => option.value === value)
  const shown = matching(options, query)
  const numbered = options.length <= SHORTCUTS
  const toggle = (next: boolean) => {
    setOpen(next)
    setQuery('')
    setLit(Math.max(0, current ? options.indexOf(current) : 0))
  }
  const pick = (option: ChipOption<T> | undefined) => {
    if (!option) return
    toggle(false)
    if (option.value !== value) onChange(option.value)
  }
  const keyed = (e: KeyboardEvent<HTMLInputElement>) => {
    const digit = Number(e.key)
    if (numbered && !query && Number.isInteger(digit) && digit >= 1 && digit <= options.length) {
      e.preventDefault()
      return pick(options[digit - 1])
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      return setLit((at) => (at + step + shown.length) % Math.max(shown.length, 1))
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      pick(shown[lit])
    }
  }
  const searchLabel = placeholder ?? `${label}…`
  return (
    <Popover trigger={<Trigger look={look} current={current} label={label} leading={leading} empty={empty} className={className} size={size} onOpen={() => toggle(true)} />} open={open} onOpenChange={toggle} align="start" className="w-[max(16rem,var(--radix-popover-trigger-width))] p-0">
      <div className="flex items-center gap-2 border-b px-3 py-2.5">
        <input autoFocus aria-label={searchLabel} placeholder={searchLabel} value={query} onChange={(e) => { setQuery(e.target.value); setLit(0) }} onKeyDown={keyed} className="min-w-0 flex-1 bg-transparent text-base text-fg outline-none placeholder:text-muted" />
        {numbered && <Kbd>1–{options.length}</Kbd>}
      </div>
      <div role="listbox" aria-label={label} className="flex max-h-80 flex-col overflow-y-auto p-1.5">
        {shown.map((option) => (
          <Choice key={option.value} option={option} at={options.indexOf(option)} numbered={numbered} chosen={option.value === value} lit={shown[lit] === option} onPick={() => pick(option)} onHover={() => setLit(shown.indexOf(option))} />
        ))}
      </div>
    </Popover>
  )
}
