import { useEffect, useState } from 'react'
import { api, isMock } from '@/api'
import type { Connection, FormField, Provider, SignInKind, SignInPoll } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { LogoTile } from '@/ui/domain'
import { Field, Input } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Dialog } from '@/ui/overlay'
import { BrowserWaiting, DeviceSignIn, ProviderSignIn, usePolling, type AiState, type DeviceStart, type RedirectStart } from './SignInSteps'

type Phase =
  | { kind: 'start' }
  | { kind: 'redirect'; start: RedirectStart }
  | { kind: 'device'; start: DeviceStart }
  | { kind: 'choose'; flow: string; poll: Extract<SignInPoll, { state: 'choose' }> }

interface Way {
  method?: string
  signIn: SignInKind
  fields: FormField[]
  button: string
  site: string
}

function wayOf(provider: Provider, method: string | null): Way {
  const alternative = provider.alternative
  if (method && alternative?.key === method) return { method, signIn: alternative.signIn, fields: alternative.fields, button: alternative.button, site: alternative.site }
  return { signIn: provider.signIn, fields: provider.fields, button: provider.button ?? `Entrar com ${provider.name}`, site: provider.site ?? `do ${provider.name}` }
}

function Choices({ poll, choice, onChoice }: { poll: Extract<SignInPoll, { state: 'choose' }>; choice: string | null; onChoice: (id: string) => void }) {
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium text-fg2">{poll.title}</p>
      {poll.choices.map((o) => (
        <button key={o.id} onClick={() => onChoice(o.id)} className={cn('flex w-full items-center gap-3 rounded-control px-3 py-2.5 text-left shadow-card transition-colors', choice === o.id ? 'bg-active' : 'bg-surface hover:bg-hover')}>
          <span className="min-w-0 flex-1">
            <span className="block text-base text-fg">{o.label}</span>
            <span className="block text-sm text-muted">{o.detail}</span>
          </span>
          {choice === o.id && <Icon name="check" size={16} className="text-fg" />}
        </button>
      ))}
    </div>
  )
}

function Accounts({ accounts, onError }: { accounts: Connection[]; onError: (message: string) => void }) {
  if (accounts.length === 0) return null
  return (
    <ul className="overflow-hidden rounded-card bg-surface shadow-card">
      {accounts.map((c) => (
        <li key={c.id} className="flex h-11 items-center gap-3 px-3.5 [&+&]:border-t">
          <span className="flex size-[18px] items-center justify-center rounded-full bg-success text-white"><Icon name="check" size={11} strokeWidth={2.6} /></span>
          <span className="min-w-0 flex-1 truncate text-base text-fg">{c.account}</span>
          <Button size="sm" variant="ghost" onClick={() => api().disconnect(c.id).catch((e: unknown) => onError(errorText(e)))}>Desconectar</Button>
        </li>
      ))}
    </ul>
  )
}

function Fields({ fields, values, onChange }: { fields: FormField[]; values: Record<string, string>; onChange: (name: string, value: string) => void }) {
  if (fields.length === 0) return null
  return (
    <div className="space-y-3">
      {fields.map((f) => (
        <Field key={f.name} label={f.label}>
          <Input value={values[f.name] ?? ''} placeholder={f.placeholder} onChange={(e) => onChange(f.name, e.target.value)} className={f.mono ? 'font-mono' : undefined} />
        </Field>
      ))}
    </div>
  )
}

