import { useEffect, useId, useState, type ReactNode } from 'react'
import type { AskQuestion, AskResponse } from '@/api/contract'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Kbd } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { AnimatePresence, motion, springPop, springSnap, tween, tweenClose } from '@/ui/motion'
import { useT } from '@/i18n'
import type { Ev } from './events'

export type Respond = (event: string, response: AskResponse) => Promise<void>

export function ActionBlock({ tone, title, icon, children, collapsed }: { tone: 'warning' | 'danger' | 'info'; title: string; icon?: ReactNode; children: ReactNode; collapsed?: ReactNode }) {
  const color = { warning: 'text-warning-fg', danger: 'text-danger-fg', info: 'text-info-fg' }[tone]
  return (
    <AnimatePresence mode="wait" initial={false}>
      {collapsed ? (
        <motion.div key="c" initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} transition={tween} className="flex items-center gap-2 text-md text-muted">
          {collapsed}
        </motion.div>
      ) : (
        <motion.div key="o" initial={{ opacity: 0, y: 8, scale: 0.99 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, height: 0, transition: tweenClose }} transition={tween} className="space-y-3">
          <p className={cn('flex items-center gap-2 text-md font-medium', color)}>
            <motion.span initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} transition={springPop} className="flex">
              {icon ?? <Icon name="alert" size={17} strokeWidth={2} />}
            </motion.span>
            {title}
          </p>
          <div className="rounded-card bg-subtle p-5 shadow-[0_0_0_1px_rgb(var(--border))]">{children}</div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}

export function ResolvedCheck({ tone = 'success' }: { tone?: 'success' | 'danger' }) {
  return (
    <motion.span initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} transition={springPop} className={cn('flex size-[18px] items-center justify-center rounded-full text-white', tone === 'success' ? 'bg-success' : 'bg-danger')}>
      <Icon name={tone === 'success' ? 'check' : 'close'} size={11} strokeWidth={3} />
    </motion.span>
  )
}

