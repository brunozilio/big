import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api'
import type { Binding, BindingChoice, BindingEach, BindingFieldSpec, BindingOption, Bindings, BindingScope, Connection, Provider } from '@/api/contract'
import { useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Card } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Field, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'

export interface Source {
  connection: Connection
  provider: Provider
}

export interface BindingField {
  capability: string
  label: string
  sources: Source[]
  multiple?: boolean
  shape?: BindingShape
}

export interface BindingShape {
  fields: BindingFieldSpec[]
  each: BindingEach | null
}

export type BindingOptions = Record<string, Record<string, BindingOption[]>>

const NONE = 'none'

export function choiceKey(connection: string, key: string): string {
  return `${connection}::${key}`
}

export function fromKey(value: string): { connection: string; key: string } | null {
  if (value === NONE) return null
  const at = value.indexOf('::')
  return at < 0 ? null : { connection: value.slice(0, at), key: value.slice(at + 2) }
}

export function fieldsOf(connections: Connection[], providers: Provider[], scope: BindingScope): BindingField[] {
  const fields: BindingField[] = []
  for (const connection of connections) {
    const provider = providers.find((p) => p.key === connection.provider)
    for (const spec of provider?.bindings ?? []) {
      if (spec.scope !== scope || !connection.capabilities.includes(spec.capability)) continue
      const known = fields.find((f) => f.capability === spec.capability)
      const shape = spec.fields?.length || spec.each ? { fields: spec.fields ?? [], each: spec.each ?? null } : undefined
      if (!known) fields.push({ capability: spec.capability, label: spec.label, sources: [{ connection, provider: provider as Provider }], multiple: spec.multiple === true, ...(shape ? { shape } : {}) })
      else {
        if (!known.shape && shape) known.shape = shape
        known.sources.push({ connection, provider: provider as Provider })
        known.multiple = known.multiple || spec.multiple === true
        if (!known.label.split(' / ').includes(spec.label)) known.label = `${known.label} / ${spec.label}`
      }
    }
  }
  return fields
}

export function useBindingFields(scope: BindingScope): BindingField[] {
  const connections = useStore((s) => s.connections)
  return useMemo(() => fieldsOf(connections?.connections ?? [], connections?.providers ?? [], scope), [connections, scope])
}

export function useBindingOptions(fields: BindingField[], open: boolean): BindingOptions {
  const [listed, setListed] = useState<BindingOptions>({})
  useEffect(() => {
    if (!open) return
    let alive = true
    const keep = (capability: string, connection: string, found: BindingOption[]) =>
      alive && setListed((known) => ({ ...known, [capability]: { ...known[capability], [connection]: found } }))
    for (const field of fields) {
      if (field.shape) continue
      for (const s of field.sources) {
        const id = s.connection.id
        api()
          .call(id, `${field.capability}.options`, {})
          .then((found) => keep(field.capability, id, found))
          .catch(() => keep(field.capability, id, []))
      }
    }
    return () => {
      alive = false
    }
  }, [open, fields])
  return listed
}

export function withBinding(bindings: Bindings | undefined, capability: string, next: Binding | null): Bindings {
  const rest: Bindings = { ...(bindings ?? {}) }
  delete rest[capability]
  return next ? { ...rest, [capability]: next } : rest
}

export function suggested(ref: string | null, fields: BindingField[], options: BindingOptions, current: Bindings | undefined): Bindings {
  let bindings: Bindings = { ...(current ?? {}) }
  if (!ref) return bindings
  for (const field of fields) {
    if (bindings[field.capability]) continue
    for (const [connection, listed] of Object.entries(options[field.capability] ?? {})) {
      const found = listed.find((o) => o.value.toLowerCase() === ref.toLowerCase())
      if (found) {
        bindings = withBinding(bindings, field.capability, { connection, value: found.value, label: found.label })
        break
      }
    }
  }
  return bindings
}

