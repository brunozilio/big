import { useEffect, useId, useRef, useState } from 'react'
import { api } from '@/api'
import type { SignInPoll, SignInStart } from '@/api/contract'
import { errorText } from '@/api/store'
import { Button } from '@/ui/button'
import { Spinner } from '@/ui/display'
import { Input } from '@/ui/form'
import { Icon } from '@/ui/icon'

export type DeviceStart = Extract<SignInStart, { kind: 'device' }>
export type RedirectStart = Extract<SignInStart, { kind: 'redirect' }>

export function usePolling(flow: string | null, interval: number, onAnswer: (answer: SignInPoll) => void, onError: (message: string) => void) {
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

export function DeviceSignIn({ site, start }: { site: string; start: DeviceStart }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard?.writeText(start.code).then(() => setCopied(true), () => undefined)
  }
  return (
    <div className="space-y-3 rounded-card bg-subtle px-4 py-4" role="status" aria-live="polite">
      <p className="text-sm text-fg2">Abra a página {site}, entre com a sua conta e digite o código:</p>
      <div className="flex items-center gap-2">
        <code className="select-all rounded-md bg-surface px-3 py-1.5 font-mono text-lg font-semibold tracking-widest text-fg" aria-label="Código de verificação">
          {start.code}
        </code>
        <Button size="sm" icon={<Icon name={copied ? 'check' : 'copy'} size={14} />} onClick={copy}>
          Copiar
        </Button>
      </div>
      <a href={start.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-base font-medium text-fg underline decoration-line-strong underline-offset-4">
        {start.url.replace(/^https:\/\//, '').replace(/\/$/, '').replace(/\?.*$/, '')} <Icon name="arrowUpRight" size={14} />
      </a>
      <p className="text-sm text-muted">Aguardando a autorização…</p>
    </div>
  )
}

export function BrowserWaiting({ site }: { site: string }) {
  return (
    <div className="flex items-center gap-3 rounded-card bg-subtle px-4 py-4 text-base text-fg2">
      <Spinner size={16} />
      Autorize o acesso na página {site} que abriu no navegador…
    </div>
  )
}

function PasteSteps({ start, onPasted }: { start: RedirectStart; onPasted: (answer: SignInPoll) => void }) {
  const inputId = useId()
  const [pasted, setPasted] = useState('')
  const [busy, setBusy] = useState(false)
  const [refusal, setRefusal] = useState('')
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
    <div className="space-y-3">
      <Button asChild size="sm" variant="primary">
        <a href={start.url} target="_blank" rel="noreferrer">
          <Icon name="external" size={14} /> Abrir página de login
        </a>
      </Button>
      {start.paste && (
        <div className="space-y-2">
          <label htmlFor={inputId} className="block font-medium text-fg">
            Cole aqui o endereço para onde o navegador foi levado (ou o código mostrado)
          </label>
          <div className="flex gap-2">
            <Input id={inputId} className="flex-1" value={pasted} onChange={(e) => setPasted(e.target.value)} placeholder="http://localhost:…/callback?code=…" />
            <Button disabled={!pasted.trim() || busy} loading={busy} onClick={finish}>
              Concluir
            </Button>
          </div>
        </div>
      )}
      {refusal && (
        <p role="alert" className="flex items-start gap-1.5 text-danger-fg">
          <Icon name="alert" size={14} className="mt-0.5 shrink-0" />
          {refusal}
        </p>
      )}
    </div>
  )
}

export type AiState = { kind: 'idle' } | { kind: 'started'; start: SignInStart } | { kind: 'done' } | { kind: 'failed'; message: string }

export function ProviderSignIn({ name, state, starting, onStart, onAnswer }: { name: string; state: AiState; starting: boolean; onStart: () => void; onAnswer: (answer: SignInPoll) => void }) {
  return (
    <div className="space-y-3">
      <div className="space-y-2">
        <span className="block text-base font-medium text-fg">Conta {name}</span>
        <div className="flex flex-wrap items-center gap-3">
          <Button onClick={onStart} loading={starting && state.kind !== 'started'}>
            Entrar com {name}
          </Button>
        </div>
      </div>
      {state.kind !== 'idle' && (
        <div role="status" className="rounded-control bg-inset/70 px-3 py-2.5 text-sm text-fg2">
          {state.kind === 'done' ? (
            <span className="flex items-center gap-1.5 text-success-fg">
              <Icon name="checkCircle" size={15} /> Entrada concluída. Salve para criar a conta.
            </span>
          ) : state.kind === 'failed' ? (
            <span className="text-danger-fg">{state.message || `Não foi possível entrar com ${name}.`}</span>
          ) : state.start.kind === 'device' ? (
            <span>
              Abra{' '}
              <a className="underline" href={state.start.url} target="_blank" rel="noreferrer">
                {state.start.url}
              </a>{' '}
              e informe o código <strong className="font-mono">{state.start.code}</strong>. Esta janela atualiza sozinha quando você terminar.
            </span>
          ) : (
            <PasteSteps start={state.start} onPasted={onAnswer} />
          )}
        </div>
      )}
    </div>
  )
}