export function AskUserCard({ questions, onSubmit, disabled }: { questions: AskQuestion[]; onSubmit: (answers: string[]) => void; disabled?: boolean }) {
  const t = useT()
  const [page, setPage] = useState(0)
  const [answers, setAnswers] = useState<(string[] | null)[]>(() => questions.map(() => null))
  const [cursor, setCursor] = useState(0)
  const [other, setOther] = useState('')
  const [direction, setDirection] = useState(1)
  const layout = useId()
  const q = questions[page]
  const optionCount = q.options.length + 1
  const selected = answers[page] ?? []

  const advance = (state: (string[] | null)[]) => {
    if (page < questions.length - 1) {
      setDirection(1)
      setPage(page + 1)
      setCursor(0)
      setOther('')
    } else {
      onSubmit(state.map((a) => (a && a.length > 0 ? a.join(', ') : '')))
    }
  }

  const choose = (label: string) => {
    if (disabled) return
    const next = [...answers]
    next[page] = q.multiSelect ? (selected.includes(label) ? selected.filter((l) => l !== label) : [...selected, label]) : [label]
    setAnswers(next)
    if (!q.multiSelect) setTimeout(() => advance(next), 220)
  }

  const skip = () => {
    const next = [...answers]
    next[page] = null
    setAnswers(next)
    advance(next)
  }

  useEffect(() => {
    if (disabled) return
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      if (target && (target.tagName === 'TEXTAREA' || target.tagName === 'INPUT')) return
      const n = Number(e.key)
      if (n >= 1 && n <= q.options.length) {
        e.preventDefault()
        setCursor(n - 1)
        choose(q.options[n - 1].label)
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        e.preventDefault()
        setCursor((c) => (c + 1) % optionCount)
      } else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        e.preventDefault()
        setCursor((c) => (c - 1 + optionCount) % optionCount)
      } else if (e.key === 'Enter') {
        e.preventDefault()
        if (cursor < q.options.length) choose(q.options[cursor].label)
        else if (q.multiSelect) advance(answers)
      } else if (e.key === 'Escape') {
        e.preventDefault()
        skip()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={tween} className="space-y-3">
      <AnimatePresence mode="wait" custom={direction} initial={false}>
        <motion.p key={page} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6, transition: tweenClose }} transition={tween} className="text-body text-fg">
          {q.question}
        </motion.p>
      </AnimatePresence>
      <div className="rounded-overlay bg-surface p-3 shadow-[0_0_0_1px_rgb(var(--border)),0_2px_8px_-2px_rgb(0_0_0/0.05)]">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={page} initial={{ opacity: 0, x: 8 * direction }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -8 * direction, transition: tweenClose }} transition={tween}>
            <div className="grid gap-1.5 sm:grid-cols-2">
              {q.options.map((o, i) => {
                const isSelected = selected.includes(o.label)
                const isCursor = cursor === i
                return (
                  <motion.button
                    key={o.label}
                    whileTap={{ scale: 0.97 }}
                    disabled={disabled}
                    onMouseEnter={() => setCursor(i)}
                    onClick={() => choose(o.label)}
                    className="relative flex min-h-[48px] items-center gap-3 rounded-card px-3 py-2 text-left disabled:opacity-60"
                  >
                    {isCursor && <motion.span layoutId={`ask-${layout}`} transition={springSnap} className="absolute inset-0 rounded-card bg-inset shadow-[0_0_0_1px_rgb(var(--border-strong))]" />}
                    <span className={cn('relative flex size-7 shrink-0 items-center justify-center rounded-control-sm text-base tabular-nums shadow-[0_0_0_1px_rgb(var(--border-strong))] transition-colors', isSelected ? 'bg-primary text-primary-fg shadow-none' : 'bg-surface text-muted')}>
                      {isSelected ? <Icon name="check" size={14} strokeWidth={2.6} /> : `${i + 1}.`}
                    </span>
                    <span className="relative min-w-0">
                      <span className={cn('block text-md', isCursor || isSelected ? 'text-fg' : 'text-fg2')}>{o.label}</span>
                      {o.description && <span className="block text-sm text-muted">{o.description}</span>}
                    </span>
                  </motion.button>
                )
              })}
            </div>
            <div className={cn('ring-field-within mt-1.5 flex items-center gap-3 rounded-card px-3 py-1.5 transition-shadow', cursor === q.options.length && 'bg-inset shadow-[0_0_0_1px_rgb(var(--border-strong))]')}>
              <span className="flex size-7 items-center justify-center rounded-control-sm text-base text-muted shadow-[0_0_0_1px_rgb(var(--border-strong))]">{q.options.length + 1}.</span>
              <input
                disabled={disabled}
                value={other}
                onChange={(e) => setOther(e.target.value)}
                onFocus={() => setCursor(q.options.length)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && other.trim()) {
                    e.preventDefault()
                    e.stopPropagation()
                    choose(other.trim())
                  }
                }}
                placeholder={t('plan.ownAnswer')}
                className="h-8 flex-1 bg-transparent text-md text-fg outline-none placeholder:text-muted"
              />
              <AnimatePresence>
                {other.trim() && (
                  <motion.span initial={{ opacity: 0, scale: 0.96 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.96, transition: tweenClose }} transition={tween}>
                    <Button size="sm" variant="primary" onClick={() => choose(other.trim())} kbd="↵">
                      {t('plan.use')}
                    </Button>
                  </motion.span>
                )}
              </AnimatePresence>
            </div>
          </motion.div>
        </AnimatePresence>
        <div className="mt-3 flex flex-wrap items-center gap-3 border-t px-1 pt-3 text-base text-muted">
          <span className="tabular-nums text-fg2">
            {page + 1} / {questions.length}
          </span>
          <span className="flex items-center gap-1.5">
            {questions.map((_, i) => (
              <motion.span key={i} initial={false} transition={tween} animate={{ width: i === page ? 16 : 6, backgroundColor: i <= page ? 'rgb(var(--text))' : 'rgb(var(--border-strong))' }} className="h-1.5 rounded-full" />
            ))}
          </span>
          <span className="hidden items-center gap-1.5 text-sm text-muted md:flex">
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd> {t('plan.toNavigate')}
          </span>
          <span className="ml-auto flex items-center gap-3">
            <button disabled={disabled} className="inline-flex items-center gap-1.5 transition-colors hover:text-fg" onClick={skip}>
              {t('plan.skipQuestion')} <Kbd>Esc</Kbd>
            </button>
            {q.multiSelect && (
              <Button size="sm" variant="primary" disabled={disabled} onClick={() => advance(answers)} kbd="↵">
                {t('plan.send')}
              </Button>
            )}
          </span>
        </div>
      </div>
    </motion.div>
  )
}


