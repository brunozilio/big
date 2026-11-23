import { useEffect, useId, useRef, useState } from 'react'
import { api, isMock } from '@/api'
import type { Provider, SignInPoll, SignInStart } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { LogoTile } from '@/ui/domain'
import { Field, Input } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Dialog } from '@/ui/overlay'

type Phase =
  | { kind: 'start' }
  | { kind: 'redirect'; start: Extract<SignInStart, { kind: 'redirect' }> }
  | { kind: 'device'; start: Extract<SignInStart, { kind: 'device' }> }
  | { kind: 'choose'; flow: string; poll: Extract<SignInPoll, { state: 'choose' }> }

function usePolling(flow: string | null, interval: number, onAnswer: (answer: SignInPoll) => void, onError: (message: string) => void) {
  const latest = useRef({ onAnswer, onError })
  latest.current = { onAnswer, onError }
  useEffect(() => {
    if (!flow) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = () => {
      api()
        .pollSignIn({ flow })
        .then((answer) => {
          if (stopped) return
          if (answer.state === 'pending') timer = setTimeout(tick, interval * 1000)
          else latest.current.onAnswer(answer)
        })
        .catch((e: unknown) => !stopped && latest.current.onError(errorText(e)))
    }
    timer = setTimeout(tick, interval * 1000)
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [flow, interval])
}

function DeviceSteps({ provider, start }: { provider: Provider; start: Extract<SignInStart, { kind: 'device' }> }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard?.writeText(start.code).then(() => setCopied(true), () => undefined)
  }
  return (
    <div className="space-y-3 rounded-card bg-subtle px-4 py-4" role="status" aria-live="polite">
      <p className="text-sm text-fg2">Abra a página do {provider.name}, entre com a sua conta e digite o código:</p>
      <div className="flex items-center gap-2">
        <code className="select-all rounded-md bg-surface px-3 py-1.5 font-mono text-lg font-semibold tracking-widest text-fg" aria-label="Código de verificação">{start.code}</code>
        <Button size="sm" icon={<Icon name={copied ? 'check' : 'copy'} size={14} />} onClick={copy}>{copied ? 'Copiado' : 'Copiar'}</Button>
      </div>
      <a href={start.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-base font-medium text-fg underline decoration-line-strong underline-offset-4">
        {start.url.replace(/^https:\/\//, '').replace(/\/$/, '')} <Icon name="arrowUpRight" size={14} />
      </a>
      <p className="text-sm text-muted">Aguardando a autorização…</p>
    </div>
  )
}

function RedirectSteps({ provider, start, onPasted }: { provider: Provider; start: Extract<SignInStart, { kind: 'redirect' }>; onPasted: (answer: SignInPoll) => void }) {
  const inputId = useId()
  const [pasted, setPasted] = useState('')
  const [busy, setBusy] = useState(false)
  const [refusal, setRefusal] = useState('')
  if (!start.paste) {
    return (
      <div className="flex items-center gap-3 rounded-card bg-subtle px-4 py-4 text-base text-fg2">
        <Spinner size={16} />
        Autorize o acesso na página do {provider.name} que abriu no navegador…
      </div>
    )
  }
  const finish = () => {
    setBusy(true)
    setRefusal('')
    api()
      .pollSignIn({ flow: start.flow, code: pasted.trim() })
      .then(onPasted)
      .catch((e: unknown) => setRefusal(errorText(e)))
      .finally(() => setBusy(false))
  }
  return (
    <div role="status" className="space-y-3 rounded-control bg-inset/70 px-3 py-2.5 text-sm text-fg2">
      <Button asChild size="sm" variant="primary">
        <a href={start.url} target="_blank" rel="noreferrer">
          <Icon name="external" size={14} /> Abrir página de login
        </a>
      </Button>
      <div className="space-y-2">
        <label htmlFor={inputId} className="block font-medium text-fg">Cole aqui o endereço para onde o navegador foi levado (ou o código mostrado)</label>
        <div className="flex gap-2">
          <Input id={inputId} className="flex-1" value={pasted} onChange={(e) => setPasted(e.target.value)} placeholder="http://localhost:…/callback?code=…" />
          <Button disabled={!pasted.trim() || busy} loading={busy} onClick={finish}>Concluir</Button>
        </div>
      </div>
      {refusal && <p role="alert" className="flex items-start gap-1.5 text-danger-fg"><Icon name="alert" size={14} className="mt-0.5 shrink-0" />{refusal}</p>}
    </div>
  )
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

export function ConnectDialog({ provider: key, onClose }: { provider: string | null; onClose: () => void }) {
  const provider = useStore((s) => s.connections?.providers.find((p) => p.key === key) ?? null)
  const all = useStore((s) => s.connections?.connections)
  const accounts = (all ?? []).filter((c) => c.provider === key)
  const [phase, setPhase] = useState<Phase>({ kind: 'start' })
  const [fields, setFields] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [choice, setChoice] = useState<string | null>(null)
  useEffect(() => {
    setPhase({ kind: 'start' })
    setChoice(null)
    setBusy(false)
    setError(null)
    setFields(Object.fromEntries((provider?.fields ?? []).map((f) => [f.name, f.value ?? ''])))
  }, [key, provider?.fields])
  const answered = (answer: SignInPoll) => {
    if (answer.state === 'done') onClose()
    else if (answer.state === 'failed') {
      setError(answer.message)
      setPhase({ kind: 'start' })
    } else if (answer.state === 'choose') setPhase({ kind: 'choose', flow: phase.kind === 'redirect' || phase.kind === 'device' ? phase.start.flow : '', poll: answer })
  }
  const flow = phase.kind === 'device' || (phase.kind === 'redirect' && !phase.start.paste) ? phase.start.flow : null
  usePolling(flow, phase.kind === 'device' ? phase.start.interval : 1, answered, (m) => {
    setError(m)
    setPhase({ kind: 'start' })
  })
  const begin = () => {
    if (!provider) return
    setBusy(true)
    setError(null)
    api()
      .startSignIn(provider.key, fields)
      .then((start) => {
        if (start.kind === 'redirect') {
          if (!start.paste && !isMock()) globalThis.open?.(start.url, '_blank', 'noopener')
          setPhase({ kind: 'redirect', start })
        } else setPhase({ kind: 'device', start })
      })
      .catch((e: unknown) => setError(errorText(e)))
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
  const connected = accounts.length > 0
  const footer = phase.kind === 'choose' ? (
    <>
      <Button variant="ghost" onClick={onClose}>Cancelar</Button>
      <Button variant="primary" loading={busy} disabled={!choice} onClick={save}>Salvar</Button>
    </>
  ) : phase.kind === 'start' ? (
    <>
      <Button variant="ghost" onClick={onClose}>Cancelar</Button>
      {(!connected || provider?.multiple) && (
        <Button variant="primary" loading={busy} onClick={begin} disabled={provider?.fields.some((f) => !fields[f.name]?.trim())}>
          {connected ? 'Adicionar conta' : provider?.fields.length ? 'Continuar' : `Entrar com ${provider?.name ?? ''}`}
          <Icon name="arrowUpRight" size={15} />
        </Button>
      )}
    </>
  ) : (
    <Button variant="ghost" onClick={onClose}>Cancelar</Button>
  )
  return (
    <Dialog open={!!provider} onOpenChange={(o) => !o && !busy && onClose()} title={provider ? `Conectar ${provider.name}` : ''} footer={footer}>
      {provider && (
        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <LogoTile logo={provider.logo} name={provider.name} size={44} />
            <p className="text-base text-muted">{provider.description}</p>
          </div>
          {phase.kind === 'start' && accounts.length > 0 && (
            <ul className="overflow-hidden rounded-card bg-surface shadow-card">
              {accounts.map((c) => (
                <li key={c.id} className="flex h-11 items-center gap-3 px-3.5 [&+&]:border-t">
                  <span className="flex size-[18px] items-center justify-center rounded-full bg-success text-white"><Icon name="check" size={11} strokeWidth={2.6} /></span>
                  <span className="min-w-0 flex-1 truncate text-base text-fg">{c.account}</span>
                  <Button size="sm" variant="ghost" onClick={() => api().disconnect(c.id).catch((e: unknown) => setError(errorText(e)))}>Desconectar</Button>
                </li>
              ))}
            </ul>
          )}
          {phase.kind === 'start' && provider.fields.length > 0 && (
            <div className="space-y-3">
              {provider.fields.map((f) => (
                <Field key={f.name} label={f.label}>
                  <Input value={fields[f.name] ?? ''} placeholder={f.placeholder} onChange={(e) => setFields((v) => ({ ...v, [f.name]: e.target.value }))} className={f.mono ? 'font-mono' : undefined} />
                </Field>
              ))}
            </div>
          )}
          {phase.kind === 'device' && <DeviceSteps provider={provider} start={phase.start} />}
          {phase.kind === 'redirect' && <RedirectSteps provider={provider} start={phase.start} onPasted={answered} />}
          {phase.kind === 'choose' && <Choices poll={phase.poll} choice={choice} onChoice={setChoice} />}
          {error && <p role="alert" className="flex items-start gap-1.5 rounded-control bg-danger-soft px-3 py-2.5 text-sm text-danger-fg"><Icon name="alert" size={14} className="mt-0.5 shrink-0" />{error}</p>}
        </div>
      )}
    </Dialog>
  )
}
