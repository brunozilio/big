import { motion } from 'framer-motion'
import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api'
import type { Application, ApplicationDraft, ApplicationRepository, CodeRepo, Connection, DirListing, Provider, TrackerProject } from '@/api/contract'
import { errorText, setApplications, useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Card, EmptyState } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Checkbox, Field, Input, Select, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { stagger, tween } from '@/ui/motion'
import { Dialog } from '@/ui/overlay'
import { FolderDialog } from './FolderDialog'

interface Source {
  connection: Connection
  provider: Provider
}

const NONE = 'none'

export function useSources(capability: string): Source[] {
  const connections = useStore((s) => s.connections)
  return useMemo(
    () =>
      (connections?.connections ?? []).flatMap((c) => {
        const provider = connections?.providers.find((p) => p.key === c.provider)
        return c.capabilities.includes(capability) && provider ? [{ connection: c, provider }] : []
      }),
    [connections, capability],
  )
}

export function useListed<T>(sources: Source[], load: (connection: string) => Promise<T[]>, open: boolean): Record<string, T[]> {
  const [listed, setListed] = useState<Record<string, T[]>>({})
  useEffect(() => {
    if (!open) return
    let alive = true
    for (const s of sources) {
      const id = s.connection.id
      load(id)
        .then((found) => alive && setListed((known) => ({ ...known, [id]: found })))
        .catch(() => alive && setListed((known) => ({ ...known, [id]: [] })))
    }
    return () => {
      alive = false
    }
  }, [open, sources, load])
  return listed
}

export const loadProjects = (connection: string) => api().call(connection, 'tracker.projects', {})
export const loadRepos = (connection: string) => api().call(connection, 'code.repos', {})

export function folderName(directory: string): string {
  return directory.split('/').filter(Boolean).pop() ?? directory
}

export function suggestedCode(ref: string | null, repos: Record<string, CodeRepo[]>): ApplicationRepository['code'] {
  if (!ref) return null
  for (const [connection, list] of Object.entries(repos)) {
    const found = list.find((r) => r.ref.toLowerCase() === ref.toLowerCase())
    if (found) return { connection, repo: found.ref }
  }
  return null
}

function blankRepository(): ApplicationRepository {
  return { id: '', code: null, directory: '', run: '', verify: [] }
}

function draftOf(application: Application | null): ApplicationDraft {
  return application ? structuredClone(application) : { name: '', tracker: null, repositories: [blankRepository()], skipDependabot: true }
}

export function choiceKey(connection: string, key: string): string {
  return `${connection}::${key}`
}

export function fromKey(value: string): { connection: string; key: string } | null {
  if (value === NONE) return null
  const at = value.indexOf('::')
  return at < 0 ? null : { connection: value.slice(0, at), key: value.slice(at + 2) }
}

