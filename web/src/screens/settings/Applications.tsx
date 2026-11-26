import { motion } from 'framer-motion'
import { useCallback, useRef, useState, type ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Application, ApplicationDraft, ApplicationRepository } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Card, EmptyState, Skeleton, Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { stagger, tween } from '@/ui/motion'
import { toast } from '@/ui/toast'
import { useT } from '@/i18n'
import { ApplicationEditor } from './ApplicationEditor'
import { folderName, shortPath } from './applicationDraft'
import { BoundFacts, RepositoryFacts } from './applicationFacts'
import { removeOptimistically, usePending } from './applicationSave'

export { folderName, shortPath, withFolders } from './applicationDraft'

const EDIT = 'edit'
const NEW = 'new'

function RepositoryLine({ repository }: { repository: ApplicationRepository }) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5">
      <div className="flex min-w-[min(100%,12rem)] flex-[1_1_12rem] items-center gap-2.5">
        <span className="flex size-6 shrink-0 items-center justify-center text-fg2"><Icon name="folder" size={15} /></span>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-fg">{folderName(repository.directory)}</p>
          <p className="truncate font-mono text-xs text-faint" title={repository.directory}>{shortPath(repository.directory)}</p>
        </div>
      </div>
      <div className="flex min-w-0 max-w-full flex-wrap items-center gap-1.5 sm:justify-end">
        <RepositoryFacts repository={repository} />
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
    <div className="flex shrink-0 items-center gap-0.5 opacity-100 transition-opacity duration-150 group-focus-within:opacity-100 group-hover:opacity-100 [@media(hover:hover)]:opacity-60">
      <IconButton tooltip={t('settings.editNamed', { name })} size="sm" onClick={onEdit}><Icon name="edit" size={15} /></IconButton>
      <IconButton tooltip={t('settings.removeNamed', { name })} size="sm" onClick={onAsk}><Icon name="trash" size={15} /></IconButton>
    </div>
  )
}

function ApplicationRow({ application, index, onEdit, onRemove }: { application: Application; index: number; onEdit: () => void; onRemove: () => void }) {
  const t = useT()
  const [confirming, setConfirming] = useState(false)
  const pending = usePending(application.id)
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <Card className={cn('group overflow-hidden transition-opacity', pending && 'opacity-70')} data-testid={`application-${application.id}`} aria-busy={pending || undefined}>
        <div className="flex min-h-14 items-center gap-3 px-4 py-2.5">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1.5">
            <button type="button" tabIndex={-1} disabled={pending} onClick={onEdit} className="max-w-full truncate text-left text-md font-medium text-fg enabled:hover:underline">{application.name}</button>
            <div className="flex flex-wrap items-center gap-1.5">
              <BoundFacts bindings={application.bindings} />
            </div>
          </div>
          {pending ? (
            <span role="status" className="flex shrink-0 items-center gap-1.5 text-sm text-muted"><Spinner size={14} />{t('settings.app_saving')}</span>
          ) : (
            <RowActions name={application.name} confirming={confirming} onEdit={onEdit} onAsk={() => setConfirming(true)} onCancel={() => setConfirming(false)} onRemove={onRemove} />
          )}
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

function ApplicationsSkeleton() {
  const t = useT()
  return (
    <div className="space-y-4" role="status" aria-busy aria-label={t('settings.app_loading')}>
      <Skeleton className="h-4 w-28" />
      {[0, 1].map((i) => (
        <Card key={i} className="overflow-hidden">
          <div className="flex min-h-14 items-center gap-3 px-4 py-2.5">
            <Skeleton className="h-4 w-32" />
            <Skeleton className="h-5 w-24 rounded-full" />
          </div>
          <div className="flex items-center gap-3 border-t px-4 py-3">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="ml-auto h-5 w-20 rounded-full" />
          </div>
        </Card>
      ))}
    </div>
  )
}

interface Restored {
  key: string
  draft: ApplicationDraft
}

function useEditing() {
  const [search, setSearch] = useSearchParams()
  const live = useRef({ search, setSearch })
  live.current = { search, setSearch }
  const dirty = useRef(false)
  const [wanted, setWanted] = useState<string | null>(null)
  const [restored, setRestored] = useState<Restored | null>(null)
  const editing = search.get(EDIT)
  const go = useCallback((target: string | null) => {
    const next = new URLSearchParams(live.current.search)
    if (target) next.set(EDIT, target)
    else next.delete(EDIT)
    live.current.setSearch(next, { replace: true })
  }, [])
  const request = (target: string) => {
    if (target === editing) return
    if (dirty.current) setWanted(target)
    else go(target)
  }
  const leave = () => {
    dirty.current = false
    setRestored(null)
    go(wanted)
    setWanted(null)
  }
  const submitted = (key: string, draft: ApplicationDraft, done: Promise<Application>) => {
    dirty.current = false
    setRestored(null)
    setWanted(null)
    go(null)
    done.catch((e: unknown) => {
      toast.error(errorText(e))
      setRestored({ key, draft })
      if (!live.current.search.get(EDIT)) go(key)
    })
  }
  const editor = (key: string) => ({
    optimistic: true,
    initial: restored?.key === key ? restored.draft : undefined,
    asking: wanted !== null,
    onStay: () => setWanted(null),
    onCancel: leave,
    onDirty: (next: boolean) => {
      dirty.current = next
    },
    onSubmitted: (draft: ApplicationDraft, done: Promise<Application>) => submitted(key, draft, done),
    onSaved: () => undefined,
  })
  return { editing, request, editor }
}

function Editing({ children }: { children: ReactNode }) {
  return (
    <motion.div initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} transition={tween}>
      {children}
    </motion.div>
  )
}

export function ApplicationsView() {
  const t = useT()
  const applications = useStore((s) => s.applications)
  const { editing, request, editor } = useEditing()
  const remove = (application: Application) => {
    removeOptimistically(application).catch((e: unknown) => toast.error(errorText(e)))
  }
  if (!applications) return <ApplicationsSkeleton />
  const creating = editing === NEW
  const empty = applications.length === 0 && !creating
  return (
    <div className="space-y-4">
      <div className="flex min-h-8 items-center justify-between">
        <h2 className="text-base font-medium text-muted">{t('settings.app_title')}</h2>
        {!empty && <Button size="sm" disabled={creating} icon={<Icon name="plus" size={15} />} onClick={() => request(NEW)}>{t('settings.app_new')}</Button>}
      </div>
      {empty && <EmptyState className="py-14" icon={<Icon name="apps" size={28} className="mb-4 text-faint" />} title={t('settings.app_empty')} text={t('settings.app_emptyText')} action={<Button variant="primary" icon={<Icon name="plus" size={15} />} onClick={() => request(NEW)}>{t('settings.app_new')}</Button>} />}
      <div className="space-y-3">
        {creating && <Editing key={NEW}><ApplicationEditor application={null} {...editor(NEW)} /></Editing>}
        {applications.map((a, i) => editing === a.id ? (
          <Editing key={a.id}><ApplicationEditor application={a} {...editor(a.id)} /></Editing>
        ) : (
          <ApplicationRow key={a.id} application={a} index={i} onEdit={() => request(a.id)} onRemove={() => remove(a)} />
        ))}
      </div>
    </div>
  )
}
