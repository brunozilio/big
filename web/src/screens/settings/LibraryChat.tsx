import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { errorText } from '@/api/store'
import { AgentAvatar } from '@/ui/agentAvatar'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { useStore } from '@/api/store'
import { Select } from '@/ui/form'
import { Tooltip } from '@/ui/overlay'
import { type ChatTurn, type Draft, type LibraryItem, applied, askedOnOpen, chooseModel, chosenModel, keepTalk, libraryApi, talkOf, touches } from './libraryData'

type Turn = ChatTurn

interface Props {
  item: LibraryItem
  draft: Draft
  onDraft: (draft: Draft) => void
}

const LOOK = { shape: 'circle' as const, color: '#8b5cf6' }

function Said({ turn, onUndo }: { turn: Turn; onUndo: () => void }) {
  const t = useT()
  if (turn.role === 'user') return <li className="ml-8 self-end whitespace-pre-wrap rounded-card bg-inset px-3 py-2 text-base text-fg">{turn.text}</li>
  return (
    <li className="flex gap-2.5">
      <AgentAvatar look={LOOK} size={20} className="mt-0.5" />
      <div className="min-w-0 flex-1 space-y-2">
        <p className="whitespace-pre-wrap text-base text-fg">{turn.text}</p>
        {turn.before && (
          <p className="flex items-center gap-2 text-sm text-muted">
            <Icon name={turn.undone ? 'undo' : 'check'} size={13} />
            <span>{t(turn.undone ? 'settings.lib_chatUndone' : 'settings.lib_chatApplied')}</span>
            {!turn.undone && <button type="button" onClick={onUndo} className="font-medium text-fg2 underline-offset-2 hover:text-fg hover:underline">{t('settings.lib_chatUndo')}</button>}
          </p>
        )}
      </div>
    </li>
  )
}

function Thinking({ live }: { live: string }) {
  const t = useT()
  return (
    <li role="status" className="flex gap-2.5 text-base">
      <AgentAvatar look={LOOK} size={20} working className="mt-0.5" />
      {live ? <p className="min-w-0 flex-1 whitespace-pre-wrap text-fg">{live}</p> : <span className="text-muted">{t('settings.lib_chatThinking')}</span>}
    </li>
  )
}

function ModelChoice({ value, onChange }: { value: string | null; onChange: (id: string) => void }) {
  const t = useT()
  const models = useStore((s) => s.models)
  if (!models || models.models.length < 2) return null
  const current = models.models.find((model) => model.id === value)?.id ?? models.default ?? models.models[0].id
  return <Select size="sm" label={t('settings.lib_chatModel')} value={current} options={models.models.map((model) => ({ value: model.id, label: model.name }))} onValueChange={onChange} className="max-w-[60%]" />
}

function Ideas({ ideas, onPick }: { ideas: string[]; onPick: (idea: string) => void }) {
  const t = useT()
  return (
    <div className="flex flex-1 flex-col justify-end gap-3 pb-1">
      <AgentAvatar look={LOOK} size={28} />
      <p className="text-md font-medium text-fg">{t('settings.lib_chatHello')}</p>
      <ul className="space-y-1.5">
        {ideas.map((idea) => (
          <li key={idea}>
            <button type="button" onClick={() => onPick(idea)} className="w-full rounded-control bg-surface px-3 py-2 text-left text-base text-fg2 shadow-card transition-colors hover:bg-hover hover:text-fg">{idea}</button>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function LibraryChat({ item, draft, onDraft }: Props) {
  const t = useT()
  const [turns, setTurns] = useState<Turn[]>(() => talkOf(item.path))
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const [live, setLive] = useState('')
  const [model, setModel] = useState(() => chosenModel())
  const [error, setError] = useState<string | null>(null)
  const end = useRef<HTMLDivElement>(null)
  const first = useRef(true)

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: 'end' })
  }, [turns, busy, live])

  const keep = (next: Turn[]) => {
    setTurns(next)
    keepTalk(item.path, next)
  }

  const send = (asked: string) => {
    const wanted = asked.trim()
    if (!wanted || busy) return
    const said: Turn[] = [...turns, { role: 'user', text: wanted }]
    keep(said)
    setText('')
    setBusy(true)
    setLive('')
    setError(null)
    libraryApi
      .assist(item.path, draft, said.map(({ role, text: told }) => ({ role, text: told })), { model, onReply: setLive })
      .then((answer) => {
        const changes = touches(answer)
        keep([...said, { role: 'assistant', text: answer.reply || t('settings.lib_chatApplied'), before: changes ? draft : undefined }])
        if (changes) onDraft(applied(draft, answer))
      })
      .catch((failure: unknown) => setError(errorText(failure)))
      .finally(() => setBusy(false))
  }

  useEffect(() => {
    if (!first.current) return
    first.current = false
    const asked = askedOnOpen(item.path)
    if (asked) send(asked)
  })

  const pick = (id: string) => {
    chooseModel(id)
    setModel(id)
  }

  const undo = (at: number) => {
    const turn = turns[at]
    if (!turn?.before) return
    onDraft(turn.before)
    keep(turns.map((other, index) => (index === at ? { ...other, undone: true } : other)))
  }

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return
    e.preventDefault()
    send(text)
  }

  return (
    <section aria-label={t('settings.lib_chat')} className="flex min-h-0 flex-col border-b bg-subtle lg:border-b-0 lg:border-r">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 pt-5">
        {turns.length === 0 ? (
          <Ideas ideas={item.assist?.ideas ?? []} onPick={send} />
        ) : (
          <ul aria-live="polite" className="flex flex-col gap-4">
            {turns.map((turn, at) => <Said key={at} turn={turn} onUndo={() => undo(at)} />)}
            {busy && <Thinking live={live} />}
          </ul>
        )}
        {error && <p role="alert" className="mt-3 text-sm text-danger-fg">{error}</p>}
        <div ref={end} className="h-4 shrink-0" />
      </div>
      <div className="shrink-0 px-4 pb-4">
        <div className="ring-field-within rounded-card bg-surface p-2 shadow-card transition-shadow">
          <textarea aria-label={t('settings.lib_chatAsk')} placeholder={t('settings.lib_chatAsk')} rows={2} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={onKey} className="max-h-40 min-h-[44px] w-full resize-none bg-transparent px-1.5 py-1 text-base text-fg outline-none placeholder:text-muted" />
          <div className="flex items-center gap-2">
            <ModelChoice value={model} onChange={pick} />
            <span className="flex-1" />
            {turns.length > 0 && !busy && (
              <Tooltip content={t('settings.lib_chatClear')}>
                <button type="button" aria-label={t('settings.lib_chatClear')} onClick={() => keep([])} className="flex size-8 items-center justify-center rounded-full text-muted transition-colors hover:bg-hover hover:text-fg"><Icon name="trash" size={15} /></button>
              </Tooltip>
            )}
            <button type="button" aria-label={t('settings.lib_chatSend')} disabled={!text.trim() || busy} onClick={() => send(text)} className={cn('flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-primary-fg transition-[opacity,transform] duration-150 active:scale-[0.94]', (!text.trim() || busy) && 'opacity-30')}>
              <Icon name="up" size={16} strokeWidth={2} />
            </button>
          </div>
        </div>
      </div>
    </section>
  )
}
