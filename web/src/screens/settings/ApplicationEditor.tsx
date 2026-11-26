import { memo, useCallback, useEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react'
import { api } from '@/api'
import type { Application, ApplicationDraft, Bindings, DirListing, Flows, RepositoryKind } from '@/api/contract'
import { errorText } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Card } from '@/ui/display'
import { Field, Input } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Collapse } from '@/ui/motion'
import { toast } from '@/ui/toast'
import { useT, type T } from '@/i18n'
import { cleanOf, draftOf, editingOf, folderName, parentOf, reduce, sameDraft, withFolders, type Change, type Editing, type Row, type Variable } from './applicationDraft'
import { useKinds } from './applicationFacts'
import { saveApplication, saveOptimistically } from './applicationSave'
import { BindingFields, groupedBySection, missingOf, suggested, useBindingFields, type BindingField, type BindingProblem, type FieldGroup } from './bindings'
import { FolderPopover } from './FolderDialog'
import { useFlows } from './flows'
import { RepositoryCard } from './RepositoryEditor'

const GENERAL = 'general'
const REPOSITORIES = 'repositories'
const ENVIRONMENT = 'environment'

const sectionKey = (name: string) => `declared:${name}`

interface Problem {
  section: string | null
  text: string
  name?: boolean
  row?: string
  binding?: BindingProblem
}

export interface EditorProps {
  application: Application | null
  onSaved: (saved: Application) => void
  onCancel?: () => void
  initial?: ApplicationDraft
  optimistic?: boolean
  onSubmitted?: (draft: ApplicationDraft, done: Promise<Application>) => void
  onDirty?: (dirty: boolean) => void
  asking?: boolean
  onStay?: () => void
  saveLabel?: string
  actions?: ReactNode
  needsFolder?: boolean
  bare?: boolean
}

function problemOf(editing: Editing, shared: BindingField[], own: BindingField[], t: T): Problem | null {
  if (!editing.name.trim()) return { section: GENERAL, name: true, text: t('settings.app_nameMissing') }
  const lack = missingOf(shared, editing.bindings)
  if (lack) {
    const declared = shared.find((f) => f.capability === lack.capability)?.section?.trim()
    const text = t('settings.app_bindingMissing', lack)
    return { section: declared ? sectionKey(declared) : GENERAL, text, binding: { capability: lack.capability, text } }
  }
  for (const row of editing.repositories) {
    const gap = missingOf(own, row.value.bindings)
    if (!gap) continue
    const text = t('settings.app_bindingMissing', gap)
    return { section: REPOSITORIES, row: row.key, text, binding: { capability: gap.capability, text } }
  }
  return null
}

function flipped(set: ReadonlySet<string>, key: string): ReadonlySet<string> {
  const next = new Set(set)
  if (!next.delete(key)) next.add(key)
  return next
}

function boundIn(group: FieldGroup, bindings: Bindings): number {
  return group.fields.filter((f) => bindings[f.capability]).length
}

function useOpening(editing: Editing, shared: BindingField[]) {
  const [sections, setSections] = useState<ReadonlySet<string>>(() => {
    const declared = groupedBySection(shared).slice(1).filter((group) => boundIn(group, editing.bindings) > 0)
    return new Set([GENERAL, REPOSITORIES, ...declared.map((group) => sectionKey(group.section ?? ''))])
  })
  const [rows, setRows] = useState<ReadonlySet<string>>(() => new Set(editing.repositories.filter((r, _, all) => all.length === 1 || !r.value.directory).map((r) => r.key)))
  const toggleSection = useCallback((id: string) => setSections((s) => flipped(s, id)), [])
  const toggleRow = useCallback((key: string) => setRows((s) => flipped(s, key)), [])
  const reveal = useCallback((problem: Problem) => {
    const { section, row } = problem
    if (section) setSections((s) => new Set(s).add(section))
    if (row) setRows((s) => new Set(s).add(row))
  }, [])
  return { sections, rows, toggleSection, toggleRow, reveal }
}

