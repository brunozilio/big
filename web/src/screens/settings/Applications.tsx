import { motion } from 'framer-motion'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Application, ApplicationDraft, ApplicationRepository, CloudAccount, CloudChoice, CodeRepo, Connection, DirListing, Provider, TrackerProject } from '@/api/contract'
import { errorText, setApplications, useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Card, EmptyState } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Checkbox, Field, Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { stagger, tween } from '@/ui/motion'
import { Dialog } from '@/ui/overlay'
import { useT } from '@/i18n'
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
export const loadTestProjects = (connection: string) => api().call(connection, 'tests.projects', {})
export const loadSecurityProjects = (connection: string) => api().call(connection, 'security.projects', {})
export const loadRepos = (connection: string) => api().call(connection, 'code.repos', {})
export const loadCloudAccounts = (connection: string) => api().call(connection, 'cloud.accounts_and_roles', {})

export function cloudOptions(accounts: CloudAccount[]): { key: string; label: string }[] {
  return accounts.flatMap((a) => {
    const name = a.name || a.id
    if (a.roles.length === 0) return [{ key: a.id, label: name }]
    return a.roles.map((role) => ({ key: `${a.id}/${role}`, label: `${name} · ${role}` }))
  })
}

export function cloudKey(chosen: CloudChoice): string {
  return choiceKey(chosen.connection, chosen.role ? `${chosen.account}/${chosen.role}` : chosen.account)
}

export function cloudChoice(value: string): CloudChoice | null {
  const picked = fromKey(value)
  if (!picked) return null
  const at = picked.key.indexOf('/')
  const account = at < 0 ? picked.key : picked.key.slice(0, at)
  return { connection: picked.connection, account, role: at < 0 ? '' : picked.key.slice(at + 1) }
}

