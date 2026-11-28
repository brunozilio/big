import { memo, useCallback, useEffect, useMemo, useState } from 'react'
import type { ApplicationRepository, Bindings, CodeRepo, DirListing, RepositoryKind } from '@/api/contract'
import { api } from '@/api'
import { useStore } from '@/api/store'
import { IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Field, Select, Switch } from '@/ui/form'
import { useAdmin, useShared } from '@/api/me'
import { Icon } from '@/ui/icon'
import { Collapse } from '@/ui/motion'
import { useT } from '@/i18n'
import { folderName, shortPath, withAccess, type Change, type Row } from './applicationDraft'
import { kindLabel, RepositoryFacts } from './applicationFacts'
import { BindingFields, groupedBySection, type BindingField, type BindingProblem } from './bindings'
import { FolderPopover } from './FolderDialog'

type Edit = (change: (repository: ApplicationRepository) => ApplicationRepository) => void

function AccessSwitch({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  const t = useT()
  return (
    <label title={t('settings.app_readOnlyHint')} className="flex shrink-0 cursor-pointer items-center gap-2 rounded-control-sm px-1.5 py-1 text-sm text-fg2 hover:bg-hover">
      <Switch checked={value} label={t('settings.app_readOnly')} onCheckedChange={onChange} />
      <span>{t('settings.app_readOnly')}</span>
    </label>
  )
}

function KindSelect({ value, kinds, onChange }: { value: string | undefined; kinds: RepositoryKind[]; onChange: (next: string) => void }) {
  const t = useT()
  const names = value && !kinds.some((k) => k.key === value) ? [...kinds.map((k) => k.key), value] : kinds.map((k) => k.key)
  if (names.length === 0) return null
  const options = names.map((name) => ({ value: name, label: kindLabel(kinds, name) }))
  return (
    <Field label={t('settings.app_kind')}>
      <Select label={t('settings.app_kind')} className="w-full" value={value || kinds[0]?.key || ''} options={options} onValueChange={onChange} />
    </Field>
  )
}

export function DirectoryField({ value, onChoose }: { value: string; onChoose: (listing: DirListing) => void }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const trigger = (
    <button type="button" aria-label={t('settings.app_directory')} className="ring-field flex h-10 w-full items-center gap-2 rounded-control bg-surface px-3 text-left shadow-card transition-colors hover:bg-hover">
      <Icon name="folder" size={16} className="text-faint" />
      <span className={value ? 'min-w-0 flex-1 truncate font-mono text-sm text-fg' : 'min-w-0 flex-1 truncate text-base text-muted'}>{value || t('settings.app_chooseFolder')}</span>
    </button>
  )
  return (
    <div className="space-y-2">
      <span className="block text-base font-medium text-fg">{t('settings.app_directory')}</span>
      <FolderPopover open={open} onOpenChange={setOpen} start={value || null} trigger={trigger} onChoose={(listing) => {
        setOpen(false)
        onChoose(listing)
      }} />
    </div>
  )
}

function useCodeRepos(connection: string | undefined): CodeRepo[] {
  const [repos, setRepos] = useState<CodeRepo[]>([])
  useEffect(() => {
    if (!connection) return
    let alive = true
    void api().call(connection, 'code.repos', {}).then((listed) => {
      if (alive) setRepos(listed as CodeRepo[])
    }).catch(() => {
      if (alive) setRepos([])
    })
    return () => {
      alive = false
    }
  }, [connection])
  return repos
}

export function RemoteField({ value, onChange }: { value: string; onChange: (remote: string) => void }) {
  const t = useT()
  const connections = useStore((s) => s.connections)?.connections ?? []
  const repos = useCodeRepos(connections.find((c) => c.capabilities.includes('code'))?.id)
  const options = useMemo(() => {
    const listed = repos.filter((repo) => repo.url).map((repo) => ({ value: repo.url as string, label: repo.ref }))
    return value && !listed.some((option) => option.value === value) ? [...listed, { value, label: value }] : listed
  }, [repos, value])
  return (
    <Field label={t('settings.app_remote')}>
      <Select label={t('settings.app_remote')} className="w-full" value={value} options={options} placeholder={t('settings.app_remotePlaceholder')} onValueChange={onChange} />
    </Field>
  )
}

interface BodyProps {
  value: ApplicationRepository
  fields: BindingField[]
  kinds: RepositoryKind[]
  problem: BindingProblem | null
  edit: Edit
  onChoose: (listing: DirListing) => void
}

function RepositoryBody({ value, fields, kinds, problem, edit, onChoose }: BodyProps) {
  const t = useT()
  const groups = useMemo(() => groupedBySection(fields), [fields])
  const bound = useCallback((bindings: Bindings) => edit((r) => ({ ...r, bindings })), [edit])
  const shared = useShared()
  const admin = useAdmin()
  return (
    <div className="space-y-5 border-t px-3 py-4">
      <div className="grid gap-4 sm:grid-cols-2">
        {shared && <RemoteField value={value.remote ?? ''} onChange={(remote) => edit((r) => ({ ...r, remote }))} />}
        {admin && !shared && <DirectoryField value={value.directory} onChoose={onChoose} />}
        <BindingFields fields={groups[0].fields} bindings={value.bindings} problem={problem} onChange={bound} />
        <KindSelect value={value.kind} kinds={kinds} onChange={(kind) => edit((r) => ({ ...r, kind }))} />
      </div>
      {value.readOnly && <p className="text-sm text-muted">{t('settings.app_readOnlyHint')}</p>}
      {groups.slice(1).map((group) => (
        <div key={group.section} className="space-y-3" data-testid={`repository-section-${group.section}`}>
          <p className="text-sm font-medium text-muted">{group.section}</p>
          <div className="grid gap-4 sm:grid-cols-2">
            <BindingFields fields={group.fields} bindings={value.bindings} problem={problem} onChange={bound} />
          </div>
        </div>
      ))}
    </div>
  )
}

export interface RepositoryProps {
  row: Row
  index: number
  removable: boolean
  open: boolean
  fields: BindingField[]
  kinds: RepositoryKind[]
  problem: BindingProblem | null
  change: (change: Change) => void
  onToggle: (key: string) => void
  onFolder: (key: string, listing: DirListing) => void
}

export const RepositoryCard = memo(function RepositoryCard({ row, index, removable, open, fields, kinds, problem, change, onToggle, onFolder }: RepositoryProps) {
  const t = useT()
  const { key, value } = row
  const edit = useCallback<Edit>((next) => change({ type: 'repository', key, change: next }), [change, key])
  const chosen = useCallback((listing: DirListing) => onFolder(key, listing), [onFolder, key])
  return (
    <div className={cn('rounded-card bg-inset', problem && 'shadow-[inset_0_0_0_1px_rgb(var(--danger)/0.45)]')} data-testid={`repository-${index}`}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2.5">
        <button type="button" aria-expanded={open} onClick={() => onToggle(key)} className="flex min-w-[min(100%,11rem)] flex-[1_1_11rem] items-center gap-2 rounded-control-sm text-left outline-none focus-visible:bg-hover">
          <Icon name={open ? 'down' : 'right'} size={15} className="shrink-0 text-muted" />
          <span className="min-w-0">
            <span className="block truncate text-base font-medium text-fg">{value.directory ? folderName(value.directory) : t('settings.app_repositoryN', { n: index + 1 })}</span>
            {value.directory && <span className="block truncate font-mono text-xs text-faint" title={value.directory}>{shortPath(value.directory)}</span>}
          </span>
        </button>
        {!open && (
          <div className="flex min-w-0 max-w-full flex-wrap items-center gap-1.5 sm:justify-end">
            <RepositoryFacts repository={value} />
          </div>
        )}
        <AccessSwitch value={value.readOnly === true} onChange={(readOnly) => edit((r) => withAccess(r, readOnly))} />
        {removable && <IconButton tooltip={t('settings.app_removeRepository')} size="sm" onClick={() => change({ type: 'remove', key })}><Icon name="trash" size={15} /></IconButton>}
      </div>
      <Collapse open={open}>
        <RepositoryBody value={value} fields={fields} kinds={kinds} problem={problem} edit={edit} onChoose={chosen} />
      </Collapse>
    </div>
  )
})