function RepositoryCard({ value, index, removable, repos, sources, onChange, onRemove, onFolder }: { value: ApplicationRepository; index: number; removable: boolean; repos: Record<string, CodeRepo[]>; sources: Source[]; onChange: (next: ApplicationRepository) => void; onRemove: () => void; onFolder: () => void }) {
  const options = [
    { value: NONE, label: 'Nenhum' },
    ...sources.flatMap((s) => (repos[s.connection.id] ?? []).map((r) => ({ value: choiceKey(s.connection.id, r.ref), label: r.ref, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const current = value.code ? choiceKey(value.code.connection, value.code.repo) : NONE
  if (value.code && !options.some((o) => o.value === current)) options.push({ value: current, label: value.code.repo })
  return (
    <Card variant="inset" className="space-y-4 px-4 py-4" data-testid={`repository-${index}`}>
      <div className="flex items-center justify-between">
        <p className="text-base font-medium text-fg">{value.directory ? folderName(value.directory) : `Repositório ${index + 1}`}</p>
        {removable && <IconButton tooltip="Remover repositório" size="sm" onClick={onRemove}><Icon name="trash" size={15} /></IconButton>}
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <span className="text-base font-medium text-fg">Diretório local</span>
          <button type="button" aria-label="Diretório local" onClick={onFolder} className="ring-field flex h-10 w-full items-center gap-2 rounded-control bg-surface px-3 text-left shadow-card transition-colors hover:bg-hover">
            <Icon name="folder" size={16} className="text-faint" />
            <span className={value.directory ? 'min-w-0 flex-1 truncate font-mono text-sm text-fg' : 'min-w-0 flex-1 truncate text-base text-muted'}>{value.directory || 'Escolher pasta'}</span>
          </button>
        </div>
        <Field label="Repositório">
          <Select label="Repositório" className="w-full" value={current} options={options} onValueChange={(v) => {
            const chosen = fromKey(v)
            onChange({ ...value, code: chosen ? { connection: chosen.connection, repo: chosen.key } : null })
          }} />
        </Field>
        <Field label="Rodar">
          <Input aria-label="Rodar" className="font-mono" value={value.run} placeholder="bun dev" onChange={(e) => onChange({ ...value, run: e.target.value })} />
        </Field>
        <Field label="Verificação">
          <Textarea aria-label="Verificação" rows={2} className="font-mono text-sm" value={value.verify.join('\n')} placeholder="bun test" onChange={(e) => onChange({ ...value, verify: e.target.value.split('\n') })} />
        </Field>
      </div>
    </Card>
  )
}

export function ApplicationDialog({ open, application, onClose }: { open: boolean; application: Application | null; onClose: () => void }) {
  const [draft, setDraft] = useState<ApplicationDraft>(() => draftOf(application))
  const [folderFor, setFolderFor] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const trackers = useSources('tracker')
  const coders = useSources('code')
  const projects = useListed<TrackerProject>(trackers, loadProjects, open)
  const repos = useListed<CodeRepo>(coders, loadRepos, open)
  useEffect(() => {
    if (open) {
      setDraft(draftOf(application))
      setError(null)
    }
  }, [open, application])
  const spaces = [
    { value: NONE, label: 'Nenhum' },
    ...trackers.flatMap((s) => (projects[s.connection.id] ?? []).map((p) => ({ value: choiceKey(s.connection.id, p.key), label: `${p.name} · ${p.key}`, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const space = draft.tracker ? choiceKey(draft.tracker.connection, draft.tracker.project) : NONE
  if (draft.tracker && !spaces.some((o) => o.value === space)) spaces.push({ value: space, label: draft.tracker.project })
  const change = (index: number, next: ApplicationRepository) => setDraft((d) => ({ ...d, repositories: d.repositories.map((r, i) => (i === index ? next : r)) }))
  const chosen = (listing: DirListing) => {
    if (folderFor === null) return
    const current = draft.repositories[folderFor]
    const code = current.code ?? suggestedCode(listing.remote.ref, repos)
    change(folderFor, { ...current, directory: listing.path, code })
    if (!draft.name.trim() && folderFor === 0) setDraft((d) => ({ ...d, name: folderName(listing.path) }))
    setFolderFor(null)
  }
  const save = () => {
    setBusy(true)
    setError(null)
    const clean = { ...draft, repositories: draft.repositories.map((r) => ({ ...r, verify: r.verify.map((v) => v.trim()).filter(Boolean) })) }
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
        title={application ? 'Editar aplicação' : 'Nova aplicação'}
        footer={
          <>
            <Button variant="ghost" onClick={onClose}>Cancelar</Button>
            <Button variant="primary" loading={busy} onClick={save}>Salvar</Button>
          </>
        }
      >
        <div className="space-y-5">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Nome">
              <Input aria-label="Nome" value={draft.name} placeholder="Checkout" onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))} />
            </Field>
            <Field label="Espaço">
              <Select label="Espaço" className="w-full" value={space} options={spaces} onValueChange={(v) => {
                const picked = fromKey(v)
                setDraft((d) => ({ ...d, tracker: picked ? { connection: picked.connection, project: picked.key } : null }))
              }} />
            </Field>
          </div>
          <div className="flex items-start gap-2.5">
            <span className="mt-0.5 flex"><Checkbox label="Ignorar pipelines do dependabot[bot]" checked={draft.skipDependabot !== false} onCheckedChange={(skipDependabot) => setDraft((d) => ({ ...d, skipDependabot }))} /></span>
            <span className="flex cursor-pointer select-none flex-col gap-0.5" onClick={() => setDraft((d) => ({ ...d, skipDependabot: d.skipDependabot === false }))}>
              <span className="text-base text-fg">Ignorar pipelines do dependabot[bot]</span>
              <span className="text-sm text-muted">O CI e a release não esperam nem contam pipelines disparados pelo Dependabot.</span>
            </span>
          </div>
          <div className="space-y-3">
            {draft.repositories.map((r, i) => (
              <RepositoryCard key={r.id || `novo-${i}`} value={r} index={i} removable={draft.repositories.length > 1} repos={repos} sources={coders} onChange={(next) => change(i, next)} onRemove={() => setDraft((d) => ({ ...d, repositories: d.repositories.filter((_, at) => at !== i) }))} onFolder={() => setFolderFor(i)} />
            ))}
            <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => setDraft((d) => ({ ...d, repositories: [...d.repositories, blankRepository()] }))}>Adicionar repositório</Button>
          </div>
          {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
        </div>
      </Dialog>
      <FolderDialog open={open && folderFor !== null} start={folderFor === null ? null : draft.repositories[folderFor]?.directory || null} onClose={() => setFolderFor(null)} onChoose={chosen} />
    </>
  )
}

function ApplicationRow({ application, index, onEdit, onRemove }: { application: Application; index: number; onEdit: () => void; onRemove: () => void }) {
  const trackers = useSources('tracker')
  const coders = useSources('code')
  const tracker = trackers.find((s) => s.connection.id === application.tracker?.connection)
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <Card className="flex items-start gap-4 px-4 py-3.5">
        <span className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-control bg-inset text-fg2"><Icon name="apps" size={18} /></span>
        <div className="min-w-0 flex-1 space-y-1.5">
          <div className="flex items-center gap-2">
            <p className="truncate text-base font-medium text-fg">{application.name}</p>
            {application.tracker && (
              <span className="flex items-center gap-1.5 rounded-full bg-inset px-2 py-0.5 text-xs text-fg2">
                {tracker && <ProviderLogo logo={tracker.provider.logo} name={tracker.provider.name} size={12} />}
                {application.tracker.project}
              </span>
            )}
          </div>
          <ul className="space-y-1">
            {application.repositories.map((r) => {
              const coder = coders.find((s) => s.connection.id === r.code?.connection)
              return (
                <li key={r.id} className="flex min-w-0 items-center gap-2 text-sm text-muted">
                  {coder ? <ProviderLogo logo={coder.provider.logo} name={coder.provider.name} size={13} /> : <Icon name="folder" size={13} />}
                  <span className="shrink-0 text-fg2">{r.code?.repo ?? folderName(r.directory)}</span>
                  <span className="truncate font-mono text-xs text-faint">{r.directory}</span>
                </li>
              )
            })}
          </ul>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <IconButton tooltip={`Editar ${application.name}`} size="sm" onClick={onEdit}><Icon name="edit" size={15} /></IconButton>
          <IconButton tooltip={`Remover ${application.name}`} size="sm" onClick={onRemove}><Icon name="trash" size={15} /></IconButton>
        </div>
      </Card>
    </motion.div>
  )
}

export function ApplicationsView() {
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
        <h2 className="text-base font-medium text-muted">Aplicações</h2>
        <Button size="sm" icon={<Icon name="plus" size={15} />} onClick={() => setEditing('new')}>Nova aplicação</Button>
      </div>
      {applications.length === 0 ? (
        <EmptyState size="inline" icon={<Icon name="apps" size={22} />} title="Nenhuma aplicação" />
      ) : (
        <div className="space-y-2.5">
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