function useLeaving(dirty: boolean, busy: boolean, { asking, onCancel, onStay }: EditorProps) {
  const [asked, setAsked] = useState(false)
  useEffect(() => {
    if (!dirty) return
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = ''
    }
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [dirty])
  const confirming = dirty && (asked || asking === true)
  const leave = () => {
    if (busy) return
    if (dirty) setAsked(true)
    else onCancel?.()
  }
  const stay = () => {
    setAsked(false)
    onStay?.()
  }
  return { confirming, leave, stay }
}

function useSaving({ application, optimistic, onSaved, onSubmitted }: EditorProps, original: ApplicationDraft) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const send = (editing: Editing, failed: (text: string) => void) => {
    const clean = cleanOf(editing, original)
    setBusy(true)
    const done = optimistic ? saveOptimistically(clean, application) : saveApplication(clean, application)
    onSubmitted?.(clean, done)
    done.then(
      (saved) => {
        toast.success(t('settings.app_saved', { name: saved.name }))
        if (alive.current) setBusy(false)
        onSaved(saved)
      },
      (e: unknown) => {
        if (!alive.current) return
        setBusy(false)
        failed(errorText(e))
      },
    )
  }
  return { busy, send }
}

function useFolderActions(editing: Editing, own: BindingField[], kind: string | undefined, change: (change: Change) => void) {
  const latest = useRef({ editing, own, kind })
  latest.current = { editing, own, kind }
  const suggest = useCallback((listing: DirListing, current: Bindings | undefined) => {
    const now = latest.current
    void suggested(listing.remote.ref, now.own, current, [folderName(listing.path), now.editing.name]).then((bindings) => change({ type: 'suggested', path: listing.path, bindings }))
  }, [change])
  const onFolder = useCallback((key: string, listing: DirListing) => {
    const rows = latest.current.editing.repositories
    const row = rows.find((r) => r.key === key)
    if (!row) return
    change({ type: 'repository', key, change: (r) => ({ ...r, directory: listing.path }) })
    if (rows[0]?.key === key) change({ type: 'named', path: listing.path })
    suggest(listing, row.value.bindings)
  }, [change, suggest])
  const onAdded = useCallback((paths: string[]) => {
    const now = latest.current
    const before = now.editing.repositories.map((r) => r.value)
    const next = withFolders(before, paths, now.kind)
    const fresh = next.filter((r) => paths.includes(r.directory) && !before.some((old) => old.directory === r.directory))
    change({ type: 'folders', paths, kind: now.kind })
    if (next[0]?.directory && !before[0]?.directory) change({ type: 'named', path: next[0].directory })
    for (const repository of fresh) {
      void api().dirs(repository.directory).then((listing) => suggest(listing, repository.bindings)).catch(() => undefined)
    }
  }, [change, suggest])
  return { onFolder, onAdded }
}

function useReveal(root: RefObject<HTMLDivElement>, problem: Problem | null) {
  useEffect(() => {
    if (!problem?.section) return
    const frame = requestAnimationFrame(() => root.current?.querySelector('[data-invalid="true"]')?.scrollIntoView?.({ block: 'center', behavior: 'smooth' }))
    return () => cancelAnimationFrame(frame)
  }, [root, problem])
}

function useReported(dirty: boolean, onDirty: ((dirty: boolean) => void) | undefined) {
  const report = useRef(onDirty)
  report.current = onDirty
  useEffect(() => {
    report.current?.(dirty)
    return () => report.current?.(false)
  }, [dirty])
}

