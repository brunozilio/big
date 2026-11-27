import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { errorText } from '@/api/store'
import { AgentAvatar } from '@/ui/agentAvatar'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { type Draft, type LibraryItem, applied, libraryApi, touches } from './libraryData'

interface Turn {
  role: 'user' | 'assistant'
  text: string
  before?: Draft
  undone?: boolean
}

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

function Thinking() {
  const t = useT()
  return (
    <li role="status" className="flex items-center gap-2.5 text-base text-muted">
      <AgentAvatar look={LOOK} size={20} working />
      {t('settings.lib_chatThinking')}
    </li>
  )
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
  const [turns, setTurns] = useState<Turn[]>([])
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const end = useRef<HTMLDivElement>(null)

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: 'end' })
  }, [turns, busy])

  const send = (asked: string) => {
    const wanted = asked.trim()
    if (!wanted || busy) return
    const said: Turn[] = [...turns, { role: 'user', text: wanted }]
    setTurns(said)
    setText('')
    setBusy(true)
    setError(null)
    libraryApi
      .assist(item.path, draft, said.map(({ role, text: told }) => ({ role, text: told })))
      .then((answer) => {
        const changes = touches(answer)
        setTurns([...said, { role: 'assistant', text: answer.reply || t('settings.lib_chatApplied'), before: changes ? draft : undefined }])
        if (changes) onDraft(applied(draft, answer))
      })
      .catch((failure: unknown) => setError(errorText(failure)))
      .finally(() => setBusy(false))
  }

  const undo = (at: number) => {
    const turn = turns[at]
    if (!turn?.before) return
    onDraft(turn.before)
    setTurns(turns.map((other, index) => (index === at ? { ...other, undone: true } : other)))
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
            {busy && <Thinking />}
          </ul>
        )}
        {error && <p role="alert" className="mt-3 text-sm text-danger-fg">{error}</p>}
        <div ref={end} className="h-4 shrink-0" />
      </div>
      <div className="shrink-0 px-4 pb-4">
        <div className="ring-field-within flex items-end gap-2 rounded-card bg-surface p-2 shadow-card transition-shadow">
          <textarea aria-label={t('settings.lib_chatAsk')} placeholder={t('settings.lib_chatAsk')} rows={2} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={onKey} className="max-h-40 min-h-[44px] flex-1 resize-none bg-transparent px-1.5 py-1 text-base text-fg outline-none placeholder:text-muted" />
          <button type="button" aria-label={t('settings.lib_chatSend')} disabled={!text.trim() || busy} onClick={() => send(text)} className={cn('flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-primary-fg transition-[opacity,transform] duration-150 active:scale-[0.94]', (!text.trim() || busy) && 'opacity-30')}>
            <Icon name="up" size={16} strokeWidth={2} />
          </button>
        </div>
      </div>
    </section>
  )
}