export function useSend(respond: Respond) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const send = (event: string, response: AskResponse) => {
    setBusy(true)
    setError(null)
    respond(event, response)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : t('plan.respondFailed')))
      .finally(() => setBusy(false))
  }
  return { busy, error, send }
}

export function Failure({ text }: { text: string | null }) {
  if (!text) return null
  return (
    <p role="alert" className="mt-3 flex items-center gap-1.5 text-sm text-danger-fg">
      <Icon name="alert" size={14} />
      {text}
    </p>
  )
}

export function AskBlock({ event, readOnly, respond }: { event: Ev<'ask'>; readOnly?: boolean; respond: Respond }) {
  const t = useT()
  const { busy, error, send } = useSend(respond)
  if (event.answers) {
    return (
      <ActionBlock
        tone="warning"
        title=""
        collapsed={
          <div className="w-full rounded-card bg-subtle p-5 shadow-[0_0_0_1px_rgb(var(--border))]">
            <div className="space-y-3.5">
              {event.questions.map((q, i) => (
                <div key={q.question}>
                  <p className="text-base text-muted">{q.question}</p>
                  <p className="mt-0.5 text-md text-fg">{event.answers?.[i] || t('plan.skipped')}</p>
                </div>
              ))}
            </div>
          </div>
        }
      >
        {null}
      </ActionBlock>
    )
  }
  return (
    <div>
      <AskUserCard questions={event.questions} disabled={readOnly || busy} onSubmit={(answers) => send(event.id, { kind: 'ask', answers })} />
      <Failure text={error} />
    </div>
  )
}

export function PermissionBlock({ event, readOnly, respond }: { event: Ev<'permission'>; readOnly?: boolean; respond: Respond }) {
  const t = useT()
  const { busy, error, send } = useSend(respond)
  if (event.resolution !== 'pending') return null
  const decide = (decision: 'allowed' | 'always' | 'denied') => send(event.id, { kind: 'permission', decision })
  return (
    <ActionBlock tone="warning" title={t('plan.awaitingApproval')} icon={<Icon name="hourglass" size={17} strokeWidth={2} />}>
      <div className="flex items-start gap-3">
        {event.logo ? <ProviderLogo logo={event.logo} size={34} tile /> : <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[28%] bg-surface text-fg2 shadow-card"><Icon name="terminal" size={18} /></span>}
        <div className="min-w-0 flex-1">
          <p className="text-base text-muted">
            {t('plan.wantsToUse')} <code className="rounded-sm bg-surface px-1.5 py-0.5 font-mono text-xs text-fg shadow-card">{event.tool}</code>
          </p>
          <p className="mt-1.5 max-h-64 overflow-y-auto whitespace-pre-wrap text-md text-fg [overflow-wrap:anywhere]">{event.detail}</p>
        </div>
      </div>
      <div className="mt-4 flex flex-wrap gap-2">
        <Button size="sm" variant="primary" disabled={readOnly || busy} onClick={() => decide('allowed')} kbd="↵">
          {t('plan.allowOnce')}
        </Button>
        {event.rememberable !== false && (
          <Button size="sm" disabled={readOnly || busy} onClick={() => decide('always')}>
            {t('plan.alwaysAllow')}
          </Button>
        )}
        <Button size="sm" variant="ghost" className="text-danger-fg hover:text-danger-fg" disabled={readOnly || busy} onClick={() => decide('denied')}>
          {t('plan.deny')}
        </Button>
      </div>
      <Failure text={error} />
    </ActionBlock>
  )
}
