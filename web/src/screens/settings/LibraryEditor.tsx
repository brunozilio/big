import { lazy, Suspense, useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import { errorText } from '@/api/store'
import { hotkeyLabel } from '@/app/escape'
import { AgentAvatar, knownLook } from '@/ui/agentAvatar'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge, Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { motion, tween } from '@/ui/motion'
import { useT } from '@/i18n'
import { LibraryChat } from './LibraryChat'
import { LibraryFields } from './LibraryFields'
import { type Draft, type FieldValue, type LibraryItem, blocked, changed, draftOf, fileName, libraryApi } from './libraryData'

const CodeEditor = lazy(() => import('@/ui/codeEditor').then((m) => ({ default: m.CodeEditor })))

type Loaded = { state: 'loading' } | { state: 'failed'; message: string } | { state: 'ready'; item: LibraryItem }

type Asking = { kind: 'discard'; then: () => void } | { kind: 'restore' } | null

interface Props {
  path: string
  onPath: (path: string) => void
  onClose: () => void
  onChanged: () => void
}

const CHAT = 'library.chat'

function recalled(): boolean {
  try {
    return localStorage.getItem(CHAT) !== 'closed'
  } catch {
    return true
  }
}

function remembered(open: boolean) {
  try {
    localStorage.setItem(CHAT, open ? 'open' : 'closed')
  } catch {
    return
  }
}

const messageOf = (error: unknown, fallback: string) => errorText(error) || fallback

function useItem(path: string) {
  const t = useT()
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' })
  const [draft, setDraft] = useState<Draft | null>(null)
  const take = (item: LibraryItem) => {
    setLoaded({ state: 'ready', item })
    setDraft(draftOf(item))
  }
  useEffect(() => {
    let alive = true
    setLoaded({ state: 'loading' })
    libraryApi
      .open(path)
      .then((item) => alive && take(item))
      .catch((error: unknown) => alive && setLoaded({ state: 'failed', message: messageOf(error, t('settings.lib_loadFailed')) }))
    return () => {
      alive = false
    }
  }, [path, t])
  return { loaded, draft, setDraft, take }
}

function NewFile({ item, onPick }: { item: LibraryItem; onPick: (path: string) => void }) {
  const t = useT()
  const [name, setName] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!name?.trim()) return setName(null)
    setError(null)
    libraryApi
      .createFile(item.item, name.trim())
      .then((made) => {
        setName(null)
        onPick(made.path)
      })
      .catch((failure: unknown) => setError(messageOf(failure, t('settings.lib_saveFailed'))))
  }
  if (name === null) {
    return (
      <button type="button" onClick={() => setName('')} className="flex h-8 w-full items-center gap-2 rounded-control-sm px-2 text-left text-sm text-muted transition-colors hover:bg-hover hover:text-fg">
        <Icon name="plus" size={14} className="shrink-0" />
        {t('settings.lib_newFile')}
      </button>
    )
  }
  return (
    <form onSubmit={submit} className="px-2 py-1">
      <input autoFocus aria-label={t('settings.lib_newFile')} placeholder="examples/case.md" value={name} onChange={(e) => setName(e.target.value)} onBlur={() => !name.trim() && setName(null)} onKeyDown={(e) => e.key === 'Escape' && (e.stopPropagation(), setName(null))} className="h-7 w-full rounded-control-sm bg-surface px-2 font-mono text-xs text-fg shadow-card outline-none" />
      {error && <p role="alert" className="mt-1 text-xs text-danger-fg">{error}</p>}
    </form>
  )
}

function Files({ item, onPick }: { item: LibraryItem; onPick: (path: string) => void }) {
  const t = useT()
  const nested = item.item.split('/').length > 2
  if (item.files.length === 0 && !nested) return null
  return (
    <div className="space-y-1.5">
      <p className="text-sm font-medium text-muted">{t('settings.lib_files')}</p>
      <ul className="-mx-2">
        {[item.item, ...item.files].map((file) => (
          <li key={file}>
            <button type="button" aria-current={file === item.path ? 'true' : undefined} onClick={() => file !== item.path && onPick(file)} className={cn('flex h-8 w-full items-center gap-2 rounded-control-sm px-2 text-left text-sm transition-colors', file === item.path ? 'bg-active text-fg' : 'text-fg2 hover:bg-hover')}>
              <Icon name="file" size={14} className="shrink-0 text-muted" />
              <span className="truncate font-mono text-xs">{file.slice(item.item.length - fileName(item.item).length)}</span>
            </button>
          </li>
        ))}
        {nested && item.editable && <li><NewFile item={item} onPick={onPick} /></li>}
      </ul>
    </div>
  )
}

