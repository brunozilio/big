import { useEffect, useReducer, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { at } from '@/api/base'
import { ApiError, request, type LiveSource } from '@/api/http'
import { useT } from '@/i18n'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { LoginLanguage } from '../login/Login'
import { EMPTY_RUN, localNext, missingRequired, parseRunEvent, runPhase, runReducer, RUNNER, startingValues, type RunAction, type RunField, type RunForm, type RunState, type RunStep, type StepState } from './runner'

export type OpenSource = (url: string) => LiveSource

const openEventSource: OpenSource = (url) => new EventSource(url) as unknown as LiveSource

const STEP_LABEL = { waiting: 'run.stepWaiting', running: 'run.stepRunning', done: 'run.stepDone', failed: 'run.stepFailed' } as const

function StepMark({ state }: { state: StepState }) {
  if (state === 'running') return <Spinner size={16} className="text-info-fg" />
  if (state === 'done') return <Icon name="checkCircle" size={16} className="text-success-fg" />
  if (state === 'failed') return <Icon name="alert" size={16} className="text-danger-fg" />
  return <Icon name="dashedCircle" size={16} className="text-faint" />
}

function Steps({ steps }: { steps: RunStep[] }) {
  const t = useT()
  return (
    <nav aria-label={t('run.steps')} className="rounded-card bg-surface p-3 shadow-card">
      <p className="mb-2 px-1 text-sm font-medium text-muted">{t('run.steps')}</p>
      {steps.length === 0 && <p className="px-1 text-sm text-muted">{t('run.waiting')}</p>}
      <ol className="space-y-1">
        {steps.map((s) => (
          <li key={s.id} className={cn('flex items-center gap-2.5 rounded-control-sm px-1 py-1.5 text-base', s.state === 'waiting' ? 'text-muted' : 'text-fg')}>
            <span className="flex shrink-0" aria-label={t(STEP_LABEL[s.state])}>
              <StepMark state={s.state} />
            </span>
            <span className="min-w-0 flex-1">{s.title}</span>
          </li>
        ))}
      </ol>
    </nav>
  )
}

function FieldRow({ field, value, disabled, onChange }: { field: RunField; value: string; disabled: boolean; onChange: (value: string) => void }) {
  if (field.kind === 'info') {
    return (
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 rounded-control bg-inset px-3 py-2">
        <span className="text-sm text-muted">{field.label}</span>
        <span className="break-all font-mono text-sm text-fg">{field.value ?? ''}</span>
      </div>
    )
  }
  const options = field.options ?? []
  const chosen = options.find((o) => o.value === value)
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium text-fg2">{field.label}</span>
      {field.kind === 'select' ? (
        <Select
          label={field.label}
          value={value}
          onValueChange={(v) => !disabled && onChange(v)}
          options={options.map((o) => ({ value: o.value, text: o.label, label: <span className="flex flex-col"><span>{o.label}</span>{o.hint && <span className="text-xs text-muted">{o.hint}</span>}</span> }))}
        />
      ) : (
        <Input type={field.kind === 'password' ? 'password' : 'text'} autoComplete={field.kind === 'password' ? 'new-password' : 'off'} spellCheck={false} value={value} required={field.required} disabled={disabled} onChange={(e) => onChange(e.target.value)} />
      )}
      {chosen?.hint && <span className="mt-1.5 block text-sm text-muted">{chosen.hint}</span>}
      {field.hint && <span className="mt-1.5 block text-sm text-muted">{field.hint}</span>}
    </label>
  )
}

function messageOf(error: unknown, fallback: string): string {
  return error instanceof ApiError && error.message ? error.message : fallback
}

function PendingForm({ form, answered, onAnswered }: { form: RunForm; answered: boolean; onAnswered: (form: string) => void }) {
  const t = useT()
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
    <form onSubmit={submit} className="space-y-4 rounded-card bg-surface p-5 shadow-card">
      <div>
        <h2 className="text-md font-medium text-fg">{form.title}</h2>
        {form.description && <p className="mt-1 whitespace-pre-line text-sm text-muted">{form.description}</p>}
      </div>
      <fieldset disabled={locked} className="space-y-3">
        {form.fields.map((f) => (
          <FieldRow key={f.key} field={f} value={values[f.key] ?? ''} disabled={locked} onChange={(v) => setValues((all) => ({ ...all, [f.key]: v }))} />
        ))}
      </fieldset>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
      {answered && <p className="text-sm text-muted">{t('run.answered')}</p>}
      <div className="flex flex-wrap justify-end gap-2">
        {form.actions.map((a) => (
          <Button key={a.id} type={a.primary ? 'submit' : 'button'} variant={a.primary ? 'primary' : 'secondary'} loading={busy === a.id} disabled={locked || (a.primary === true && missing)} onClick={a.primary ? undefined : () => answer(a)}>
            {a.label}
          </Button>
        ))}
      </div>
    </form>
  )
}