function CloudField({ open, value, onChange }: { open: boolean; value: CloudChoice | null; onChange: (next: CloudChoice | null) => void }) {
  const t = useT()
  const clouds = useSources('cloud')
  const accounts = useListed<CloudAccount>(clouds, loadCloudAccounts, open)
  if (clouds.length === 0) return null
  const options = [
    { value: NONE, label: t('settings.app_cloudDefault') },
    ...clouds.flatMap((s) => cloudOptions(accounts[s.connection.id] ?? []).map((o) => ({ value: choiceKey(s.connection.id, o.key), label: o.label, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const current = value ? cloudKey(value) : NONE
  if (value && !options.some((o) => o.value === current)) options.push({ value: current, label: value.role ? `${value.account} · ${value.role}` : value.account })
  return (
    <Field label={t('settings.app_cloud')}>
      <Select label={t('settings.app_cloud')} className="w-full" leading={<Icon name="cloud" size={16} className="shrink-0 text-faint" />} value={current} options={options} onValueChange={(v) => onChange(cloudChoice(v))} />
    </Field>
  )
}

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
  return { id: '', code: null, directory: '', run: [], verify: [] }
}

function draftOf(application: Application | null): ApplicationDraft {
  return application ? structuredClone(application) : { name: '', tracker: null, repositories: [blankRepository()], skipDependabot: true }
}

export function projectLabel(p: TrackerProject): string {
  return p.name && p.name !== p.key ? `${p.name} · ${p.key}` : p.key
}

export function choiceKey(connection: string, key: string): string {
  return `${connection}::${key}`
}

export function fromKey(value: string): { connection: string; key: string } | null {
  if (value === NONE) return null
  const at = value.indexOf('::')
  return at < 0 ? null : { connection: value.slice(0, at), key: value.slice(at + 2) }
}

function CommandList({ label, value, placeholder, onChange }: { label: string; value: string[]; placeholder: string; onChange: (next: string[]) => void }) {
  const t = useT()
  const lines = value.length > 0 ? value : ['']
  const set = (at: number, line: string) => onChange(lines.map((l, i) => (i === at ? line : l)))
  return (
    <div className="space-y-2">
      <span className="text-base font-medium text-fg">{label}</span>
      <div className="space-y-2">
        {lines.map((line, i) => (
          <div key={i} className="flex items-center gap-1.5">
            <Input aria-label={`${label} ${i + 1}`} className="font-mono text-sm" value={line} placeholder={i === 0 ? placeholder : undefined} onChange={(e) => set(i, e.target.value)} />
            {lines.length > 1 && <IconButton tooltip={t('settings.app_removeCommand')} size="sm" onClick={() => onChange(lines.filter((_, at) => at !== i))}><Icon name="trash" size={15} /></IconButton>}
          </div>
        ))}
      </div>
      <Button variant="ghost" size="sm" icon={<Icon name="plus" size={15} />} onClick={() => onChange([...lines, ''])}>{t('settings.app_addCommand')}</Button>
    </div>
  )
}

function RepositoryCard({ value, index, removable, repos, sources, onChange, onRemove, onFolder }: { value: ApplicationRepository; index: number; removable: boolean; repos: Record<string, CodeRepo[]>; sources: Source[]; onChange: (next: ApplicationRepository) => void; onRemove: () => void; onFolder: () => void }) {
  const t = useT()
  const options = [
    { value: NONE, label: t('settings.none') },
    ...sources.flatMap((s) => (repos[s.connection.id] ?? []).map((r) => ({ value: choiceKey(s.connection.id, r.ref), label: r.ref, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const current = value.code ? choiceKey(value.code.connection, value.code.repo) : NONE
  if (value.code && !options.some((o) => o.value === current)) options.push({ value: current, label: value.code.repo })
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
        <Field label={t('settings.app_repository')}>
          <Select label={t('settings.app_repository')} className="w-full" leading={<Icon name="branch" size={16} className="shrink-0 text-faint" />} value={current} options={options} onValueChange={(v) => {
            const chosen = fromKey(v)
            onChange({ ...value, code: chosen ? { connection: chosen.connection, repo: chosen.key } : null })
          }} />
        </Field>
        <CommandList label={t('settings.app_run')} value={value.run} placeholder="bun dev" onChange={(run) => onChange({ ...value, run })} />
        <CommandList label={t('settings.app_verify')} value={value.verify} placeholder="bun test" onChange={(verify) => onChange({ ...value, verify })} />
      </div>
    </Card>
  )
}

export function ApplicationDialog({ open, application, onClose }: { open: boolean; application: Application | null; onClose: () => void }) {
  const t = useT()
  const [draft, setDraft] = useState<ApplicationDraft>(() => draftOf(application))
  const [folderFor, setFolderFor] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const trackers = useSources('tracker')
  const coders = useSources('code')
  const projects = useListed<TrackerProject>(trackers, loadProjects, open)
  const testers = useSources('tests')
  const testProjects = useListed<TrackerProject>(testers, loadTestProjects, open)
  const scanners = useSources('security')
  const scanned = useListed<TrackerProject>(scanners, loadSecurityProjects, open)
  const repos = useListed<CodeRepo>(coders, loadRepos, open)
  useEffect(() => {
    if (open) {
      setDraft(draftOf(application))
      setError(null)
    }
  }, [open, application])
  const spaces = [
    { value: NONE, label: t('settings.none') },
    ...trackers.flatMap((s) => (projects[s.connection.id] ?? []).map((p) => ({ value: choiceKey(s.connection.id, p.key), label: projectLabel(p), icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const space = draft.tracker ? choiceKey(draft.tracker.connection, draft.tracker.project) : NONE
  if (draft.tracker && !spaces.some((o) => o.value === space)) spaces.push({ value: space, label: draft.tracker.project })
  const suites = [
    { value: NONE, label: t('settings.none') },
    ...testers.flatMap((s) => (testProjects[s.connection.id] ?? []).map((p) => ({ value: choiceKey(s.connection.id, p.key), label: projectLabel(p), icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const suite = draft.tests ? choiceKey(draft.tests.connection, draft.tests.project) : NONE
  if (draft.tests && !suites.some((o) => o.value === suite)) suites.push({ value: suite, label: draft.tests.project })
  const scans = [
    { value: NONE, label: t('settings.none') },
    ...scanners.flatMap((s) => (scanned[s.connection.id] ?? []).map((p) => ({ value: choiceKey(s.connection.id, p.key), label: projectLabel(p), icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const scan = draft.security ? choiceKey(draft.security.connection, draft.security.project) : NONE
  if (draft.security && !scans.some((o) => o.value === scan)) scans.push({ value: scan, label: draft.security.project })
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
    const clean = { ...draft, repositories: draft.repositories.map((r) => ({ ...r, run: r.run.map((v) => v.trim()).filter(Boolean), verify: r.verify.map((v) => v.trim()).filter(Boolean) })) }
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
            <Field label={t('settings.app_space')}>
              <Select label={t('settings.app_space')} className="w-full" value={space} options={spaces} onValueChange={(v) => {
                const picked = fromKey(v)
                setDraft((d) => ({ ...d, tracker: picked ? { connection: picked.connection, project: picked.key } : null }))
              }} />
            </Field>
            {testers.length > 0 && (
              <Field label={t('settings.app_tests')}>
                <Select label={t('settings.app_tests')} className="w-full" value={suite} options={suites} onValueChange={(v) => {
                  const picked = fromKey(v)
                  setDraft((d) => ({ ...d, tests: picked ? { connection: picked.connection, project: picked.key } : null }))
                }} />
              </Field>
            )}
            {scanners.length > 0 && (
              <Field label={t('settings.app_security')}>
                <Select label={t('settings.app_security')} className="w-full" value={scan} options={scans} onValueChange={(v) => {
                  const picked = fromKey(v)
                  setDraft((d) => ({ ...d, security: picked ? { connection: picked.connection, project: picked.key } : null }))
                }} />
              </Field>
            )}
            <CloudField open={open} value={draft.cloud ?? null} onChange={(cloud) => setDraft((d) => ({ ...d, cloud }))} />
          </div>
          <div className="flex items-center gap-2.5">
            <span className="flex"><Checkbox label={t('settings.app_skipDependabot')} checked={draft.skipDependabot !== false} onCheckedChange={(skipDependabot) => setDraft((d) => ({ ...d, skipDependabot }))} /></span>
            <span className="cursor-pointer select-none text-base text-fg" onClick={() => setDraft((d) => ({ ...d, skipDependabot: d.skipDependabot === false }))}>{t('settings.app_skipDependabot')}</span>
          </div>
          <div className="space-y-3">
            {draft.repositories.map((r, i) => (
              <RepositoryCard key={r.id || `novo-${i}`} value={r} index={i} removable={draft.repositories.length > 1} repos={repos} sources={coders} onChange={(next) => change(i, next)} onRemove={() => setDraft((d) => ({ ...d, repositories: d.repositories.filter((_, at) => at !== i) }))} onFolder={() => setFolderFor(i)} />
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

function ApplicationFacts({ application }: { application: Application }) {
  const t = useT()
  const trackers = useSources('tracker')
  const testers = useSources('tests')
  const scanners = useSources('security')
  const clouds = useSources('cloud')
  const logoOf = (sources: Source[], connection: string | undefined) => {
    const found = sources.find((s) => s.connection.id === connection)
    return found ? <ProviderLogo logo={found.provider.logo} name={found.provider.name} size={12} /> : undefined
  }
  const { tracker, tests, security, cloud } = application
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {tracker && <Fact label={t('settings.app_space')} value={tracker.project} logo={logoOf(trackers, tracker.connection)} />}
      {tests && <Fact label={t('settings.app_tests')} value={tests.project} logo={logoOf(testers, tests.connection)} />}
      {security && <Fact label={t('settings.app_security')} value={security.project} logo={logoOf(scanners, security.connection)} />}
      {cloud && <Fact label={t('settings.app_cloudShort')} value={cloud.role ? `${cloud.account} · ${cloud.role}` : cloud.account} logo={logoOf(clouds, cloud.connection)} />}
      {application.skipDependabot !== false && <Fact label="" value={t('settings.app_dependabotSkipped')} logo={<Icon name="shield" size={12} className="text-faint" />} />}
    </div>
  )
}

function CommandChips({ icon, label, commands }: { icon: 'play' | 'check'; label: string; commands: string[] }) {
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
  const coders = useSources('code')
  const coder = coders.find((s) => s.connection.id === repository.code?.connection)
  const empty = repository.run.length === 0 && repository.verify.length === 0
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5 sm:flex-nowrap">
      <div className="flex min-w-0 flex-1 items-center gap-2.5">
        <span className="flex size-6 shrink-0 items-center justify-center text-fg2">
          {coder ? <ProviderLogo logo={coder.provider.logo} name={coder.provider.name} size={15} /> : <Icon name="folder" size={15} />}
        </span>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-fg">{repository.code?.repo ?? folderName(repository.directory)}</p>
          <p className="truncate font-mono text-xs text-faint" title={repository.directory}>{shortPath(repository.directory)}</p>
        </div>
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-1.5 sm:justify-end">
        <CommandChips icon="play" label={t('settings.app_run')} commands={repository.run} />
        <CommandChips icon="check" label={t('settings.app_verify')} commands={repository.verify} />
        {empty && <span className="text-xs text-faint">{t('settings.app_noCommands')}</span>}
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