function Lines({ text, max }: { text: string; max?: number }) {
  const t = useT()
  const lines = text.split('\n').length
  if (!max || lines <= max) return null
  return <p role="status" className="absolute bottom-4 right-5 z-10 rounded-control-sm bg-warning-soft px-2.5 py-1 text-xs text-warning-fg">{t('settings.lib_lines', { n: lines, max })}</p>
}

function Confirm({ text, action, danger, onYes, onNo }: { text: string; action: string; danger?: boolean; onYes: () => void; onNo: () => void }) {
  const t = useT()
  return (
    <div role="alertdialog" aria-label={text} className="flex items-center gap-2">
      <span className="hidden text-sm text-muted sm:inline">{text}</span>
      <Button size="sm" variant="ghost" onClick={onNo}>{t('settings.cancel')}</Button>
      <Button size="sm" variant={danger ? 'danger' : 'secondary'} autoFocus onClick={onYes}>{action}</Button>
    </div>
  )
}

function Title({ item }: { item: LibraryItem }) {
  const t = useT()
  const look = knownLook(item.avatar)
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      {look && <AgentAvatar look={look} name={item.title} size={22} />}
      <h2 className="truncate text-base font-medium text-fg">{item.title}</h2>
      <span className="hidden truncate font-mono text-xs text-faint md:inline">{item.path}</span>
      {item.origin !== 'shipped' && <Badge tone={item.origin === 'edited' ? 'info' : 'neutral'}>{t(`settings.lib_origin_${item.origin}`)}</Badge>}
    </div>
  )
}

function Editing({ item, draft, chat, setDraft, onSave, onPick }: { item: LibraryItem; draft: Draft; chat: boolean; setDraft: (draft: Draft) => void; onSave: () => void; onPick: (path: string) => void }) {
  const fields = item.fields ?? []
  const aside = fields.length > 0 || item.files.length > 0 || item.item.split('/').length > 2
  const locked = !item.editable
  const set = (key: string, value: FieldValue) => setDraft({ ...draft, values: { ...draft.values, [key]: value } })
  const form = item.mode === 'form' && item.body
  return (
    <div className={cn('grid min-h-0 flex-1', chat && aside && 'lg:grid-cols-[360px_300px_minmax(0,1fr)]', chat && !aside && 'lg:grid-cols-[360px_minmax(0,1fr)]', !chat && aside && 'lg:grid-cols-[340px_minmax(0,1fr)]')}>
      {chat && <LibraryChat key={item.path} item={item} draft={draft} onDraft={setDraft} />}
      {aside && (
        <aside className={cn('max-h-[40dvh] space-y-6 overflow-y-auto border-b px-6 py-6 lg:max-h-none lg:border-b-0 lg:border-r', !chat && 'bg-subtle')}>
          {fields.length > 0 && <LibraryFields fields={fields} values={draft.values} disabled={locked} onChange={set} />}
          <Files item={item} onPick={onPick} />
        </aside>
      )}
      <section aria-label={form ? form.label : fileName(item.path)} className="relative flex min-h-0 min-w-0 flex-col">
        {form && <Lines text={draft.body ?? ''} max={form.maxLines} />}
        <Suspense fallback={<div className="flex flex-1 items-center justify-center"><Spinner /></div>}>
          {form ? (
            <CodeEditor key={item.path} label={form.label} language={form.language} value={draft.body ?? ''} readOnly={locked} onSave={onSave} onChange={(body) => setDraft({ ...draft, body })} />
          ) : (
            <CodeEditor key={item.path} label={fileName(item.path)} language={item.language} value={draft.source} readOnly={locked} onSave={onSave} onChange={(source) => setDraft({ ...draft, source })} />
          )}
        </Suspense>
      </section>
    </div>
  )
}