function Section({ id, title, note, open, invalid, onToggle, children }: { id: string; title: string; note?: ReactNode; open: boolean; invalid?: boolean; onToggle: (id: string) => void; children: ReactNode }) {
  return (
    <section data-testid={`section-${id}`}>
      <button type="button" aria-expanded={open} onClick={() => onToggle(id)} className="flex w-full items-center gap-2 px-4 py-3 text-left outline-none transition-colors hover:bg-hover/60 focus-visible:bg-hover">
        <Icon name={open ? 'down' : 'right'} size={15} className="shrink-0 text-muted" />
        <span className="text-base font-medium text-fg">{title}</span>
        {note !== undefined && <span className="text-sm tabular-nums text-muted">{note}</span>}
        {invalid && <Icon name="alert" size={14} className="text-danger-fg" />}
      </button>
      <Collapse open={open}>
        <div className="px-4 pb-4 pt-1">{children}</div>
      </Collapse>
    </section>
  )
}

const EnvList = memo(function EnvList({ value, onChange }: { value: Variable[]; onChange: (next: Variable[]) => void }) {
  const t = useT()
  const rows = value.length > 0 ? value : [['', ''] as Variable]
  const set = (at: number, row: Variable) => onChange(rows.map((r, i) => (i === at ? row : r)))
  return (
    <div className="space-y-2">
      {rows.map(([name, val], i) => (
        <div key={i} className="flex items-center gap-1.5">
          <Input aria-label={`${t('settings.app_envName')} ${i + 1}`} className="min-w-0 flex-1 font-mono text-sm sm:max-w-[16rem]" value={name} placeholder={t('settings.app_envNamePlaceholder')} onChange={(e) => set(i, [e.target.value, val])} />
          <Input aria-label={`${t('settings.app_envValue')} ${i + 1}`} className="min-w-0 flex-1 font-mono text-sm" value={val} placeholder={t('settings.app_envValuePlaceholder')} onChange={(e) => set(i, [name, e.target.value])} />
          {(rows.length > 1 || name || val) && <IconButton tooltip={t('settings.app_removeVariable')} size="sm" onClick={() => onChange(rows.filter((_, at) => at !== i))}><Icon name="trash" size={15} /></IconButton>}
        </div>
      ))}
      <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => onChange([...rows, ['', '']])}>{t('settings.app_addVariable')}</Button>
    </div>
  )
})

const GeneralFields = memo(function GeneralFields({ name, fields, bindings, problem, focus, change }: { name: string; fields: BindingField[]; bindings: Bindings; problem: Problem | null; focus: boolean; change: (change: Change) => void }) {
  const t = useT()
  const bound = useCallback((next: Bindings) => change({ type: 'bindings', bindings: next }), [change])
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <div data-invalid={problem?.name ? 'true' : undefined}>
        <Field label={t('settings.app_name')} error={problem?.name ? problem.text : undefined}>
          <Input aria-label={t('settings.app_name')} autoFocus={focus} value={name} placeholder={t('settings.app_namePlaceholder')} onChange={(e) => change({ type: 'name', name: e.target.value })} />
        </Field>
      </div>
      <BindingFields fields={fields} bindings={bindings} problem={problem?.row ? null : problem?.binding} onChange={bound} />
    </div>
  )
})

const DeclaredFields = memo(function DeclaredFields({ group, bindings, problem, change }: { group: FieldGroup; bindings: Bindings; problem: Problem | null; change: (change: Change) => void }) {
  const bound = useCallback((next: Bindings) => change({ type: 'bindings', bindings: next }), [change])
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <BindingFields fields={group.fields} bindings={bindings} problem={problem?.row ? null : problem?.binding} onChange={bound} />
    </div>
  )
})

interface RepositoriesProps {
  rows: Row[]
  opened: ReadonlySet<string>
  fields: BindingField[]
  flows: Flows
  kinds: RepositoryKind[]
  problem: Problem | null
  change: (change: Change) => void
  onToggle: (key: string) => void
  onFolder: (key: string, listing: DirListing) => void
  onAdded: (paths: string[]) => void
}

