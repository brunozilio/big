import { useLayoutEffect, useRef, useState } from 'react'
import { useStore } from '@/api/store'
import { AGENT_COLORS } from '@/ui/agentAvatar'
import { cn } from '@/ui/cn'
import { Input, MultiSelect, Select, Switch, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { type FieldValue, type LibraryField, problemOf, sizeOf, toggled } from './libraryData'

interface Props {
  fields: LibraryField[]
  values: Record<string, FieldValue>
  disabled: boolean
  onChange: (key: string, value: FieldValue) => void
}

const labelClass = 'text-sm font-medium text-muted'

const textOf = (value: FieldValue | undefined): string => (typeof value === 'string' ? value : '')
const listOf = (value: FieldValue | undefined): string[] => (Array.isArray(value) ? value : [])

function Swatches({ label, value, disabled, onChange }: { label: string; value: string; disabled: boolean; onChange: (color: string) => void }) {
  const colors = value && !AGENT_COLORS.includes(value) ? [...AGENT_COLORS, value] : AGENT_COLORS
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-2">
      {colors.map((color) => (
        <button
          key={color}
          type="button"
          role="radio"
          aria-checked={color === value}
          aria-label={color}
          disabled={disabled}
          onClick={() => onChange(color)}
          style={{ backgroundColor: color }}
          className={cn('size-6 rounded-full transition-shadow duration-150 disabled:opacity-45', color === value ? 'shadow-[0_0_0_2px_rgb(var(--surface)),0_0_0_4px_rgb(var(--text))]' : 'hover:shadow-[0_0_0_2px_rgb(var(--surface)),0_0_0_3px_rgb(var(--border-strong))]')}
        />
      ))}
    </div>
  )
}

function AutoArea({ label, value, disabled, invalid, onChange }: { label: string; value: string; disabled: boolean; invalid: boolean; onChange: (value: string) => void }) {
  const area = useRef<HTMLTextAreaElement>(null)
  useLayoutEffect(() => {
    const el = area.current
    if (!el) return
    el.style.height = 'auto'
    if (el.scrollHeight > 0) el.style.height = `${el.scrollHeight + 2}px`
  }, [value])
  return <Textarea ref={area} aria-label={label} aria-invalid={invalid || undefined} rows={2} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)} className="block resize-none overflow-hidden text-sm leading-relaxed" />
}

function useOptions(field: LibraryField) {
  const models = useStore((s) => s.models)
  const declared = (field.options ?? []).map((option) => ({ value: option, label: option === '' ? '—' : option }))
  if (field.from !== 'models' || !models) return declared
  const known = new Set(declared.map((option) => option.value))
  return [...declared, ...models.models.filter((model) => !known.has(model.id)).map((model) => ({ value: model.id, label: model.name }))]
}

function Control({ field, value, disabled, invalid, onChange }: { field: LibraryField; value: FieldValue | undefined; disabled: boolean; invalid: boolean; onChange: (value: FieldValue) => void }) {
  const options = useOptions(field)
  const locked = disabled || field.readOnly === true
  if (field.kind === 'area') return <AutoArea label={field.label} value={textOf(value)} disabled={locked} invalid={invalid} onChange={onChange} />
  if (field.kind === 'chips') return <MultiSelect label={field.label} placeholder="—" values={listOf(value)} options={options} onToggle={(option) => !locked && onChange(toggled(listOf(value), option))} className="w-full" />
  if (field.kind === 'select') return <Select label={field.label} placeholder="—" value={textOf(value)} options={options} onValueChange={(option) => !locked && onChange(option)} className="w-full" />
  if (field.kind === 'color') return <Swatches label={field.label} value={textOf(value)} disabled={locked} onChange={onChange} />
  return <Input aria-label={field.label} aria-invalid={invalid || undefined} value={textOf(value)} disabled={locked} onChange={(e) => onChange(e.target.value)} className={cn(locked && 'opacity-60')} />
}

function Note({ field, value }: { field: LibraryField; value: FieldValue | undefined }) {
  const t = useT()
  const problem = problemOf(field, value)
  const size = sizeOf(textOf(value))
  const counted = field.max !== undefined && (field.kind === 'area' || size > field.max * 0.8)
  if (!problem && !counted) return null
  const text = problem?.kind === 'required' ? t('settings.lib_required') : problem?.kind === 'shape' ? problem.hint : ''
  return (
    <p className={cn('flex justify-between gap-3 text-xs', problem ? 'text-danger-fg' : 'text-faint')}>
      <span role={text ? 'alert' : undefined}>{text}</span>
      {counted && <span className="tabular-nums">{`${size}/${field.max}`}</span>}
    </p>
  )
}

function Row({ field, value, disabled, onChange }: { field: LibraryField; value: FieldValue | undefined; disabled: boolean; onChange: (value: FieldValue) => void }) {
  if (field.kind === 'switch') {
    return (
      <div className="flex items-center justify-between gap-4">
        <span className={labelClass}>{field.label}</span>
        <Switch label={field.label} checked={value === true} disabled={disabled} onCheckedChange={onChange} />
      </div>
    )
  }
  return (
    <div className="space-y-1.5">
      <p className={labelClass}>{field.label}</p>
      <Control field={field} value={value} disabled={disabled} invalid={problemOf(field, value) !== null} onChange={onChange} />
      <Note field={field} value={value} />
    </div>
  )
}

export function LibraryFields({ fields, values, disabled, onChange }: Props) {
  const t = useT()
  const [more, setMore] = useState(false)
  const main = fields.filter((field) => !field.more)
  const extra = fields.filter((field) => field.more)
  const row = (field: LibraryField) => <Row key={field.key} field={field} value={values[field.key]} disabled={disabled} onChange={(value) => onChange(field.key, value)} />
  return (
    <div className="space-y-5">
      {main.map(row)}
      {extra.length > 0 && (
        <button type="button" aria-expanded={more} onClick={() => setMore(!more)} className="flex items-center gap-1.5 text-sm text-muted transition-colors hover:text-fg">
          <Icon name={more ? 'down' : 'right'} size={14} />
          {t('settings.lib_more')}
        </button>
      )}
      {more && extra.map(row)}
    </div>
  )
}
