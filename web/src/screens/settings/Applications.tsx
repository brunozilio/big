import { motion } from 'framer-motion'
import { useEffect, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Application, ApplicationDraft, ApplicationRepository, Binding, Bindings, DirListing, Flows } from '@/api/contract'
import { errorText, setApplications, useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Card, EmptyState } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Field, Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { stagger, tween } from '@/ui/motion'
import { Dialog } from '@/ui/overlay'
import { useT } from '@/i18n'
import { FolderDialog } from './FolderDialog'
import { FlowSelect, useFlows, withFlow } from './flows'
import { BindingFields, missingOf, suggested, useBindingFields, useBoundLook, type BindingField } from './bindings'

export function folderName(directory: string): string {
  return directory.split('/').filter(Boolean).pop() ?? directory
}

function blankRepository(): ApplicationRepository {
  return { id: '', directory: '', kind: 'application', run: [], verify: [], e2e: [], e2eDirectory: '', bindings: {} }
}

function draftOf(application: Application | null): ApplicationDraft {
  return application ? structuredClone(application) : { name: '', bindings: {}, repositories: [blankRepository()], env: {} }
}

type Variable = [string, string]

function variablesOf(env: Record<string, string> | undefined): Variable[] {
  return Object.entries(env ?? {})
}

function envOf(rows: Variable[]): Record<string, string> {
  return Object.fromEntries(rows.map(([name, value]): Variable => [name.trim(), value]).filter(([name]) => name))
}

function EnvList({ value, onChange }: { value: Variable[]; onChange: (next: Variable[]) => void }) {
  const t = useT()
  const rows = value.length > 0 ? value : [['', ''] as Variable]
  const set = (at: number, row: Variable) => onChange(rows.map((r, i) => (i === at ? row : r)))
  return (
    <div className="space-y-2">
      <span className="block text-base font-medium text-fg">{t('settings.app_env')}</span>
      <div className="space-y-2">
        {rows.map(([name, val], i) => (
          <div key={i} className="flex items-center gap-1.5">
            <Input aria-label={`${t('settings.app_envName')} ${i + 1}`} className="font-mono text-sm sm:max-w-[16rem]" value={name} placeholder="CYPRESS_BASE_URL" onChange={(e) => set(i, [e.target.value, val])} />
            <Input aria-label={`${t('settings.app_envValue')} ${i + 1}`} className="font-mono text-sm" value={val} placeholder="http://localhost:3000" onChange={(e) => set(i, [name, e.target.value])} />
            {(rows.length > 1 || name || val) && <IconButton tooltip={t('settings.app_removeVariable')} size="sm" onClick={() => onChange(rows.filter((_, at) => at !== i))}><Icon name="trash" size={15} /></IconButton>}
          </div>
        ))}
      </div>
      <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => onChange([...rows, ['', '']])}>{t('settings.app_addVariable')}</Button>
    </div>
  )
}

const KINDS = ['application', 'library', 'configuration', 'infrastructure', 'tests'] as const

type Kind = (typeof KINDS)[number]

function isKind(name: string): name is Kind {
  return (KINDS as readonly string[]).includes(name)
}

function useKindLabel(): (name: string) => string {
  const t = useT()
  return (name) => (isKind(name) ? t(`settings.app_kind_${name}`) : name)
}

function KindSelect({ value, onChange }: { value: string | undefined; onChange: (next: string) => void }) {
  const t = useT()
  const label = useKindLabel()
  const names: string[] = value && !isKind(value) ? [...KINDS, value] : [...KINDS]
  const options = names.map((name) => ({ value: name, label: label(name) }))
  return (
    <Field label={t('settings.app_kind')}>
      <Select label={t('settings.app_kind')} className="w-full" value={value || 'application'} options={options} onValueChange={onChange} />
    </Field>
  )
}

