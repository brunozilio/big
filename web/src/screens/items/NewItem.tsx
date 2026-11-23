import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api'
import type { Application, Connection, IssueSummary, Item, Provider } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer, type Tuning } from '@/chat/Composer'
import { Button, IconButton } from '@/ui/button'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuTrigger, Popover } from '@/ui/overlay'
import { configuredSpaces, rememberApplication, rememberedApplication, searchTargets, type Space } from './projects'

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

function ApplicationMenu({ applications, chosen, onChoose }: { applications: Application[]; chosen: Application | null; onChoose: (application: Application) => void }) {
  if (applications.length === 0 || !chosen) return null
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label={`Aplicação: ${chosen.name}`} className="flex h-8 max-w-[160px] items-center gap-1.5 rounded-full px-2.5 text-base text-fg2 transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <Icon name="apps" size={15} className="shrink-0 text-muted" />
          <span className="truncate">{chosen.name}</span>
        </button>
      </MenuTrigger>
      <MenuContent align="start" className="max-h-[320px] overflow-y-auto">
        <MenuLabel>Aplicação</MenuLabel>
        {applications.map((a) => (
          <MenuItem key={a.id} icon={<Icon name="apps" size={15} />} onSelect={() => onChoose(a)} right={a.id === chosen.id ? <Icon name="check" size={15} /> : undefined}>
            {a.name}
          </MenuItem>
        ))}
      </MenuContent>
    </Menu>
  )
}

function IssuePicker({ sources, spaces, onPick }: { sources: Source[]; spaces: Space[]; onPick: (issue: Picked) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (!open) return
    let alive = true
    const t = setTimeout(() => {
      const targets = searchTargets(sources.map((s) => s.connection.id), spaces, q)
      Promise.all(
        targets.map((target) => {
          const source = sources.find((s) => s.connection.id === target.connection)
          return api().call(target.connection, 'tracker.search', target.args).then((list) => (source ? list.map((i) => ({ ...i, source })) : []))
        }),
      )
        .then((lists) => alive && (setFound(lists.flat()), setError(null)))
        .catch((e: unknown) => alive && setError(errorText(e)))
    }, 150)
    return () => {
      alive = false
      clearTimeout(t)
    }
  }, [open, q, sources, spaces])
  const first = sources[0]
  if (!first) return null
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      align="start"
      className="w-[460px] max-w-[calc(100vw-24px)] p-1.5"
      trigger={
        <button aria-label={`Começar de uma issue do ${first.provider.name}`} className="flex h-8 min-w-8 items-center justify-center gap-1.5 rounded-full px-2 transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <ProviderLogo logo={first.provider.logo} name={first.provider.name} size={16} />
        </button>
      }
    >
      <div className="flex items-center gap-1 p-1">
        <div className="min-w-0 flex-1"><SearchInput autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Buscar issue do ${first.provider.name}`} /></div>
      </div>
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
  const applications = useStore((s) => s.applications)
  const [chosenId, setChosenId] = useState<string | null>(() => rememberedApplication())
  const chosen = useMemo(() => {
    const listed = applications ?? []
    return listed.find((a) => a.id === chosenId) ?? listed[0] ?? null
  }, [applications, chosenId])
  const choose = (next: Application) => {
    setChosenId(next.id)
    rememberApplication(next.id)
  }
  const spaces = useMemo(() => configuredSpaces(chosen ? [chosen] : [], sources.map((s) => s.connection.id)), [chosen, sources])
  const defaultModel = useStore((s) => s.models?.default ?? null)
  const [tuning, setTuning] = useState<Tuning>({ effort: 'medium', fast: false })
  const start = (text: string, attachments: string[]) => {
    if (!issue && !text) return
    setBusy(true)
    setError(null)
    api()
      .createItem({ application: chosen?.id ?? null, text, issue: issue ? { connection: issue.source.connection.id, key: issue.key } : null, model: model ?? defaultModel ?? 'auto', ...tuning, attachments })
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
        tuning={tuning}
        onTuning={setTuning}
        leading={
          <>
            <ApplicationMenu applications={applications ?? []} chosen={chosen} onChoose={choose} />
            <IssuePicker sources={sources} spaces={spaces} onPick={setIssue} />
          </>
        }
        onSubmit={(input) => start(input.text, input.attachments)}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
    </div>
  )
}