function LogPanel({ logs, open, onToggle }: { logs: string[]; open: boolean; onToggle: () => void }) {
  const t = useT()
  const end = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const box = end.current
    if (open && box) box.scrollTop = box.scrollHeight
  }, [logs, open])
  return (
    <section className="rounded-card bg-surface shadow-card">
      <button type="button" aria-expanded={open} onClick={onToggle} className="flex w-full items-center gap-2 px-4 py-3 text-left text-sm font-medium text-fg2 hover:text-fg">
        <Icon name="terminal" size={15} />
        <span className="flex-1">{t('run.log')}</span>
        <span className="text-muted">{open ? t('run.hideLog') : t('run.showLog')}</span>
      </button>
      {open && (
        <div ref={end} role="log" className="max-h-80 overflow-auto border-t border-border px-4 py-3 font-mono text-xs leading-5 text-fg2">
          {logs.length === 0 ? <p className="text-muted">{t('run.emptyLog')}</p> : logs.map((line, i) => <div key={i} className="whitespace-pre-wrap break-all">{line || ' '}</div>)}
        </div>
      )}
    </section>
  )
}

function Banner({ tone, children }: { tone: 'danger' | 'success' | 'muted'; children: ReactNode }) {
  const look = tone === 'danger' ? 'bg-danger/10 text-danger-fg' : tone === 'success' ? 'bg-success/10 text-success-fg' : 'bg-inset text-fg2'
  return <div role={tone === 'danger' ? 'alert' : 'status'} className={cn('rounded-control px-3 py-2 text-sm', look)}>{children}</div>
}

function Outcome({ state }: { state: RunState }) {
  const t = useT()
  const phase = runPhase(state)
  if (phase === 'finished' && state.finish) {
    const next = localNext(state.finish.next)
    return (
      <div className="space-y-4 rounded-card bg-surface p-5 shadow-card">
        <div className="flex items-start gap-2.5">
          <Icon name="checkCircle" size={20} className="mt-0.5 shrink-0 text-success-fg" />
          <div>
            <p className="text-md font-medium text-fg">{t('run.finished')}</p>
            {state.finish.message && <p className="mt-1 whitespace-pre-line text-sm text-muted">{state.finish.message}</p>}
          </div>
        </div>
        <Button variant="primary" size="lg" className="w-full" onClick={() => window.location.assign(at(next))}>
          {t('run.continue')}
        </Button>
      </div>
    )
  }
  if (phase === 'failed') return <Banner tone="danger">{t('run.failed')}</Banner>
  if (phase === 'ended') return <Banner tone="muted">{t('run.ended')}</Banner>
  return null
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

export function Run({ open = openEventSource }: { open?: OpenSource }) {
  const t = useT()
  const [state, answered] = useRunner(open)
  const [logOpen, setLogOpen] = useState(true)
  const failed = runPhase(state) === 'failed'
  useEffect(() => {
    if (failed) setLogOpen(true)
  }, [failed])
  return (
    <div className="flex min-h-dvh flex-col bg-bg px-4">
      <main className="mx-auto w-full max-w-[880px] flex-1 py-12">
        <header className="mb-8">
          <h1 className="text-xl font-semibold leading-tight text-fg">{t('run.title')}</h1>
          <p className="mt-1 text-base text-muted">{t('run.subtitle')}</p>
          {state.url && <p className="mt-1 break-all font-mono text-sm text-muted">{t('run.server', { url: state.url })}</p>}
        </header>
        <div className="grid gap-4 md:grid-cols-[240px_minmax(0,1fr)] md:items-start">
          <Steps steps={state.steps} />
          <div className="min-w-0 space-y-4">
            {state.lost && <Banner tone="danger">{t('run.lost')}</Banner>}
            {state.errors.map((message, i) => <Banner key={i} tone="danger">{message}</Banner>)}
            <Outcome state={state} />
            {state.form && <PendingForm key={`${state.form.id}:${state.seq}`} form={state.form} answered={state.answered} onAnswered={answered} />}
            <LogPanel logs={state.logs} open={logOpen} onToggle={() => setLogOpen((v) => !v)} />
          </div>
        </div>
      </main>
      <footer className="flex justify-center pb-6">
        <LoginLanguage />
      </footer>
    </div>
  )
}
