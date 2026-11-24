import * as RCheckbox from '@radix-ui/react-checkbox'
import * as RSelect from '@radix-ui/react-select'
import { motion } from 'framer-motion'
import { cloneElement, forwardRef, isValidElement, useId, type InputHTMLAttributes, type MouseEvent, type ReactNode, type TextareaHTMLAttributes } from 'react'
import { cn } from './cn'
import { Kbd } from './display'
import { Icon } from './icon'
import { springSnap, tween } from './motion'
import { popIn } from './overlay'

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { leading?: ReactNode; trailing?: ReactNode }>(function Input({ className, leading, trailing, ...props }, ref) {
  return (
    <div className={cn('ring-field-within flex h-10 items-center gap-2 rounded-control bg-surface px-3 shadow-card transition-shadow', className)}>
      {leading && <span className="flex text-faint">{leading}</span>}
      <input ref={ref} className="h-full min-w-0 flex-1 bg-transparent text-base text-fg outline-none placeholder:text-muted" {...props} />
      {trailing}
    </div>
  )
})

export function SearchInput({ kbd, className, ...props }: InputHTMLAttributes<HTMLInputElement> & { kbd?: string }) {
  return <Input leading={<Icon name="search" size={16} />} trailing={kbd ? <Kbd>{kbd}</Kbd> : undefined} className={className} {...props} />
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea({ className, ...props }, ref) {
  return <textarea ref={ref} className={cn('ring-field w-full rounded-control bg-surface px-3 py-2.5 text-base text-fg shadow-card outline-none transition-shadow placeholder:text-muted', className)} {...props} />
})

export function keepClickOffButtons(e: MouseEvent<HTMLLabelElement>) {
  const control = e.currentTarget.control
  if (control instanceof HTMLButtonElement && !control.contains(e.target as Node)) e.preventDefault()
}

export function Field({ label, help, error, children }: { label: string; help?: ReactNode; error?: string; children: ReactNode }) {
  const noteId = useId()
  const note = error || help ? noteId : undefined
  const control = isValidElement<{ 'aria-describedby'?: string; 'aria-invalid'?: boolean }>(children)
    ? cloneElement(children, { 'aria-describedby': note, 'aria-invalid': error ? true : undefined })
    : children
  return (
    <label className="block space-y-2" onClick={keepClickOffButtons}>
      <span className="text-base font-medium text-fg">{label}</span>
      {control}
      {error ? <span id={noteId} className="flex items-center gap-1 text-xs text-danger-fg"><Icon name="alert" size={13} />{error}</span> : help && <span id={noteId} className="flex items-center gap-1.5 text-xs text-muted">{help}</span>}
    </label>
  )
}

export function Checkbox({ checked, onCheckedChange, label, disabled }: { checked: boolean; onCheckedChange: (v: boolean) => void; label?: string; disabled?: boolean }) {
  return (
    <RCheckbox.Root
      aria-label={label}
      checked={checked}
      disabled={disabled}
      onCheckedChange={(v) => onCheckedChange(v === true)}
      className="flex size-[18px] shrink-0 items-center justify-center rounded-[5px] bg-surface shadow-[0_0_0_1px_rgb(var(--border-strong))] transition-colors data-[state=checked]:bg-info data-[state=checked]:shadow-none disabled:opacity-45"
    >
      <RCheckbox.Indicator forceMount>
        <motion.svg viewBox="0 0 14 14" width="12" height="12" initial={false} animate={{ opacity: checked ? 1 : 0, scale: checked ? 1 : 0.5 }} transition={tween}>
          <motion.path d="M3 7.5l2.5 2.5L11 4" fill="none" stroke="white" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
        </motion.svg>
      </RCheckbox.Indicator>
    </RCheckbox.Root>
  )
}

export interface SelectOption {
  value: string
  label: ReactNode
  icon?: ReactNode
}

export function Select({ value, onValueChange, options, placeholder, className, size = 'md', label, leading }: { value: string; onValueChange: (v: string) => void; options: SelectOption[]; placeholder?: string; className?: string; size?: 'sm' | 'md'; label?: string; leading?: ReactNode }) {
  return (
    <RSelect.Root value={value} onValueChange={onValueChange}>
      <RSelect.Trigger aria-label={label} className={cn('inline-flex items-center justify-between gap-2 rounded-control bg-surface px-3 text-fg shadow-card transition-colors hover:bg-hover', size === 'sm' ? 'h-8 text-sm' : 'h-10 text-base', className)}>
        <span className="flex min-w-0 items-center gap-2 truncate">
          {leading}
          <RSelect.Value placeholder={placeholder} />
        </span>
        <Icon name="down" size={15} className="text-muted" />
      </RSelect.Trigger>
      <RSelect.Portal>
        <RSelect.Content position="popper" sideOffset={6} className={`z-50 max-h-80 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-card bg-surface p-1.5 shadow-pop origin-[var(--radix-select-content-transform-origin)] ${popIn}`}>
          <RSelect.Viewport>
            {options.map((o) => (
              <RSelect.Item key={o.value} value={o.value} className="flex h-9 cursor-pointer items-center gap-2.5 rounded-control-sm px-2.5 text-base text-fg2 outline-none data-[highlighted]:bg-hover data-[highlighted]:text-fg">
                {o.icon}
                <RSelect.ItemText>{o.label}</RSelect.ItemText>
                <RSelect.ItemIndicator className="ml-auto pl-3">
                  <Icon name="check" size={15} />
                </RSelect.ItemIndicator>
              </RSelect.Item>
            ))}
          </RSelect.Viewport>
        </RSelect.Content>
      </RSelect.Portal>
    </RSelect.Root>
  )
}

export function SegmentedControl<T extends string>({ value, onChange, options, className, size = 'md' }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; icon?: ReactNode; badge?: ReactNode }[]; className?: string; size?: 'sm' | 'md' }) {
  const id = useId()
  return (
    <div role="tablist" className={cn('inline-flex rounded-control bg-inset p-[3px]', className)}>
      {options.map((o) => {
        const active = o.value === value
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(o.value)}
            className={cn('relative inline-flex flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-control-sm px-3 transition-colors duration-150', size === 'sm' ? 'hit-44 h-7 text-sm' : 'h-8 text-base', active ? 'text-fg' : 'text-muted hover:text-fg2')}
          >
            {active && <motion.span layoutId={`seg-${id}`} transition={springSnap} className="absolute inset-0 rounded-control-sm bg-surface shadow-[0_1px_2px_rgb(0_0_0/0.06),0_0_0_1px_rgb(0_0_0/0.04)]" />}
            <span className="relative z-10 flex items-center gap-1.5">
              {o.icon}
              {o.label}
              {o.badge}
            </span>
          </button>
        )
      })}
    </div>
  )
}

export function UnderlineTabs<T extends string>({ value, onChange, options, className }: { value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; count?: number }[]; className?: string }) {
  const id = useId()
  return (
    <div className={cn('flex gap-6 border-b', className)}>
      {options.map((o) => (
        <button key={o.value} onClick={() => onChange(o.value)} className={cn('relative flex h-10 items-center gap-1.5 text-base transition-colors', o.value === value ? 'text-fg' : 'text-muted hover:text-fg2')}>
          {o.label}
          {o.count !== undefined && <span className="text-xs text-muted">{o.count}</span>}
          {o.value === value && <motion.span layoutId={`ul-${id}`} className="absolute inset-x-0 -bottom-px h-[2px] rounded-full bg-fg" transition={springSnap} />}
        </button>
      ))}
    </div>
  )
}
