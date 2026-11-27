import { useEffect, useReducer, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { at } from '@/api/base'
import { ApiError, request, type LiveSource } from '@/api/http'
import { useLanguage, useT } from '@/i18n'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { LoginLanguage } from '../login/Login'
import { journeyLabels, markInstalled, OnboardingShell, StageHeading } from '../onboarding/Shell'
import { EMPTY_RUN, localize, localNext, missingRequired, parseRunEvent, runPhase, runReducer, RUNNER, startingValues, type Localized, type RunAction, type RunField, type RunForm, type RunState, type RunStep, type StepState } from './runner'

export type OpenSource = (url: string) => LiveSource
export type Go = (url: string) => void

const openEventSource: OpenSource = (url) => new EventSource(url) as unknown as LiveSource

const goTo: Go = (url) => window.location.assign(url)

export const CONTINUE_MS = 1000

const STEP_LABEL = { waiting: 'run.stepWaiting', running: 'run.stepRunning', done: 'run.stepDone', failed: 'run.stepFailed' } as const

function useLocalize(): (value: Localized | undefined) => string {
  const language = useLanguage()
  return (value) => localize(value, language)
}

function StepMark({ state }: { state: StepState }) {
  if (state === 'running') return <Spinner size={16} className="text-info-fg" />
  if (state === 'done') return <Icon name="checkCircle" size={16} className="text-success-fg" />
  if (state === 'failed') return <Icon name="alert" size={16} className="text-danger-fg" />
  return <span className="size-1.5 rounded-full bg-faint" />
}

function Progress({ steps }: { steps: RunStep[] }) {
  const t = useT()
  const text = useLocalize()
  if (steps.length === 0) {
    return (
      <p className="flex items-center gap-3 text-base text-muted">
        <Spinner size={16} />
        {t('run.waiting')}
      </p>
    )
  }
  return (
    <ol aria-label={t('run.steps')} className="space-y-3">
      {steps.map((s) => (
        <li key={s.id} className={cn('flex items-center gap-3 text-base', s.state === 'waiting' ? 'text-muted' : 'text-fg')}>
          <span className="flex size-4 shrink-0 items-center justify-center" aria-label={t(STEP_LABEL[s.state])}>
            <StepMark state={s.state} />
          </span>
          <span className="min-w-0 flex-1">{text(s.title)}</span>
        </li>
      ))}
    </ol>
  )
}

function FieldRow({ field, value, disabled, onChange }: { field: RunField; value: string; disabled: boolean; onChange: (value: string) => void }) {
  const text = useLocalize()
  const label = text(field.label)
  if (field.kind === 'info') {
    return (
      <p className="text-sm text-muted">
        <span>{label}</span> <span className="break-all text-fg2">{field.value ?? ''}</span>
      </p>
    )
  }
  const options = field.options ?? []
  const chosen = options.find((o) => o.value === value)
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium text-fg2">{label}</span>
      {field.kind === 'select' ? (
        <Select
          label={label}
          value={value}
          onValueChange={(v) => !disabled && onChange(v)}
          options={options.map((o) => ({ value: o.value, text: text(o.label), label: <span className="flex flex-col"><span>{text(o.label)}</span>{o.hint && <span className="text-xs text-muted">{text(o.hint)}</span>}</span> }))}
        />
      ) : (
        <Input type={field.kind === 'password' ? 'password' : 'text'} autoComplete={field.kind === 'password' ? 'new-password' : 'off'} spellCheck={false} value={value} required={field.required} disabled={disabled} onChange={(e) => onChange(e.target.value)} />
      )}
      {chosen?.hint && <span className="mt-1.5 block text-sm text-muted">{text(chosen.hint)}</span>}
      {field.hint && <span className="mt-1.5 block text-sm text-muted">{text(field.hint)}</span>}
    </label>
  )
}

function messageOf(error: unknown, fallback: string): string {
  return error instanceof ApiError && error.message ? error.message : fallback
}

function Bar({ children }: { children: ReactNode }) {
  return <div className="mt-6 flex items-center gap-2">{children}</div>
}

