import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api'
import type { Connection, IssueSummary, Item, Provider } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer } from '@/chat/Composer'
import { Button, IconButton } from '@/ui/button'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Popover } from '@/ui/overlay'

interface Source {
  connection: Connection
  provider: Provider
}

interface Picked extends IssueSummary {
  source: Source
}

export function useTrackers(): Source[] {
  const connections = useStore((s) => s.connections)
  return useMemo(
    () =>
      (connections?.connections ?? []).flatMap((c) => {
        const provider = connections?.providers.find((p) => p.key === c.provider)
        return c.capabilities.includes('tracker') && provider ? [{ connection: c, provider }] : []
      }),
    [connections],
  )
}

function IssuePicker({ sources, onPick }: { sources: Source[]; onPick: (issue: Picked) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (!open) return
    let alive = true
    const t = setTimeout(() => {
      Promise.all(sources.map((s) => api().call(s.connection.id, 'tracker.search', { query: q }).then((list) => list.map((i) => ({ ...i, source: s })))))
        .then((lists) => alive && (setFound(lists.flat()), setError(null)))
        .catch((e: unknown) => alive && setError(errorText(e)))
    }, 150)
    return () => {
      alive = false
      clearTimeout(t)
    }
  }, [open, q, sources])
  const first = sources[0]
  if (!first) return null
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      align="start"
      className="w-[420px] max-w-[calc(100vw-24px)] p-1.5"
      trigger={
        <button aria-label={`Começar de uma issue do ${first.provider.name}`} className="flex size-8 items-center justify-center rounded-full transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <ProviderLogo logo={first.provider.logo} name={first.provider.name} size={16} />
        </button>
      }
    >
      <div className="p-1"><SearchInput autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Buscar issue do ${first.provider.name}`} /></div>
      <ul className="mt-1 max-h-[280px] overflow-y-auto">
        {found.map((i) => (
          <li key={`${i.source.connection.id}:${i.key}`}>
            <button onClick={() => { setOpen(false); onPick(i) }} className="flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
              <StatusIcon status="todo" size={15} />
              <span className="w-[64px] shrink-0 text-base text-fg2">{i.key}</span>
              <span className="min-w-0 flex-1 truncate text-base text-fg">{i.title}</span>
            </button>
          </li>
        ))}
        {found.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{error ?? 'Nenhuma issue encontrada.'}</li>}
      </ul>
    </Popover>
  )
}

export function NewItem({ onCreated }: { onCreated: (it: Item) => void }) {
  const [busy, setBusy] = useState(false)
  const [issue, setIssue] = useState<Picked | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [model, setModel] = useState<string | null>(null)
  const sources = useTrackers()
  const defaultModel = useStore((s) => s.models?.default ?? null)
  const start = (text: string, attachments: string[]) => {
    if (!issue && !text) return
    setBusy(true)
    setError(null)
    api()
      .createItem({ text, issue: issue ? { connection: issue.source.connection.id, key: issue.key } : null, model: model ?? defaultModel ?? 'auto', attachments })
      .then((it) => {
        setItems((items) => (items.some((x) => x.id === it.id) ? items : [it, ...items]))
        setIssue(null)
        onCreated(it)
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  return (
    <div className="mb-8 space-y-2">
      <AnimatePresence initial={false}>
        {issue && (
          <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
            <ProviderLogo logo={issue.source.provider.logo} name={issue.source.provider.name} size={15} />
            <span className="text-base text-fg2">{issue.key}</span>
            <span className="min-w-0 flex-1 truncate text-base text-fg">{issue.title}</span>
            <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sPlan" size={15} />} onClick={() => start('', [])}>Planejar</Button>
            <IconButton tooltip="Remover issue" size="sm" onClick={() => setIssue(null)}><Icon name="close" size={15} /></IconButton>
          </motion.div>
        )}
      </AnimatePresence>
      <Composer
        autoFocus
        placeholder={issue ? `Instruções para planejar ${issue.key} (opcional)` : 'O que precisa ser feito?'}
        disabled={busy}
        allowEmpty={!!issue}
        model={model}
        onModel={setModel}
        leading={<IssuePicker sources={sources} onPick={setIssue} />}
        onSubmit={(input) => start(input.text, input.attachments)}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
    </div>
  )
}
