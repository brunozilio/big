import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { api } from '@/api'
import type { Api, Binding, BindingChoice, BindingEach, BindingSummary, BindingFieldSpec, BindingOption, Bindings, BindingScope, Connection, Provider } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Card } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Field, MultiSelect, Select } from '@/ui/form'
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
  section?: string
  order?: number
}

export interface BindingShape {
  fields: BindingFieldSpec[]
  each: BindingEach | null
}

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
      const placed = { ...(spec.section ? { section: spec.section } : {}), ...(spec.order === undefined ? {} : { order: spec.order }) }
      if (!known) fields.push({ capability: spec.capability, label: spec.label, sources: [{ connection, provider: provider as Provider }], multiple: spec.multiple === true, ...(shape ? { shape } : {}), ...placed })
      else {
        if (!known.shape && shape) known.shape = shape
        if (!known.section && spec.section) known.section = spec.section
        if (known.order === undefined && spec.order !== undefined) known.order = spec.order
        known.sources.push({ connection, provider: provider as Provider })
        known.multiple = known.multiple || spec.multiple === true
        if (!known.label.split(' / ').includes(spec.label)) known.label = `${known.label} / ${spec.label}`
      }
    }
  }
  return ordered(fields)
}

const LAST = Number.MAX_SAFE_INTEGER

function ordered(fields: BindingField[]): BindingField[] {
  return fields.map((field, at) => ({ field, at })).sort((a, b) => (a.field.order ?? LAST) - (b.field.order ?? LAST) || a.at - b.at).map((placed) => placed.field)
}

export interface FieldGroup {
  section: string | null
  fields: BindingField[]
}

export function groupedBySection(fields: BindingField[]): FieldGroup[] {
  const groups: FieldGroup[] = [{ section: null, fields: [] }]
  for (const field of fields) {
    const section = field.section?.trim() || null
    const known = groups.find((g) => g.section === section)
    if (known) known.fields.push(field)
    else groups.push({ section, fields: [field] })
  }
  return groups
}

export function useBindingFields(scope: BindingScope): BindingField[] {
  const connections = useStore((s) => s.connections)
  return useMemo(() => fieldsOf(connections?.connections ?? [], connections?.providers ?? [], scope), [connections, scope])
}

const PAUSE = 250
const FRESH_MS = 30_000

type Found = Record<string, BindingOption[]>

export interface Search {
  found: Found
  search: (query: string) => void
  hint: string | undefined
  complete: boolean
  more: () => void
}

type Listed = { found: Found; failed: string | null }

type Asked = { query: string; field?: string; chosen?: Record<string, string>; names?: string[]; page?: number }

type Kept = { at: number; listed: Listed }

const kept = new WeakMap<Api, Map<string, Kept>>()
const flying = new WeakMap<Api, Map<string, Promise<Listed>>>()

function shelf<T>(all: WeakMap<Api, Map<string, T>>): Map<string, T> {
  const owner = api()
  const known = all.get(owner)
  if (known) return known
  const made = new Map<string, T>()
  all.set(owner, made)
  return made
}

function keyOf(connections: string[], action: string, args: Asked): string {
  return JSON.stringify([connections, action, args.field ?? '', args.chosen ?? {}, args.query, args.names ?? [], args.page ?? 0])
}

function recalled(key: string): Listed | null {
  const hit = shelf(kept).get(key)
  if (!hit) return null
  if (Date.now() - hit.at <= FRESH_MS) return hit.listed
  shelf(kept).delete(key)
  return null
}

function asked(connections: string[], action: `${string}.options`, args: Asked): Promise<Listed> {
  let failed: string | null = null
  const each = connections.map((connection) =>
    api()
      .call(connection, action, args)
      .then((listed) => [connection, listed] as const)
      .catch((e: unknown) => ((failed = errorText(e)), [connection, [] as BindingOption[]] as const)),
  )
  return Promise.all(each).then((pairs) => ({ found: Object.fromEntries(pairs), failed }))
}