export function LibraryEditor({ path, onPath, onClose, onChanged }: Props) {
  const t = useT()
  const panel = useRef<HTMLDivElement>(null)
  const { loaded, draft, setDraft, take } = useItem(path)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState<Asking>(null)
  const item = loaded.state === 'ready' ? loaded.item : null
  const [chat, setChat] = useState(() => recalled())
  const talks = !!item?.editable && !!item.assist
  const toggle = () => {
    remembered(!chat)
    setChat(!chat)
  }
  const dirty = !!item && !!draft && changed(item, draft)

  useEffect(() => {
    panel.current?.focus()
  }, [])

  const leave = (then: () => void) => (dirty ? setAsking({ kind: 'discard', then }) : then())

  const save = () => {
    if (!item || !draft || !dirty || saving || blocked(item, draft)) return
    setSaving(true)
    setError(null)
    libraryApi
      .save(item, draft)
      .then((saved) => {
        take(saved)
        onChanged()
      })
      .catch((e: unknown) => setError(messageOf(e, t('settings.lib_saveFailed'))))
      .finally(() => setSaving(false))
  }

  const restore = () => {
    if (!item) return
    setAsking(null)
    setError(null)
    libraryApi
      .restore(item.path)
      .then(() => {
        onChanged()
        if (item.origin === 'own' && item.path === item.item) return onClose()
        if (item.origin === 'own') return onPath(item.item)
        return libraryApi.open(item.path).then(take)
      })
      .catch((e: unknown) => setError(messageOf(e, t('settings.lib_saveFailed'))))
  }

  const onKey = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return
    e.stopPropagation()
    if (asking) setAsking(null)
    else leave(onClose)
  }

  return (
    <motion.div ref={panel} role="dialog" aria-modal="true" aria-label={item?.title ?? t('settings.tab_library')} tabIndex={-1} onKeyDown={onKey} initial={{ opacity: 0, scale: 0.99 }} animate={{ opacity: 1, scale: 1 }} transition={tween} className="fixed inset-0 z-30 flex flex-col bg-bg outline-none">
      <header className="flex h-14 shrink-0 items-center gap-3 border-b px-3">
        <IconButton tooltip={t('settings.lib_back')} onClick={() => leave(onClose)}><Icon name="left" size={18} /></IconButton>
        <div className="min-w-0 flex-1">{item && <Title item={item} />}</div>
        {error && <p role="alert" className="max-w-[40%] truncate text-sm text-danger-fg" title={error}>{error}</p>}
        {asking?.kind === 'discard' && <Confirm text={t('settings.lib_discardAsk')} action={t('settings.lib_discard')} danger onYes={() => { const then = asking.then; setAsking(null); then() }} onNo={() => setAsking(null)} />}
        {asking?.kind === 'restore' && item && <Confirm text={t(item.origin === 'own' ? 'settings.lib_deleteAsk' : 'settings.lib_restoreAsk')} action={t(item.origin === 'own' ? 'settings.lib_delete' : 'settings.lib_restore')} danger onYes={restore} onNo={() => setAsking(null)} />}
        {talks && <IconButton tooltip={t('settings.lib_chat')} aria-pressed={chat} active={chat} onClick={toggle}><Icon name="sparkles" size={17} /></IconButton>}
        {!asking && item?.editable && (
          <>
            {item.origin !== 'shipped' && <Button size="sm" variant="ghost" onClick={() => setAsking({ kind: 'restore' })}>{t(item.origin === 'own' ? 'settings.lib_delete' : 'settings.lib_restore')}</Button>}
            <Button size="sm" variant="primary" disabled={!dirty || (!!item && !!draft && blocked(item, draft))} loading={saving} kbd={hotkeyLabel('s')} onClick={save}>{t('settings.save')}</Button>
          </>
        )}
        {item && !item.editable && <Badge tone="outline">{t('settings.lib_readOnly')}</Badge>}
      </header>
      {loaded.state === 'loading' && <div className="flex flex-1 items-center justify-center"><Spinner /></div>}
      {loaded.state === 'failed' && <p role="alert" className="p-6 text-sm text-danger-fg">{loaded.message}</p>}
      {item && draft && <Editing item={item} draft={draft} chat={talks && chat} setDraft={setDraft} onSave={save} onPick={(next) => leave(() => onPath(next))} />}
    </motion.div>
  )
}