function PendingForm({ form, answered, onAnswered, notes }: { form: RunForm; answered: boolean; onAnswered: (form: string) => void; notes: ReactNode }) {
  const t = useT()
  const text = useLocalize()
  const [values, setValues] = useState(() => startingValues(form))
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const locked = answered || busy !== null
  const missing = missingRequired(form, values)
  const answer = (action: RunAction) => {
    if (locked || (action.primary && missing)) return
    setBusy(action.id)
    setError(null)
    request('POST', RUNNER.answer, { form: form.id, action: action.id, values })
      .then(() => onAnswered(form.id))
      .catch((err: unknown) => setError(messageOf(err, t('run.answerFailed'))))
      .finally(() => setBusy(null))
  }
  const primary = form.actions.find((a) => a.primary)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (primary) answer(primary)
  }
  return (
    <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col">
      <StageHeading title={text(form.title)} text={text(form.description)} />
      <div className="scrollbar-thin -mx-1 min-h-0 flex-1 space-y-4 overflow-y-auto px-1 pb-2">
        {notes}
        <fieldset disabled={locked} className="space-y-4">
          {form.fields.map((f) => (
            <FieldRow key={f.key} field={f} value={values[f.key] ?? ''} disabled={locked} onChange={(v) => setValues((all) => ({ ...all, [f.key]: v }))} />
          ))}
        </fieldset>
      </div>
      {error && <p role="alert" className="mt-3 text-sm text-danger-fg">{error}</p>}
      {answered && <p role="status" className="mt-3 text-sm text-muted">{t('run.answered')}</p>}
      <Bar>
        {form.actions.map((a) => (
          <Button key={a.id} type={a.primary ? 'submit' : 'button'} variant={a.primary ? 'primary' : 'ghost'} className={a.primary ? 'ml-auto' : undefined} loading={busy === a.id} disabled={locked || (a.primary === true && missing)} onClick={a.primary ? undefined : () => answer(a)}>
            {text(a.label)}
          </Button>
        ))}
      </Bar>
    </form>
  )
}

function Notes({ state }: { state: RunState }) {
  const t = useT()
  const text = useLocalize()
  if (!state.lost && state.errors.length === 0) return null
  return (
    <div role="alert" className="space-y-1 rounded-control bg-danger/10 px-3 py-2 text-sm text-danger-fg">
      {state.lost && <p>{t('run.lost')}</p>}
      {state.errors.map((message, i) => <p key={i}>{text(message)}</p>)}
    </div>
  )
}

function useRunner(open: OpenSource): [RunState, (form: string) => void] {
  const [state, dispatch] = useReducer(runReducer, EMPTY_RUN)
  useEffect(() => {
    dispatch({ type: 'reset' })
    const source = open(at(RUNNER.events))
    source.onmessage = (e) => {
      const event = parseRunEvent(e.data)
      if (event) dispatch(event)
    }
    source.onerror = () => {
      source.close()
      dispatch({ type: 'lost' })
    }
    return () => source.close()
  }, [open])
  return [state, (form) => dispatch({ type: 'answered', form })]
}

function useContinue(state: RunState, go: Go): (() => void) | null {
  const next = state.finish ? at(localNext(state.finish.next)) : null
  const latest = useRef(go)
  latest.current = go
  useEffect(() => {
    if (!next) return
    markInstalled()
    const timer = setTimeout(() => latest.current(next), CONTINUE_MS)
    return () => clearTimeout(timer)
  }, [next])
  return next ? () => go(next) : null
}

function Working({ state, onContinue }: { state: RunState; onContinue: (() => void) | null }) {
  const t = useT()
  const text = useLocalize()
  const phase = runPhase(state)
  const heading =
    phase === 'finished' ? { title: t('run.finished'), text: text(state.finish?.message) || t('run.finishedText') }
    : phase === 'failed' ? { title: t('run.failedTitle'), text: t('run.failed') }
    : phase === 'ended' ? { title: t('run.ended') }
    : { title: t('run.title'), text: t('run.subtitle') }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <StageHeading title={heading.title} text={heading.text} />
      <div className="scrollbar-thin -mx-1 min-h-0 flex-1 space-y-5 overflow-y-auto px-1 pb-2">
        <Notes state={state} />
        <Progress steps={state.steps} />
      </div>
      {onContinue && (
        <Bar>
          <Button variant="primary" className="ml-auto" onClick={onContinue}>{t('run.continue')}</Button>
        </Bar>
      )}
    </div>
  )
}

export function Run({ open = openEventSource, go = goTo }: { open?: OpenSource; go?: Go }) {
  const t = useT()
  const [state, answered] = useRunner(open)
  const onContinue = useContinue(state, go)
  return (
    <OnboardingShell labels={journeyLabels(t)} current={0} className="min-h-dvh" language={<LoginLanguage className="w-36" />}>
      {state.form ? (
        <PendingForm key={`${state.form.id}:${state.seq}`} form={state.form} answered={state.answered} onAnswered={answered} notes={<Notes state={state} />} />
      ) : (
        <Working state={state} onContinue={onContinue} />
      )}
    </OnboardingShell>
  )
}