function Repositories({ rows, opened, fields, flows, kinds, problem, change, onToggle, onFolder, onAdded }: RepositoriesProps) {
  const t = useT()
  const [adding, setAdding] = useState(false)
  const last = [...rows].reverse().find((r) => r.value.directory)?.value.directory
  const added = (paths: string[]) => {
    setAdding(false)
    onAdded(paths)
  }
  return (
    <div className="space-y-2.5">
      {rows.map((row, i) => (
        <RepositoryCard key={row.key} row={row} index={i} removable={rows.length > 1} open={opened.has(row.key)} fields={fields} flows={flows} kinds={kinds} problem={problem?.row === row.key ? (problem.binding ?? null) : null} change={change} onToggle={onToggle} onFolder={onFolder} />
      ))}
      <FolderPopover open={adding} onOpenChange={setAdding} start={last ? parentOf(last) : null} trigger={<Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />}>{t('settings.app_addRepository')}</Button>} onChoose={(listing) => added([listing.path])} onChooseMany={added} />
    </div>
  )
}

interface BarProps {
  dirty: boolean
  busy: boolean
  confirming: boolean
  problem: Problem | null
  ready: boolean
  saveLabel?: string
  actions?: ReactNode
  cancellable: boolean
  onSave: () => void
  onLeave: () => void
  onStay: () => void
  onDiscard: () => void
}

function ActionBar({ dirty, busy, confirming, problem, ready, saveLabel, actions, cancellable, onSave, onLeave, onStay, onDiscard }: BarProps) {
  const t = useT()
  return (
    <div className="sticky bottom-0 z-10 space-y-2 rounded-b-card border-t bg-surface px-4 py-3" data-testid="editor-actions">
      {problem && <p role="alert" className="text-sm text-danger-fg">{problem.text}</p>}
      {confirming ? (
        <div className="flex flex-wrap items-center justify-end gap-2">
          <span className="mr-auto text-base text-fg">{t('settings.app_discardAsk')}</span>
          <Button variant="ghost" onClick={onStay}>{t('settings.app_keepEditing')}</Button>
          <Button variant="danger" onClick={onDiscard}>{t('settings.app_discard')}</Button>
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          {actions}
          {dirty && !busy && (
            <span className="flex items-center gap-1.5 text-sm text-muted" data-testid="unsaved">
              <span aria-hidden className="size-1.5 rounded-full bg-warning" />
              {t('settings.app_unsaved')}
            </span>
          )}
          <div className="ml-auto flex items-center gap-2">
            {cancellable && <Button variant="ghost" disabled={busy} onClick={onLeave}>{t('settings.cancel')}</Button>}
            <Button variant="primary" loading={busy} disabled={!ready || busy} onClick={onSave}>{saveLabel ?? t('settings.save')}</Button>
          </div>
        </div>
      )}
    </div>
  )
}

interface SectionsProps {
  editing: Editing
  problem: Problem | null
  focus: boolean
  opening: ReturnType<typeof useOpening>
  change: (change: Change) => void
  folders: ReturnType<typeof useFolderActions>
}