function RepositoryCard({ value, index, removable, fields, flows, onChange, onRemove, onFolder }: { value: ApplicationRepository; index: number; removable: boolean; fields: BindingField[]; flows: Flows; onChange: (next: ApplicationRepository) => void; onRemove: () => void; onFolder: () => void }) {
  const t = useT()
  return (
    <Card variant="inset" className="space-y-4 px-4 py-4" data-testid={`repository-${index}`}>
      <div className="flex items-center justify-between">
        <p className="text-base font-medium text-fg">{value.directory ? folderName(value.directory) : t('settings.app_repositoryN', { n: index + 1 })}</p>
        {removable && <IconButton tooltip={t('settings.app_removeRepository')} size="sm" onClick={onRemove}><Icon name="trash" size={15} /></IconButton>}
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <span className="text-base font-medium text-fg">{t('settings.app_directory')}</span>
          <button type="button" aria-label={t('settings.app_directory')} onClick={onFolder} className="ring-field flex h-10 w-full items-center gap-2 rounded-control bg-surface px-3 text-left shadow-card transition-colors hover:bg-hover">
            <Icon name="folder" size={16} className="text-faint" />
            <span className={value.directory ? 'min-w-0 flex-1 truncate font-mono text-sm text-fg' : 'min-w-0 flex-1 truncate text-base text-muted'}>{value.directory || t('settings.app_chooseFolder')}</span>
          </button>
        </div>
        <BindingFields fields={fields} bindings={value.bindings} onChange={(bindings) => onChange({ ...value, bindings })} />
        <KindSelect value={value.kind} onChange={(kind) => onChange({ ...value, kind })} />
        <FlowSelect value={value.flow} flows={flows} onChange={(flow) => onChange(withFlow(value, flow))} />
      </div>
    </Card>
  )
}

export function ApplicationDialog({ open, application, onClose }: { open: boolean; application: Application | null; onClose: () => void }) {
  const t = useT()
  const [draft, setDraft] = useState<ApplicationDraft>(() => draftOf(application))
  const [variables, setVariables] = useState<Variable[]>(() => variablesOf(application?.env))
  const [folderFor, setFolderFor] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const shared = useBindingFields('application')
  const own = useBindingFields('repository')
  const flows = useFlows(open)
  useEffect(() => {
    if (open) {
      setDraft(draftOf(application))
      setVariables(variablesOf(application?.env))
      setError(null)
    }
  }, [open, application])
  const change = (index: number, next: ApplicationRepository) => setDraft((d) => ({ ...d, repositories: d.repositories.map((r, i) => (i === index ? next : r)) }))
  const chosen = (listing: DirListing) => {
    if (folderFor === null) return
    const current = draft.repositories[folderFor]
    const at = folderFor
    change(at, { ...current, directory: listing.path })
    void suggested(listing.remote.ref, own, current.bindings).then((bindings) => setDraft((d) => ({ ...d, repositories: d.repositories.map((r, i) => (i === at ? { ...r, bindings: { ...bindings, ...r.bindings } } : r)) })))
    if (!draft.name.trim() && folderFor === 0) setDraft((d) => ({ ...d, name: folderName(listing.path) }))
    setFolderFor(null)
  }
  const save = () => {
    const missing = missingOf(shared, draft.bindings) ?? draft.repositories.map((r) => missingOf(own, r.bindings)).find(Boolean) ?? null
    if (missing) return setError(t('settings.app_bindingMissing', missing))
    setBusy(true)
    setError(null)
    const clean = { ...draft, env: envOf(variables) }
    const sent = application ? api().saveApplication({ ...clean, id: application.id }) : api().createApplication(clean)
    sent
      .then((saved) => {
        setApplications((list) => (list.some((x) => x.id === saved.id) ? list.map((x) => (x.id === saved.id ? saved : x)) : [...list, saved]))
        onClose()
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(o) => !o && folderFor === null && onClose()}
        size="lg"
        title={application ? t('settings.app_edit') : t('settings.app_new')}
        footer={
          <>
            <Button variant="ghost" onClick={onClose}>{t('settings.cancel')}</Button>
            <Button variant="primary" loading={busy} onClick={save}>{t('settings.save')}</Button>
          </>
        }
      >
        <div className="space-y-5">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label={t('settings.app_name')}>
              <Input aria-label={t('settings.app_name')} value={draft.name} placeholder="Checkout" onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))} />
            </Field>
            <BindingFields fields={shared} bindings={draft.bindings} onChange={(bindings) => setDraft((d) => ({ ...d, bindings }))} />
          </div>
          <EnvList value={variables} onChange={setVariables} />
          <div className="space-y-3">
            {draft.repositories.map((r, i) => (
              <RepositoryCard key={r.id || `novo-${i}`} value={r} index={i} removable={draft.repositories.length > 1} fields={own} flows={flows} onChange={(next) => change(i, next)} onRemove={() => setDraft((d) => ({ ...d, repositories: d.repositories.filter((_, at) => at !== i) }))} onFolder={() => setFolderFor(i)} />
            ))}
            <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => setDraft((d) => ({ ...d, repositories: [...d.repositories, blankRepository()] }))}>{t('settings.app_addRepository')}</Button>
          </div>
          {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
        </div>
      </Dialog>
      <FolderDialog open={open && folderFor !== null} start={folderFor === null ? null : draft.repositories[folderFor]?.directory || null} onClose={() => setFolderFor(null)} onChoose={chosen} />
    </>
  )
}