function lookup(connections: string[], action: `${string}.options`, args: Asked): Promise<Listed> {
  const key = keyOf(connections, action, args)
  const hit = recalled(key)
  if (hit) return Promise.resolve(hit)
  const running = shelf(flying)
  const known = running.get(key)
  if (known) return known
  const store = shelf(kept)
  const sent = asked(connections, action, args)
    .then((listed) => {
      if (!listed.failed) store.set(key, { at: Date.now(), listed })
      return listed
    })
    .finally(() => running.delete(key))
  running.set(key, sent)
  return sent
}

type Shown = { scope: string; found: Found; failed: string | null }

const NOTHING: Found = {}

type Paged = { from: string; page: number; found: Found; done: boolean; loading: boolean }

const UNPAGED: Paged = { from: '', page: 0, found: NOTHING, done: false, loading: false }

function joinedFound(first: Found, later: Found): Found {
  const keys = [...new Set([...Object.keys(first), ...Object.keys(later)])]
  return Object.fromEntries(
    keys.map((key) => {
      const known = first[key] ?? []
      const seen = new Set(known.map((o) => o.value))
      return [key, [...known, ...(later[key] ?? []).filter((o) => !seen.has(o.value))]]
    }),
  )
}

function grew(before: Found, after: Found): boolean {
  return Object.keys(after).some((key) => (after[key]?.length ?? 0) > (before[key]?.length ?? 0))
}

function usePages(from: string, ready: boolean, base: Found, ask: (page: number) => Promise<Listed>) {
  const [paged, setPaged] = useState<Paged>(UNPAGED)
  const current = paged.from === from ? paged : { ...UNPAGED, from }
  const found = useMemo(() => joinedFound(base, current.found), [base, current.found])
  const more = useCallback(() => {
    if (!ready || current.done || current.loading) return
    const page = current.page + 1
    setPaged({ ...current, loading: true })
    void ask(page).then((listed) => {
      setPaged((now) => {
        if (now.from !== from) return now
        const next = joinedFound(now.found, listed.found)
        const done = Boolean(listed.failed) || !grew(joinedFound(base, now.found), joinedFound(base, next))
        return { from, page, found: next, done, loading: false }
      })
    })
  }, [ready, current, ask, from, base])
  return { found, more, loading: current.loading }
}