export function BindingSelect({ field, options, value, onChange }: { field: BindingField; options: BindingOptions; value: Binding | undefined; onChange: (next: Binding | null) => void }) {
  const t = useT()
  const listed = options[field.capability] ?? {}
  const choices = [
    { value: NONE, label: t('settings.none') },
    ...field.sources.flatMap((s) => (listed[s.connection.id] ?? []).map((o) => ({ value: choiceKey(s.connection.id, o.value), label: o.label, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const current = value ? choiceKey(value.connection, value.value) : NONE
  if (value && !choices.some((o) => o.value === current)) choices.push({ value: current, label: value.label || value.value })
  const labelOf = (connection: string, key: string) => (listed[connection] ?? []).find((o) => o.value === key)?.label
  return (
    <Field label={field.label}>
      <Select label={field.label} className="w-full" value={current} options={choices} onValueChange={(v) => {
        const picked = fromKey(v)
        onChange(picked ? { connection: picked.connection, value: picked.key, label: labelOf(picked.connection, picked.key) ?? picked.key } : null)
      }} />
    </Field>
  )
}

export function withChoices(binding: Binding, more: BindingChoice[]): Binding {
  const kept = more.filter((c, at) => c.value !== binding.value && more.findIndex((o) => o.value === c.value) === at)
  const { more: _, ...rest } = binding
  return kept.length > 0 ? { ...rest, more: kept } : rest
}

function MoreChoices({ field, options, value, onChange }: { field: BindingField; options: BindingOptions; value: Binding; onChange: (next: Binding) => void }) {
  const t = useT()
  const listed = options[field.capability]?.[value.connection] ?? []
  const more = value.more ?? []
  const taken = new Set([value.value, ...more.map((c) => c.value)])
  const choicesFor = (current: string) => [
    { value: NONE, label: current === NONE ? t('settings.addAnother') : t('settings.none') },
    ...listed.filter((o) => o.value === current || !taken.has(o.value)).map((o) => ({ value: o.value, label: o.label })),
  ]
  const labelOf = (key: string) => listed.find((o) => o.value === key)?.label ?? key
  const rows = [...more.map((c) => c.value), NONE]
  return (
    <>
      {rows.map((current, at) => {
        const choices = choicesFor(current)
        if (current !== NONE && !choices.some((o) => o.value === current)) choices.push({ value: current, label: more[at]?.label || current })
        if (current === NONE && choices.length === 1) return null
        return (
          <Select key={`${at}:${current}`} label={field.label} className="mt-2 w-full" value={current} options={choices} onValueChange={(v) => {
            const next = [...more]
            if (v === NONE) next.splice(at, 1)
            else next[at] = { value: v, label: labelOf(v) }
            onChange(withChoices(value, next))
          }} />
        )
      })}
    </>
  )
}

export function BindingFields({ fields, options, bindings, onChange }: { fields: BindingField[]; options: BindingOptions; bindings: Bindings | undefined; onChange: (next: Bindings) => void }) {
  return (
    <>
      {fields.map((field) => field.shape ? (
        <ShapedBinding key={field.capability} field={field} shape={field.shape} value={bindings?.[field.capability]} onChange={(next) => onChange(withBinding(bindings, field.capability, next))} />
      ) : (
        <div key={field.capability}>
          <BindingSelect field={field} options={options} value={bindings?.[field.capability]} onChange={(next) => {
            const before = bindings?.[field.capability]
            const kept = next && before && field.multiple && before.connection === next.connection ? withChoices(next, before.more ?? []) : next
            onChange(withBinding(bindings, field.capability, kept))
          }} />
          {field.multiple && bindings?.[field.capability] ? (
            <MoreChoices field={field} options={options} value={bindings[field.capability] as Binding} onChange={(next) => onChange(withBinding(bindings, field.capability, next))} />
          ) : null}
        </div>
      ))}
    </>
  )
}

type Choices = Record<string, BindingChoice>
type FieldOptions = Record<string, BindingOption[]>

const filled = (choices: Choices | undefined, key: string) => Boolean(choices?.[key]?.value.trim())

export function missingOf(fields: BindingField[], bindings: Bindings | undefined): { field: string; binding: string } | null {
  for (const field of fields) {
    const bound = bindings?.[field.capability]
    if (!field.shape || !bound?.connection) continue
    const lack = field.shape.fields.find((spec) => spec.required && !filled(bound.fields, spec.key))
    if (lack) return { field: lack.label, binding: field.label }
    const each = field.shape.each
    if (!each) continue
    const profiles = bound.profiles ?? []
    if (profiles.length === 0 && each.fields.some((spec) => spec.required)) return { field: each.label, binding: field.label }
    for (const profile of profiles) {
      if (!filled(profile, each.key)) return { field: each.label, binding: field.label }
      const gap = each.fields.find((spec) => spec.required && !filled(profile, spec.key))
      if (gap) return { field: `${gap.label} (${profile[each.key]?.label || profile[each.key]?.value})`, binding: field.label }
    }
  }
  return null
}

function useFieldOptions(connection: string, capability: string, keys: string[]): FieldOptions {
  const [listed, setListed] = useState<FieldOptions>({})
  const wanted = keys.join('|')
  useEffect(() => {
    setListed({})
    if (!connection) return
    let alive = true
    for (const field of wanted.split('|').filter(Boolean)) {
      api()
        .call(connection, `${capability}.options`, { field })
        .then((found) => alive && setListed((known) => ({ ...known, [field]: found })))
        .catch(() => alive && setListed((known) => ({ ...known, [field]: [] })))
    }
    return () => {
      alive = false
    }
  }, [connection, capability, wanted])
  return listed
}

function ChoiceSelect({ spec, options, value, onChange }: { spec: BindingFieldSpec; options: BindingOption[]; value: BindingChoice | undefined; onChange: (next: BindingChoice | null) => void }) {
  const t = useT()
  const label = spec.required ? `${spec.label} *` : spec.label
  const choices = [{ value: NONE, label: t('settings.none') }, ...options.map((o) => ({ value: o.value, label: o.label }))]
  if (value && !choices.some((o) => o.value === value.value)) choices.push({ value: value.value, label: value.label || value.value })
  return (
    <Field label={label}>
      <Select label={spec.label} className="w-full" value={value?.value ?? NONE} options={choices} onValueChange={(v) => {
        if (v === NONE) return onChange(null)
        onChange({ value: v, label: options.find((o) => o.value === v)?.label ?? v })
      }} />
    </Field>
  )
}

function withChoice(choices: Choices | undefined, key: string, next: BindingChoice | null): Choices {
  const rest: Choices = { ...(choices ?? {}) }
  delete rest[key]
  return next ? { ...rest, [key]: next } : rest
}

function ProfileCard({ each, options, value, onChange, onRemove }: { each: BindingEach; options: FieldOptions; value: Choices; onChange: (next: Choices) => void; onRemove: () => void }) {
  const t = useT()
  const head = { key: each.key, label: each.label, required: true }
  return (
    <Card variant="inset" className="space-y-3 px-3 py-3">
      <div className="flex items-end gap-2">
        <div className="min-w-0 flex-1"><ChoiceSelect spec={head} options={options[each.key] ?? []} value={value[each.key]} onChange={(next) => onChange(withChoice(value, each.key, next))} /></div>
        <IconButton tooltip={t('settings.app_bindingRemove', { label: each.label })} size="sm" onClick={onRemove}><Icon name="trash" size={15} /></IconButton>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {each.fields.map((spec) => <ChoiceSelect key={spec.key} spec={spec} options={options[spec.key] ?? []} value={value[spec.key]} onChange={(next) => onChange(withChoice(value, spec.key, next))} />)}
      </div>
    </Card>
  )
}

function ConnectionSelect({ field, value, onChange }: { field: BindingField; value: string; onChange: (next: string) => void }) {
  const t = useT()
  const choices = [{ value: NONE, label: t('settings.none') }, ...field.sources.map((s) => ({ value: s.connection.id, label: s.connection.label || s.provider.name, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))]
  return (
    <Field label={t('settings.app_bindingConnection')}>
      <Select label={t('settings.app_bindingConnection')} className="w-full" value={value || NONE} options={choices} onValueChange={(v) => onChange(v === NONE ? '' : v)} />
    </Field>
  )
}

export function ShapedBinding({ field, shape, value, onChange }: { field: BindingField; shape: BindingShape; value: Binding | undefined; onChange: (next: Binding | null) => void }) {
  const t = useT()
  const single = field.sources.length === 1 ? field.sources[0].connection.id : ''
  const connection = value?.connection || single
  const keys = [...shape.fields.map((spec) => spec.key), ...(shape.each ? [shape.each.key, ...shape.each.fields.map((spec) => spec.key)] : [])]
  const options = useFieldOptions(connection, field.capability, keys)
  const current: Binding = { connection, value: value?.value ?? '', fields: value?.fields ?? {}, profiles: value?.profiles ?? [] }
  const set = (next: Partial<Binding>) => onChange({ ...current, ...next })
  const profiles = current.profiles ?? []
  const each = shape.each
  return (
    <div className="space-y-3 sm:col-span-2" data-testid={`binding-${field.capability}`}>
      <p className="text-base font-medium text-fg">{field.label}</p>
      {field.sources.length > 1 && <ConnectionSelect field={field} value={value?.connection ?? ''} onChange={(next) => onChange(next ? { connection: next, value: '', fields: {}, profiles: [] } : null)} />}
      {connection && shape.fields.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2">
          {shape.fields.map((spec) => <ChoiceSelect key={spec.key} spec={spec} options={options[spec.key] ?? []} value={current.fields?.[spec.key]} onChange={(next) => set({ fields: withChoice(current.fields, spec.key, next) })} />)}
        </div>
      )}
      {connection && each && (
        <div className="space-y-2">
          {profiles.map((profile, at) => (
            <ProfileCard key={at} each={each} options={options} value={profile} onChange={(next) => set({ profiles: profiles.map((p, i) => (i === at ? next : p)) })} onRemove={() => set({ profiles: profiles.filter((_, i) => i !== at) })} />
          ))}
          <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => set({ profiles: [...profiles, {}] })}>{t('settings.app_bindingAdd', { label: each.label })}</Button>
        </div>
      )}
    </div>
  )
}

export function boundText(binding: Binding, each: BindingEach | undefined): string {
  if (binding.label || binding.value) return binding.label || binding.value
  const profiles = (binding.profiles ?? []).map((p) => (each ? p[each.key]?.label || p[each.key]?.value : '')).filter(Boolean)
  if (profiles.length > 0) return profiles.join(' · ')
  return Object.values(binding.fields ?? {}).map((c) => c.label || c.value).join(' · ')
}

export function useBoundLook(): (capability: string, binding: Binding) => { label: string; text: string; source: Source | null } {
  const connections = useStore((s) => s.connections)
  return (capability, binding) => {
    const connection = connections?.connections.find((c) => c.id === binding.connection)
    const provider = connections?.providers.find((p) => p.key === connection?.provider)
    const spec = provider?.bindings?.find((b) => b.capability === capability)
    return { label: spec?.label ?? capability, text: boundText(binding, spec?.each), source: connection && provider ? { connection, provider } : null }
  }
}