export function ConnectDialog({ provider: key, onClose }: { provider: string | null; onClose: () => void }) {
  const provider = useStore((s) => s.connections?.providers.find((p) => p.key === key) ?? null)
  const all = useStore((s) => s.connections?.connections)
  const accounts = (all ?? []).filter((c) => c.provider === key)
  const [phase, setPhase] = useState<Phase>({ kind: 'start' })
  const [method, setMethod] = useState<string | null>(null)
  const [fields, setFields] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [choice, setChoice] = useState<string | null>(null)
  const [ai, setAi] = useState<AiState>({ kind: 'idle' })
  const way = provider ? wayOf(provider, method) : null
  useEffect(() => {
    setPhase({ kind: 'start' })
    setMethod(null)
    setChoice(null)
    setBusy(false)
    setError(null)
    setAi({ kind: 'idle' })
  }, [key])
  useEffect(() => {
    setFields(Object.fromEntries((way?.fields ?? []).map((f) => [f.name, f.value ?? ''])))
  }, [way?.fields])
  const answered = (answer: SignInPoll) => {
    if (answer.state === 'done') {
      if (provider?.ai) setAi({ kind: 'done' })
      else onClose()
    } else if (answer.state === 'failed') {
      if (provider?.ai) setAi({ kind: 'failed', message: answer.message })
      else {
        setError(answer.message)
        setPhase({ kind: 'start' })
      }
    } else if (answer.state === 'choose') setPhase({ kind: 'choose', flow: phase.kind === 'redirect' || phase.kind === 'device' ? phase.start.flow : '', poll: answer })
  }
  const aiFlow = ai.kind === 'started' && (ai.start.kind === 'device' || !ai.start.paste) ? ai.start : null
  const flow = aiFlow?.flow ?? (phase.kind === 'device' || (phase.kind === 'redirect' && !phase.start.paste) ? phase.start.flow : null)
  const interval = aiFlow?.kind === 'device' ? aiFlow.interval : phase.kind === 'device' ? phase.start.interval : 1
  usePolling(flow, interval, answered, (m) => {
    if (provider?.ai) setAi({ kind: 'failed', message: m })
    else {
      setError(m)
      setPhase({ kind: 'start' })
    }
  })
  const begin = () => {
    if (!provider || !way) return
    setBusy(true)
    setError(null)
    if (provider.ai) setAi({ kind: 'idle' })
    api()
      .startSignIn(provider.key, fields, way.method)
      .then((start) => {
        if (start.kind === 'redirect' && !start.paste && !isMock()) globalThis.open?.(start.url, '_blank', 'noopener')
        if (provider.ai) setAi({ kind: 'started', start })
        else if (start.kind === 'redirect') setPhase({ kind: 'redirect', start })
        else setPhase({ kind: 'device', start })
      })
      .catch((e: unknown) => (provider.ai ? setAi({ kind: 'failed', message: errorText(e) }) : setError(errorText(e))))
      .finally(() => setBusy(false))
  }
  const save = () => {
    if (phase.kind !== 'choose' || !choice) return
    setBusy(true)
    api()
      .pollSignIn({ flow: phase.flow, choice })
      .then(answered)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  const cancel = <Button variant="ghost" onClick={onClose}>Cancelar</Button>
  const footer = provider?.ai ? (
    <>
      {cancel}
      <Button variant="primary" disabled={ai.kind !== 'done'} onClick={onClose}>Salvar</Button>
    </>
  ) : phase.kind === 'choose' ? (
    <>
      {cancel}
      <Button variant="primary" loading={busy} disabled={!choice} onClick={save}>Salvar</Button>
    </>
  ) : phase.kind === 'start' ? (
    <>
      {cancel}
      <Button variant="primary" loading={busy} onClick={begin} disabled={way?.fields.some((f) => !fields[f.name]?.trim())}>
        {way?.button}
        <Icon name="arrowUpRight" size={15} />
      </Button>
    </>
  ) : (
    cancel
  )
  const alternative = provider?.alternative
  return (
    <Dialog open={!!provider} onOpenChange={(o) => !o && !busy && onClose()} title={provider ? `Conectar ${provider.name}` : ''} footer={footer}>
      {provider && way && (
        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <LogoTile logo={provider.logo} name={provider.name} size={44} />
            <p className="text-base text-muted">{provider.description}</p>
          </div>
          {phase.kind === 'start' && <Accounts accounts={accounts} onError={setError} />}
          {provider.ai && <ProviderSignIn name={provider.name} state={ai} starting={busy} onStart={begin} onAnswer={answered} />}
          {!provider.ai && phase.kind === 'start' && <Fields fields={way.fields} values={fields} onChange={(name, value) => setFields((v) => ({ ...v, [name]: value }))} />}
          {!provider.ai && phase.kind === 'start' && alternative && (
            <button type="button" onClick={() => setMethod(method ? null : alternative.key)} className="text-sm text-muted underline decoration-line-strong underline-offset-4 transition-colors hover:text-fg">
              {method ? `Entrar com a conta ${provider.name}` : alternative.label}
            </button>
          )}
          {phase.kind === 'device' && <DeviceSignIn site={way.site} start={phase.start} />}
          {phase.kind === 'redirect' && <BrowserWaiting site={way.site} />}
          {phase.kind === 'choose' && <Choices poll={phase.poll} choice={choice} onChoice={setChoice} />}
          {error && <p role="alert" className="flex items-start gap-1.5 rounded-control bg-danger-soft px-3 py-2.5 text-sm text-danger-fg"><Icon name="alert" size={14} className="mt-0.5 shrink-0" />{error}</p>}
        </div>
      )}
    </Dialog>
  )
}
