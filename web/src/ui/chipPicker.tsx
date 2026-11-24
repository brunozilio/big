import { type KeyboardEvent, type ReactNode, useState } from 'react'
import { cn } from './cn'
import { Kbd } from './display'
import { Icon } from './icon'
import { Popover } from './overlay'

export interface ChipOption<T extends string> {
  value: T
  label: string
  icon: ReactNode
}

function Choice<T extends string>({ option, at, chosen, lit, onPick, onHover }: { option: ChipOption<T>; at: number; chosen: boolean; lit: boolean; onPick: () => void; onHover: () => void }) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={chosen}
      onClick={onPick}
      onMouseMove={onHover}
      className={cn('flex h-9 w-full items-center gap-2.5 rounded-control-sm px-2.5 text-left text-base text-fg2 outline-none transition-colors', lit && 'bg-hover text-fg')}
    >
      <span className="flex w-4 justify-center">{option.icon}</span>
      <span className="flex-1 truncate">{option.label}</span>
      {chosen && <Icon name="check" size={15} className="text-fg2" />}
      <span className="w-4 text-right text-sm tabular-nums text-muted">{at + 1}</span>
    </button>
  )
}

export function ChipPicker<T extends string>({ value, options, onChange, label, placeholder, className }: { value: T; options: ChipOption<T>[]; onChange: (value: T) => void; label: string; placeholder: string; className?: string }) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [lit, setLit] = useState(0)
  const current = options.find((option) => option.value === value) ?? options[0]
  const shown = options.filter((option) => option.label.toLowerCase().includes(query.trim().toLowerCase()))
  const toggle = (next: boolean) => {
    setOpen(next)
    setQuery('')
    setLit(Math.max(0, options.indexOf(current)))
  }
  const pick = (option: ChipOption<T> | undefined) => {
    if (!option) return
    toggle(false)
    if (option.value !== value) onChange(option.value)
  }
  const keyed = (e: KeyboardEvent<HTMLInputElement>) => {
    const digit = Number(e.key)
    if (Number.isInteger(digit) && digit >= 1 && digit <= options.length) {
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
  const trigger = (
    <button
      type="button"
      aria-label={label}
      className={cn('inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full bg-inset px-2.5 text-sm font-medium text-fg2 shadow-[inset_0_0_0_1px_rgb(var(--border))] transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg', className)}
    >
      {current.icon}
      <span>{current.label}</span>
    </button>
  )
  return (
    <Popover trigger={trigger} open={open} onOpenChange={toggle} align="start" className="w-64 p-0">
      <div className="flex items-center gap-2 border-b px-3 py-2.5">
        <input autoFocus aria-label={placeholder} placeholder={placeholder} value={query} onChange={(e) => { setQuery(e.target.value); setLit(0) }} onKeyDown={keyed} className="min-w-0 flex-1 bg-transparent text-base text-fg outline-none placeholder:text-muted" />
        <Kbd>1–{options.length}</Kbd>
      </div>
      <div role="menu" aria-label={label} className="p-1.5">
        {shown.map((option) => {
          const at = options.indexOf(option)
          return <Choice key={option.value} option={option} at={at} chosen={option.value === value} lit={shown[lit] === option} onPick={() => pick(option)} onHover={() => setLit(shown.indexOf(option))} />
        })}
      </div>
    </Popover>
  )
}