export function useSearch(connections: string[], capability: string, field?: string, eager = false, chosen?: Record<string, string>): Search {
  const t = useT()
  const [active, setActive] = useState(false)
  const [query, setQuery] = useState('')
  const [shown, setShown] = useState<Shown>({ scope: '', found: NOTHING, failed: null })
  const [busy, setBusy] = useState(false)
  const begun = useRef(false)
  const wanted = connections.filter(Boolean).join('|')
  const given = chosen && Object.keys(chosen).length > 0 ? JSON.stringify(chosen) : ''
  const scope = `${wanted} ${capability} ${field ?? ''} ${given}`
  useEffect(() => {
    if (!active) return
    const typed = query.trim()
    if ((typed === '' && !eager) || wanted === '') {
      setShown({ scope, found: NOTHING, failed: null })
      setBusy(false)
      return
    }
    const ids = wanted.split('|')
    const args: Asked = { query: typed, ...(field ? { field } : {}), ...(given ? { chosen: JSON.parse(given) as Record<string, string> } : {}) }
    const hit = recalled(keyOf(ids, `${capability}.options`, args))
    if (hit) {
      setShown({ scope, found: hit.found, failed: hit.failed })
      setBusy(false)
      return
    }
    setBusy(true)
    let alive = true
    const timer = setTimeout(() => {
      void lookup(ids, `${capability}.options`, args).then((listed) => {
        if (!alive) return
        setShown({ scope, found: listed.found, failed: listed.failed })
        setBusy(false)
      })
    }, typed === '' ? 0 : PAUSE)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [active, query, wanted, capability, field, eager, given, scope])
  const search = useCallback((next: string) => {
    if (!begun.current && eager) setBusy(true)
    begun.current = true
    setActive(true)
    setQuery(next)
  }, [eager])
  const base = shown.scope === scope ? shown.found : NOTHING
  const failed = shown.scope === scope ? shown.failed : null
  const typed = query.trim()
  const ask = useCallback((page: number) => {
    const args: Asked = { query: typed, page, ...(field ? { field } : {}), ...(given ? { chosen: JSON.parse(given) as Record<string, string> } : {}) }
    return lookup(wanted.split('|'), `${capability}.options`, args)
  }, [typed, field, given, wanted, capability])
  const ready = active && !busy && !failed && wanted !== '' && (eager || typed !== '') && Object.values(base).some((listed) => listed.length > 0)
  const { found, more, loading } = usePages(`${scope} ${typed}`, ready, base, ask)
  const empty = Object.values(found).every((listed) => listed.length === 0)
  const hint = busy || loading ? t('settings.search_busy') : failed && empty ? failed : query.trim() === '' && !eager ? t('settings.search_type') : empty ? t('settings.search_empty') : undefined
  const complete = active && eager && !busy && shown.scope === scope && !failed && query.trim() === ''
  return { found, search, hint, complete, more }
}

export function withBinding(bindings: Bindings | undefined, capability: string, next: Binding | null): Bindings {
  const rest: Bindings = { ...(bindings ?? {}) }
  delete rest[capability]
  return next ? { ...rest, [capability]: next } : rest
}

const plain = (text: string) => text.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')

function partsOf(option: BindingOption): string[] {
  const pieces = [option.value, option.label].flatMap((text) => text.split(/[/:\\·|]+/)).map((piece) => piece.trim())
  return pieces.flatMap((piece) => [piece, piece.replace(/^[^_]+_/, '')]).map(plain).filter(Boolean)
}

export function matching(options: BindingOption[], names: string[]): BindingOption | undefined {
  for (const name of names.map(plain).filter(Boolean)) {
    const whole = options.find((o) => plain(o.value) === name || plain(o.label) === name)
    const found = whole ?? options.find((o) => partsOf(o).includes(name))
    if (found) return found
  }
  return undefined
}

function namesOf(ref: string | null, also: string[]): string[] {
  const given = [ref ?? '', ref ? (ref.split('/').pop() ?? '') : '', ...also].map((name) => name.trim()).filter(Boolean)
  return [...new Set(given)]
}

function suggests(source: Source, capability: string): boolean {
  return (source.provider.bindings ?? []).some((spec) => spec.capability === capability && spec.suggests === true)
}

async function declared(field: BindingField, names: string[]): Promise<Binding | null> {
  const connections = field.sources.filter((s) => suggests(s, field.capability)).map((s) => s.connection.id)
  if (connections.length === 0) return null
  const { found } = await lookup(connections, `${field.capability}.options`, { query: '', names })
  for (const connection of connections) {
    const first = found[connection]?.[0]
    if (first) return { connection, value: first.value, label: first.label }
  }
  return null
}

async function suggestion(field: BindingField, names: string[]): Promise<Binding | null> {
  const own = await declared(field, names)
  if (own) return own
  const connections = field.sources.filter((s) => !suggests(s, field.capability)).map((s) => s.connection.id)
  if (connections.length === 0) return null
  const answers = await Promise.all(names.map((query) => lookup(connections, `${field.capability}.options`, { query })))
  for (const { found: listed } of answers) {
    for (const [connection, options] of Object.entries(listed)) {
      const found = matching(options, names)
      if (found) return { connection, value: found.value, label: found.label }
    }
  }
  return null
}

export async function suggested(ref: string | null, fields: BindingField[], current: Bindings | undefined, also: string[] = []): Promise<Bindings> {
  const names = namesOf(ref, also)
  if (names.length === 0) return { ...(current ?? {}) }
  const open = fields.filter((field) => !current?.[field.capability] && !field.shape)
  const found = await Promise.all(open.map((field) => suggestion(field, names)))
  return open.reduce<Bindings>((bindings, field, at) => (found[at] ? withBinding(bindings, field.capability, found[at]) : bindings), { ...(current ?? {}) })
}

export function BindingSelect({ field, value, onChange }: { field: BindingField; value: Binding | undefined; onChange: (next: Binding | null) => void }) {
  const t = useT()
  const { found: listed, search, hint, more } = useSearch(field.sources.map((s) => s.connection.id), field.capability, undefined, true)
  const choices = [
    { value: NONE, label: t('settings.none') },
    ...field.sources.flatMap((s) => (listed[s.connection.id] ?? []).map((o) => ({ value: choiceKey(s.connection.id, o.value), label: o.label, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const current = value ? choiceKey(value.connection, value.value) : NONE
  if (value && !choices.some((o) => o.value === current)) choices.push({ value: current, label: value.label || value.value })
  const labelOf = (connection: string, key: string) => (listed[connection] ?? []).find((o) => o.value === key)?.label
  return (
    <Field label={field.label}>
      <Select label={field.label} className="w-full" value={current} options={choices} onSearch={search} onMore={more} hint={hint} onValueChange={(v) => {
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

function MoreChoices({ field, value, onChange }: { field: BindingField; value: Binding; onChange: (next: Binding) => void }) {
  const t = useT()
  const { found, search, hint, more: further } = useSearch([value.connection], field.capability, undefined, true)
  const listed = found[value.connection] ?? []
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
        return (
          <Select key={`${at}:${current}`} label={field.label} className="mt-2 w-full" value={current} options={choices} onSearch={search} onMore={further} hint={hint} onValueChange={(v) => {
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

export interface BindingProblem {
  capability: string
  text: string
}

export const BindingFields = memo(function BindingFields({ fields, bindings, problem, onChange }: { fields: BindingField[]; bindings: Bindings | undefined; problem?: BindingProblem | null; onChange: (next: Bindings) => void }) {
  return (
    <>
      {fields.map((field) => field.shape ? (
        <ShapedBinding key={field.capability} field={field} shape={field.shape} value={bindings?.[field.capability]} error={problem?.capability === field.capability ? problem.text : undefined} onChange={(next) => onChange(withBinding(bindings, field.capability, next))} />
      ) : (
        <div key={field.capability}>
          <BindingSelect field={field} value={bindings?.[field.capability]} onChange={(next) => {
            const before = bindings?.[field.capability]
            const kept = next && before && field.multiple && before.connection === next.connection ? withChoices(next, before.more ?? []) : next
            onChange(withBinding(bindings, field.capability, kept))
          }} />
          {field.multiple && bindings?.[field.capability] ? (
            <MoreChoices field={field} value={bindings[field.capability] as Binding} onChange={(next) => onChange(withBinding(bindings, field.capability, next))} />
          ) : null}
        </div>
      ))}
    </>
  )
})

type Choices = Record<string, BindingChoice>

const filled = (choices: Choices | undefined, key: string) => Boolean(choices?.[key]?.value.trim())

const vacant = new WeakMap<Api, Map<string, boolean>>()

function vacancyKey(connection: string, capability: string, field: string, chosen: Record<string, string> | undefined): string {
  return JSON.stringify([connection, capability, field, Object.entries(chosen ?? {}).sort(([a], [b]) => a.localeCompare(b))])
}

function isVacant(connection: string, capability: string, field: string, chosen: Record<string, string> | undefined): boolean {
  return shelf(vacant).get(vacancyKey(connection, capability, field, chosen)) === true
}

function needed(spec: BindingFieldSpec, profile: Choices, connection: string, capability: string): boolean {
  return Boolean(spec.required) && !filled(profile, spec.key) && !(spec.skippable && isVacant(connection, capability, spec.key, chosenOf(profile, spec.key)))
}

export function missingOf(fields: BindingField[], bindings: Bindings | undefined): { field: string; binding: string; capability: string } | null {
  for (const field of fields) {
    const bound = bindings?.[field.capability]
    if (!field.shape || !bound?.connection) continue
    const lack = field.shape.fields.find((spec) => spec.required && !filled(bound.fields, spec.key))
    if (lack) return { field: lack.label, binding: field.label, capability: field.capability }
    const each = field.shape.each
    if (!each) continue
    const profiles = (bound.profiles ?? []).filter((profile) => Object.values(profile).some((choice) => choice?.value.trim()))
    for (const profile of profiles) {
      if (!filled(profile, each.key)) return { field: each.label, binding: field.label, capability: field.capability }
      const { [each.key]: _, ...rest } = profile
      const gap = each.fields.find((spec) => needed(spec, rest, bound.connection, field.capability))
      if (gap) return { field: `${gap.label} (${profile[each.key]?.label || profile[each.key]?.value})`, binding: field.label, capability: field.capability }
    }
  }
  return null
}

function useSettled(spec: BindingFieldSpec, connection: string, capability: string, ready: boolean, value: BindingChoice | undefined, chosen: Record<string, string> | undefined, onChange: (next: BindingChoice | null) => void) {
  const searched = useSearch([connection], capability, spec.key, true, chosen)
  const { search, complete } = searched
  const options = searched.found[connection] ?? []
  const listed = ready && complete
  const only = listed && options.length === 1 ? options[0] : undefined
  const none = Boolean(spec.skippable) && listed && options.length === 0
  const key = vacancyKey(connection, capability, spec.key, chosen)
  useEffect(() => {
    if (ready) search('')
  }, [ready, search])
  useEffect(() => {
    if (spec.required && !value && only) onChange({ value: only.value, label: only.label })
  }, [spec.required, value, only, onChange])
  useEffect(() => {
    if (listed) shelf(vacant).set(key, none)
  }, [listed, none, key])
  return { ...searched, options, none }
}

function ChoiceSelect({ spec, connection, capability, value, chosen, ready = false, onChange }: { spec: BindingFieldSpec; connection: string; capability: string; value: BindingChoice | undefined; chosen?: Record<string, string>; ready?: boolean; onChange: (next: BindingChoice | null) => void }) {
  const t = useT()
  const { options, search, hint, none } = useSettled(spec, connection, capability, ready, value, chosen, onChange)
  if (none && !value) return null
  const label = spec.required ? `${spec.label} *` : spec.label
  const choices = [{ value: NONE, label: t('settings.none') }, ...options.map((o) => ({ value: o.value, label: o.label }))]
  if (value && !choices.some((o) => o.value === value.value)) choices.push({ value: value.value, label: value.label || value.value })
  return (
    <Field label={label}>
      <Select label={spec.label} className="w-full" value={value?.value ?? NONE} options={choices} onSearch={search} hint={hint} onValueChange={(v) => {
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

function chosenOf(choices: Choices, except: string): Record<string, string> {
  return Object.fromEntries(Object.entries(choices).filter(([key, choice]) => key !== except && choice?.value).map(([key, choice]) => [key, choice.value]))
}

function withDependent(choices: Choices, specs: BindingFieldSpec[], key: string, next: BindingChoice | null): Choices {
  const at = specs.findIndex((spec) => spec.key === key)
  const changed = (choices[key]?.value ?? '') !== (next?.value ?? '')
  const later = changed && at >= 0 ? specs.slice(at + 1).map((spec) => spec.key) : []
  const kept: Choices = Object.fromEntries(Object.entries(choices).filter(([name]) => !later.includes(name)))
  return withChoice(kept, key, next)
}

export interface ProfileGroup {
  heads: BindingChoice[]
  rest: Choices
}

const signature = (rest: Choices) => JSON.stringify(Object.entries(rest).map(([key, choice]) => [key, choice.value]).sort())

export function groupsOf(profiles: Choices[], key: string): ProfileGroup[] {
  const groups: ProfileGroup[] = []
  for (const profile of profiles) {
    const { [key]: head, ...rest } = profile
    const same = head ? groups.find((g) => g.heads.length > 0 && signature(g.rest) === signature(rest)) : undefined
    if (same && head) same.heads.push(head)
    else groups.push({ heads: head ? [head] : [], rest })
  }
  return groups
}

export function profilesOf(groups: ProfileGroup[], key: string): Choices[] {
  return groups.flatMap((g) => (g.heads.length === 0 ? [g.rest] : g.heads.map((head) => ({ ...g.rest, [key]: head }))))
}

function HeadSelect({ each, connection, capability, heads, taken, onChange }: { each: BindingEach; connection: string; capability: string; heads: BindingChoice[]; taken: string[]; onChange: (next: BindingChoice[]) => void }) {
  const { found, search, hint } = useSearch([connection], capability, each.key, true)
  const listed = found[connection] ?? []
  const mine = heads.map((h) => h.value)
  const options = listed.filter((o) => mine.includes(o.value) || !taken.includes(o.value)).map((o) => ({ value: o.value, label: o.label }))
  for (const head of heads) if (!options.some((o) => o.value === head.value)) options.push({ value: head.value, label: head.label || head.value })
  const toggle = (value: string) => {
    if (mine.includes(value)) return onChange(heads.filter((h) => h.value !== value))
    onChange([...heads, { value, label: listed.find((o) => o.value === value)?.label ?? value }])
  }
  return (
    <Field label={`${each.label} *`}>
      <MultiSelect label={each.label} className="w-full" values={mine} options={options} onSearch={search} hint={hint} onToggle={toggle} />
    </Field>
  )
}

function ProfileCard({ each, connection, capability, group, taken, onChange, onRemove }: { each: BindingEach; connection: string; capability: string; group: ProfileGroup; taken: string[]; onChange: (next: ProfileGroup) => void; onRemove: () => void }) {
  const t = useT()
  const readyAt = (at: number) => group.heads.length > 0 && each.fields.slice(0, at).every((spec) => !spec.required || filled(group.rest, spec.key))
  return (
    <Card variant="inset" className="space-y-3 px-3 py-3">
      <div className="flex items-end gap-2">
        <div className="min-w-0 flex-1"><HeadSelect each={each} connection={connection} capability={capability} heads={group.heads} taken={taken} onChange={(heads) => onChange({ ...group, heads })} /></div>
        <IconButton tooltip={t('settings.app_bindingRemove', { label: each.label })} size="sm" onClick={onRemove}><Icon name="trash" size={15} /></IconButton>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {each.fields.map((spec, at) => <ChoiceSelect key={spec.key} spec={spec} connection={connection} capability={capability} value={group.rest[spec.key]} chosen={chosenOf(group.rest, spec.key)} ready={readyAt(at)} onChange={(next) => onChange({ ...group, rest: withDependent(group.rest, each.fields, spec.key, next) })} />)}
      </div>
    </Card>
  )
}

function Profiles({ each, connection, capability, profiles, onChange }: { each: BindingEach; connection: string; capability: string; profiles: Choices[]; onChange: (next: Choices[]) => void }) {
  const t = useT()
  const [groups, setGroups] = useState(() => groupsOf(profiles, each.key))
  const shown = JSON.stringify(profilesOf(groups, each.key)) === JSON.stringify(profiles) ? groups : groupsOf(profiles, each.key)
  const update = (next: ProfileGroup[]) => {
    setGroups(next)
    onChange(profilesOf(next, each.key))
  }
  const takenBy = (at: number) => shown.flatMap((g, i) => (i === at ? [] : g.heads.map((h) => h.value)))
  return (
    <div className="space-y-2">
      {shown.map((group, at) => (
        <ProfileCard key={at} each={each} connection={connection} capability={capability} group={group} taken={takenBy(at)} onChange={(next) => update(shown.map((g, i) => (i === at ? next : g)))} onRemove={() => update(shown.filter((_, i) => i !== at))} />
      ))}
      <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => update([...shown, { heads: [], rest: {} }])}>{t('settings.app_bindingAdd', { label: each.label })}</Button>
    </div>
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

export function ShapedBinding({ field, shape, value, error, onChange }: { field: BindingField; shape: BindingShape; value: Binding | undefined; error?: string; onChange: (next: Binding | null) => void }) {
  const single = field.sources.length === 1 ? field.sources[0].connection.id : ''
  const live = field.sources.some((s) => s.connection.id === value?.connection)
  const connection = live && value ? value.connection : single
  const current: Binding = { connection, value: value?.value ?? '', fields: value?.fields ?? {}, profiles: value?.profiles ?? [] }
  const set = (next: Partial<Binding>) => onChange({ ...current, ...next })
  const profiles = current.profiles ?? []
  const each = shape.each
  return (
    <div className={cn('space-y-3 sm:col-span-2', error && '-m-2 rounded-control p-2 shadow-[inset_0_0_0_1px_rgb(var(--danger)/0.45)]')} data-testid={`binding-${field.capability}`} data-invalid={error ? 'true' : undefined}>
      <p className="text-base font-medium text-fg">{field.label}</p>
      {error && <p className="flex items-center gap-1 text-xs text-danger-fg" data-testid="binding-error"><Icon name="alert" size={13} />{error}</p>}
      {field.sources.length > 1 && <ConnectionSelect field={field} value={live ? (value?.connection ?? '') : ''} onChange={(next) => onChange(next ? { connection: next, value: '', fields: {}, profiles: [] } : null)} />}
      {connection && shape.fields.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2">
          {shape.fields.map((spec) => <ChoiceSelect key={spec.key} spec={spec} connection={connection} capability={field.capability} value={current.fields?.[spec.key]} onChange={(next) => set({ fields: withChoice(current.fields, spec.key, next) })} />)}
        </div>
      )}
      {connection && each && <Profiles each={each} connection={connection} capability={field.capability} profiles={profiles} onChange={(next) => set({ profiles: next })} />}
    </div>
  )
}

const PLACEHOLDER = /\{([^{}]+)\}/g
const LOOSE_EDGES = /^[\s·•|,;:/-]+|[\s·•|,;:/-]+$/g

function rendered(template: string, pick: (key: string) => string): string {
  const keys = [...template.matchAll(PLACEHOLDER)].map((m) => m[1].trim())
  if (keys.length > 0 && keys.every((key) => !pick(key))) return ''
  const parts = template.split(PLACEHOLDER).map((part, i) => (i % 2 === 1 ? pick(part.trim()) : part))
  return parts.filter((_, i) => i % 2 === 1 || (parts[i - 1] !== '' && parts[i + 1] !== '')).join('').replace(LOOSE_EDGES, '')
}

export function summaryText(binding: Binding, { template, separator = ' · ' }: BindingSummary): string {
  const own = (key: string) => (key === 'value' ? binding.value : key === 'label' ? binding.label || binding.value : '')
  const choice = (sources: Record<string, BindingChoice>[]) => (key: string) => {
    const found = sources.map((source) => source[key]).find(Boolean)
    return found?.label || found?.value || own(key)
  }
  const fields = binding.fields ?? {}
  const profiles = binding.profiles ?? []
  const lines = profiles.length > 0 ? profiles.map((p) => rendered(template, choice([p, fields]))) : [rendered(template, choice([fields]))]
  return [...new Set(lines.filter(Boolean))].join(separator)
}

export function boundText(binding: Binding, each: BindingEach | undefined, summary?: BindingSummary): string {
  const summarized = summary ? summaryText(binding, summary) : ''
  if (summarized) return summarized
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
    return { label: spec?.label ?? capability, text: boundText(binding, spec?.each, spec?.summary), source: connection && provider ? { connection, provider } : null }
  }
}
