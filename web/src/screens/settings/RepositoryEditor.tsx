import { memo, useCallback, useMemo, useState } from 'react'
import type { ApplicationRepository, Bindings, DirListing, Flows, RepositoryKind } from '@/api/contract'
import { IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Field, Select, Switch } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Collapse } from '@/ui/motion'
import { useT } from '@/i18n'
import { folderName, shortPath, withAccess, type Change, type Row } from './applicationDraft'
import { kindLabel, RepositoryFacts } from './applicationFacts'
import { BindingFields, groupedBySection, type BindingField, type BindingProblem } from './bindings'
import { FolderPopover } from './FolderDialog'
import { FlowSelect, withFlow } from './flows'

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
  const options = names.map((name) => ({ value: name, label: kindLabel(kinds, name, t) }))
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

interface BodyProps {
  value: ApplicationRepository
  fields: BindingField[]
  flows: Flows
  kinds: RepositoryKind[]
  problem: BindingProblem | null
  edit: Edit
  onChoose: (listing: DirListing) => void
}

function RepositoryBody({ value, fields, flows, kinds, problem, edit, onChoose }: BodyProps) {
  const t = useT()
  const groups = useMemo(() => groupedBySection(fields), [fields])
  const bound = useCallback((bindings: Bindings) => edit((r) => ({ ...r, bindings })), [edit])
  return (
    <div className="space-y-5 border-t px-3 py-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <DirectoryField value={value.directory} onChoose={onChoose} />
        <BindingFields fields={groups[0].fields} bindings={value.bindings} problem={problem} onChange={bound} />
        <KindSelect value={value.kind} kinds={kinds} onChange={(kind) => edit((r) => ({ ...r, kind }))} />
        {!value.readOnly && <FlowSelect value={value.flow} flows={flows} onChange={(flow) => edit((r) => withFlow(r, flow))} />}
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
  flows: Flows
  kinds: RepositoryKind[]
  problem: BindingProblem | null
  change: (change: Change) => void
  onToggle: (key: string) => void
  onFolder: (key: string, listing: DirListing) => void
}

export const RepositoryCard = memo(function RepositoryCard({ row, index, removable, open, fields, flows, kinds, problem, change, onToggle, onFolder }: RepositoryProps) {
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
        <RepositoryBody value={value} fields={fields} flows={flows} kinds={kinds} problem={problem} edit={edit} onChoose={chosen} />
      </Collapse>
    </div>
  )
})