function Sections({ editing, problem, focus, opening, change, folders }: SectionsProps) {
  const t = useT()
  const kinds = useKinds()
  const shared = useBindingFields('application')
  const own = useBindingFields('repository')
  const flows = useFlows(true)
  const groups = useMemo(() => groupedBySection(shared), [shared])
  const variables = useCallback((next: Variable[]) => change({ type: 'variables', variables: next }), [change])
  const { sections, rows, toggleSection, toggleRow } = opening
  const filled = editing.variables.filter(([name]) => name.trim()).length
  return (
    <div className="divide-y">
      <Section id={GENERAL} title={t('settings.app_sectionGeneral')} open={sections.has(GENERAL)} invalid={problem?.section === GENERAL} onToggle={toggleSection}>
        <GeneralFields name={editing.name} fields={groups[0].fields} bindings={editing.bindings} problem={problem} focus={focus} change={change} />
      </Section>
      {groups.slice(1).map((group) => (
        <Section key={group.section} id={sectionKey(group.section ?? '')} title={group.section ?? ''} note={`${boundIn(group, editing.bindings)}/${group.fields.length}`} open={sections.has(sectionKey(group.section ?? ''))} invalid={problem?.section === sectionKey(group.section ?? '')} onToggle={toggleSection}>
          <DeclaredFields group={group} bindings={editing.bindings} problem={problem} change={change} />
        </Section>
      ))}
      <Section id={REPOSITORIES} title={t('settings.app_sectionRepositories')} note={editing.repositories.length} open={sections.has(REPOSITORIES)} invalid={problem?.section === REPOSITORIES} onToggle={toggleSection}>
        <Repositories rows={editing.repositories} opened={rows} fields={own} flows={flows} kinds={kinds} problem={problem} change={change} onToggle={toggleRow} onFolder={folders.onFolder} onAdded={folders.onAdded} />
      </Section>
      <Section id={ENVIRONMENT} title={t('settings.app_env')} note={filled > 0 ? filled : undefined} open={sections.has(ENVIRONMENT)} onToggle={toggleSection}>
        <EnvList value={editing.variables} onChange={variables} />
      </Section>
    </div>
  )
}

export function ApplicationEditor(props: EditorProps) {
  const { application, onCancel, bare } = props
  const t = useT()
  const kinds = useKinds()
  const shared = useBindingFields('application')
  const own = useBindingFields('repository')
  const root = useRef<HTMLDivElement>(null)
  const [original] = useState(() => draftOf(application, kinds[0]?.key))
  const [baseline] = useState(() => cleanOf(editingOf(original), original))
  const [editing, dispatch] = useReducer(reduce, undefined, () => editingOf(props.initial ?? original))
  const [problem, setProblem] = useState<Problem | null>(null)
  const change = useCallback((next: Change) => {
    setProblem(null)
    dispatch(next)
  }, [])
  const dirty = useMemo(() => !sameDraft(cleanOf(editing, original), baseline), [editing, original, baseline])
  const opening = useOpening(editing, shared)
  const folders = useFolderActions(editing, own, kinds[0]?.key, change)
  const { busy, send } = useSaving(props, original)
  const { confirming, leave, stay } = useLeaving(dirty && !busy, busy, props)
  useReported(dirty && !busy, props.onDirty)
  useReveal(root, problem)
  const existing = useRef(application !== null)
  useEffect(() => {
    if (existing.current) root.current?.focus({ preventScroll: true })
  }, [])
  const fail = (found: Problem) => {
    setProblem(found)
    opening.reveal(found)
  }
  const save = () => {
    const found = problemOf(editing, shared, own, t)
    if (found) return fail(found)
    setProblem(null)
    send(editing, (text) => fail({ section: null, text }))
  }
  const keyed = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'Escape' || e.defaultPrevented || !onCancel) return
    if (!(e.target instanceof Node) || !root.current?.contains(e.target)) return
    if (confirming) stay()
    else leave()
  }
  const ready = !props.needsFolder || (editing.name.trim() !== '' && Boolean(editing.repositories[0]?.value.directory))
  const body = (
    <div ref={root} tabIndex={-1} onKeyDown={keyed} className="outline-none" data-testid="application-editor">
      {!bare && <h3 className="px-4 pb-1 pt-3.5 text-md font-medium text-fg">{application ? t('settings.app_edit') : t('settings.app_new')}</h3>}
      <Sections editing={editing} problem={problem} focus={!application} opening={opening} change={change} folders={folders} />
      <ActionBar dirty={dirty} busy={busy} confirming={confirming} problem={problem} ready={ready} saveLabel={props.saveLabel} actions={props.actions} cancellable={Boolean(onCancel)} onSave={save} onLeave={leave} onStay={stay} onDiscard={() => onCancel?.()} />
    </div>
  )
  return bare ? body : <Card className="shadow-pop">{body}</Card>
}