export function shortPath(directory: string): string {
  return directory.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, '~')
}

function Fact({ label, value, logo }: { label: string; value: string; logo?: ReactNode }) {
  return (
    <span className="inline-flex h-6 min-w-0 max-w-full items-center gap-1.5 rounded-full border bg-surface px-2 text-xs">
      <span className="shrink-0 text-faint">{label}</span>
      {logo}
      <span className="truncate font-medium text-fg2">{value}</span>
    </span>
  )
}

function BoundFacts({ bindings }: { bindings: Bindings | undefined }) {
  const look = useBoundLook()
  return (
    <>
      {Object.entries(bindings ?? {}).map(([capability, binding]: [string, Binding]) => {
        const { label, text, source } = look(capability, binding)
        const logo = source ? <ProviderLogo logo={source.provider.logo} name={source.provider.name} size={12} /> : undefined
        return <Fact key={capability} label={label} value={text} logo={logo} />
      })}
    </>
  )
}

function ApplicationFacts({ application }: { application: Application }) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <BoundFacts bindings={application.bindings} />
    </div>
  )
}

function CommandChips({ icon, label, commands }: { icon: 'play' | 'check' | 'sTest'; label: string; commands: string[] }) {
  return (
    <>
      {commands.map((c, i) => (
        <span key={`${c}-${i}`} title={label} className="inline-flex h-6 min-w-0 max-w-[16rem] items-center gap-1 rounded-control-sm bg-inset px-1.5 font-mono text-xs text-fg2">
          <Icon name={icon} size={11} className="shrink-0 text-faint" />
          <span className="truncate">{c}</span>
        </span>
      ))}
    </>
  )
}

function RepositoryLine({ repository }: { repository: ApplicationRepository }) {
  const t = useT()
  const kind = useKindLabel()
  const e2e = (repository.e2e ?? []).map((c) => (repository.e2eDirectory ? `${repository.e2eDirectory}: ${c}` : c))
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5 sm:flex-nowrap">
      <div className="flex min-w-0 flex-1 items-center gap-2.5">
        <span className="flex size-6 shrink-0 items-center justify-center text-fg2"><Icon name="folder" size={15} /></span>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-fg">{folderName(repository.directory)}</p>
          <p className="truncate font-mono text-xs text-faint" title={repository.directory}>{shortPath(repository.directory)}</p>
        </div>
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-1.5 sm:justify-end">
        {repository.kind && <Fact label={t('settings.app_kind')} value={kind(repository.kind)} />}
        <BoundFacts bindings={repository.bindings} />
        <CommandChips icon="play" label={t('settings.app_run')} commands={repository.run} />
        <CommandChips icon="check" label={t('settings.app_verify')} commands={repository.verify} />
        <CommandChips icon="sTest" label={t('settings.app_e2e')} commands={e2e} />
      </div>
    </li>
  )
}

