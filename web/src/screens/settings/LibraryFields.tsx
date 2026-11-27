import { useState } from 'react'
import { AGENT_COLORS } from '@/ui/agentAvatar'
import { cn } from '@/ui/cn'
import { Input, MultiSelect, Select, Switch, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { type FieldValue, type LibraryField, toggled } from './libraryData'

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

function Control({ field, value, disabled, onChange }: { field: LibraryField; value: FieldValue | undefined; disabled: boolean; onChange: (value: FieldValue) => void }) {
  const options = (field.options ?? []).map((option) => ({ value: option, label: option }))
  if (field.kind === 'area') return <Textarea aria-label={field.label} rows={4} value={textOf(value)} disabled={disabled} onChange={(e) => onChange(e.target.value)} className="resize-y text-sm leading-relaxed" />
  if (field.kind === 'chips') return <MultiSelect label={field.label} placeholder="—" values={listOf(value)} options={options} onToggle={(option) => !disabled && onChange(toggled(listOf(value), option))} className="w-full" />
  if (field.kind === 'select') return <Select label={field.label} placeholder="—" value={textOf(value)} options={options} onValueChange={(option) => !disabled && onChange(option)} className="w-full" />
  if (field.kind === 'color') return <Swatches label={field.label} value={textOf(value)} disabled={disabled} onChange={onChange} />
  return <Input aria-label={field.label} value={textOf(value)} disabled={disabled} onChange={(e) => onChange(e.target.value)} />
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
      <Control field={field} value={value} disabled={disabled} onChange={onChange} />
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