function RowActions({ name, confirming, onEdit, onAsk, onCancel, onRemove }: { name: string; confirming: boolean; onEdit: () => void; onAsk: () => void; onCancel: () => void; onRemove: () => void }) {
  const t = useT()
  if (confirming) {
    return (
      <div className="flex shrink-0 items-center gap-1.5">
        <span className="hidden text-sm text-muted sm:inline">{t('settings.app_removeConfirm', { name })}</span>
        <Button variant="ghost" size="sm" onClick={onCancel}>{t('settings.cancel')}</Button>
        <Button variant="danger" size="sm" onClick={onRemove}>{t('settings.remove')}</Button>
      </div>
    )
  }
  return (
    <div className="flex shrink-0 items-center gap-0.5 opacity-100 transition-opacity duration-150 sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100">
      <IconButton tooltip={t('settings.editNamed', { name })} size="sm" onClick={onEdit}><Icon name="edit" size={15} /></IconButton>
      <IconButton tooltip={t('settings.removeNamed', { name })} size="sm" onClick={onAsk}><Icon name="trash" size={15} /></IconButton>
    </div>
  )
}

function ApplicationRow({ application, index, onEdit, onRemove }: { application: Application; index: number; onEdit: () => void; onRemove: () => void }) {
  const [confirming, setConfirming] = useState(false)
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <Card className="group overflow-hidden" data-testid={`application-${application.id}`}>
        <div className="flex min-h-14 items-center gap-3 px-4 py-2.5">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1.5">
            <button type="button" tabIndex={-1} onClick={onEdit} className="max-w-full truncate text-left text-md font-medium text-fg hover:underline">{application.name}</button>
            <ApplicationFacts application={application} />
          </div>
          <RowActions name={application.name} confirming={confirming} onEdit={onEdit} onAsk={() => setConfirming(true)} onCancel={() => setConfirming(false)} onRemove={onRemove} />
        </div>
        {application.repositories.length > 0 && (
          <ul className="divide-y border-t bg-bg/40">
            {application.repositories.map((r, i) => <RepositoryLine key={r.id || i} repository={r} />)}
          </ul>
        )}
      </Card>
    </motion.div>
  )
}

export function ApplicationsView() {
  const t = useT()
  const applications = useStore((s) => s.applications)
  const [editing, setEditing] = useState<Application | null | 'new'>(null)
  const [error, setError] = useState<string | null>(null)
  const remove = (application: Application) => {
    setError(null)
    api()
      .deleteApplication(application.id)
      .then(() => setApplications((list) => list.filter((x) => x.id !== application.id)))
      .catch((e: unknown) => setError(errorText(e)))
  }
  if (!applications) return null
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-base font-medium text-muted">{t('settings.app_title')}</h2>
        <Button size="sm" icon={<Icon name="plus" size={15} />} onClick={() => setEditing('new')}>{t('settings.app_new')}</Button>
      </div>
      {applications.length === 0 ? (
        <EmptyState size="inline" icon={<Icon name="apps" size={22} />} title={t('settings.app_empty')} text={t('settings.app_emptyText')} action={<Button size="sm" icon={<Icon name="plus" size={15} />} onClick={() => setEditing('new')}>{t('settings.app_new')}</Button>} />
      ) : (
        <div className="space-y-3">
          {applications.map((a, i) => (
            <ApplicationRow key={a.id} application={a} index={i} onEdit={() => setEditing(a)} onRemove={() => remove(a)} />
          ))}
        </div>
      )}
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
      <ApplicationDialog open={editing !== null} application={editing === 'new' ? null : editing} onClose={() => setEditing(null)} />
    </div>
  )
}
