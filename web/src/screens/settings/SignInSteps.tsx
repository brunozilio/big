import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Connection, FormField, McpTool, SignInPoll, SignInStart } from '@/api/contract'
import { isMcpTool } from '@/api/guards'
import { errorText } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { LogoTile } from '@/ui/domain'
import { Field, Input } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { type T, useT } from '@/i18n'

export type DeviceStart = Extract<SignInStart, { kind: 'device' }>
export type RedirectStart = Extract<SignInStart, { kind: 'redirect' }>
export type ChoosePoll = Extract<SignInPoll, { state: 'choose' }>

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

function shortUrl(url: string) {
  return url.replace(/^https?:\/\//, '').replace(/\?.*$/, '').replace(/\/$/, '')
}

function Panel({ children }: { children: ReactNode }) {
  return <div className="space-y-3 rounded-card bg-subtle px-4 py-4" role="status" aria-live="polite">{children}</div>
}

function Waiting({ text }: { text: string }) {
  return (
    <p className="flex items-center gap-2 text-sm text-muted">
      <Spinner size={14} />
      {text}
    </p>
  )
}

function OpenLink({ url }: { url: string }) {
  return (
    <a href={url} target="_blank" rel="noreferrer" className="inline-flex max-w-full items-center gap-1 text-base font-medium text-fg underline decoration-line-strong underline-offset-4">
      <span className="truncate">{shortUrl(url)}</span> <Icon name="arrowUpRight" size={14} />
    </a>
  )
}

export function ConnectHeader({ logo, name, description }: { logo: string; name: string; description: string }) {
  return (
    <div className="flex items-center gap-3">
      <LogoTile logo={logo} name={name} size={44} />
      <div className="min-w-0">
        <p className="text-md font-medium text-fg">{name}</p>
        <p className="truncate text-base text-muted">{description}</p>
      </div>
    </div>
  )
}

function toolsText(tools: McpTool[], t: T) {
  if (tools.length === 0) return t('settings.signIn_noTools')
  const names = tools.slice(0, 3).map((t) => t.name).join(', ')
  return tools.length > 3 ? t('settings.signIn_moreTools', { names, n: tools.length - 3 }) : names
}

function McpTools({ connection }: { connection: string }) {
  const t = useT()
  const [text, setText] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    api()
      .call(connection, 'mcp.tools', {})
      .then((tools) => live && setText(Array.isArray(tools) && tools.every(isMcpTool) ? toolsText(tools, t) : null))
      .catch(() => live && setText(null))
    return () => {
      live = false
    }
  }, [connection, t])
  if (!text) return null
  return <span className="ml-2 text-sm text-muted">· {text}</span>
}

export function ConnectedAccounts({ accounts, onDisconnect }: { accounts: Connection[]; onDisconnect: (id: string) => void }) {
  const t = useT()
  if (accounts.length === 0) return null
  return (
    <div className="space-y-2">
      <p className="text-sm font-medium text-fg2">{t('settings.signIn_accounts')}</p>
      <ul className="overflow-hidden rounded-card bg-surface shadow-card">
        {accounts.map((c) => (
          <li key={c.id} className="flex h-11 items-center gap-3 px-3.5 [&+&]:border-t">
            <span className={cn('flex size-[18px] items-center justify-center rounded-full text-white', c.status === 'error' ? 'bg-danger' : 'bg-success')}>
              <Icon name={c.status === 'error' ? 'alert' : 'check'} size={11} strokeWidth={2.6} />
            </span>
            <span className="min-w-0 flex-1 truncate text-base text-fg">
              {c.account || c.label}
              {c.capabilities.includes('mcp') && <McpTools connection={c.id} />}
            </span>
            <Button size="sm" variant="ghost" onClick={() => onDisconnect(c.id)}>{t('settings.signIn_disconnect')}</Button>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function SignInFields({ fields, values, onChange }: { fields: FormField[]; values: Record<string, string>; onChange: (name: string, value: string) => void }) {
  if (fields.length === 0) return null
  return (
    <div className="space-y-3">
      {fields.map((f) => (
        <Field key={f.name} label={f.label}>
          <Input type={f.secret ? 'password' : 'text'} autoComplete={f.secret ? 'off' : undefined} value={values[f.name] ?? ''} placeholder={f.placeholder} onChange={(e) => onChange(f.name, e.target.value)} className={f.mono ? 'font-mono' : undefined} />
        </Field>
      ))}
    </div>
  )
}

export function DeviceCode({ site, start }: { site: string; start: DeviceStart }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard?.writeText(start.code).then(() => setCopied(true), () => undefined)
  }
  if (!start.code) {
    return (
      <Panel>
        <p className="text-sm text-fg2">{t('settings.signIn_authorize', { site })}</p>
        <OpenLink url={start.url} />
        <Waiting text={t('settings.signIn_awaiting')} />
      </Panel>
    )
  }
  return (
    <Panel>
      <p className="text-sm text-fg2">{t('settings.signIn_deviceOpen', { site })}</p>
      <div className="flex items-center gap-2">
        <code className="select-all rounded-md bg-surface px-3 py-1.5 font-mono text-lg font-semibold tracking-widest text-fg" aria-label={t('settings.signIn_code')}>
          {start.code}
        </code>
        <Button size="sm" icon={<Icon name={copied ? 'check' : 'copy'} size={14} />} onClick={copy}>
          {copied ? t('settings.copied') : t('settings.copy')}
        </Button>
      </div>
      <OpenLink url={start.url} />
      <Waiting text={t('settings.signIn_awaiting')} />
    </Panel>
  )
}

export function BrowserWait({ site, start, relaying, pasted, onPasted }: { site: string; start: RedirectStart; relaying?: boolean; pasted: string; onPasted: (value: string) => void }) {
  const t = useT()
  const inputId = useId()
  return (
    <Panel>
      <p className="text-sm text-fg2">{t('settings.signIn_authorize', { site })}</p>
      <OpenLink url={start.url} />
      {(start.relay || !start.paste) && <Waiting text={relaying ? t('settings.signIn_relayConfirm') : t('settings.signIn_awaiting')} />}
      {start.paste && (
        <div className="space-y-1.5">
          <label htmlFor={inputId} className="block text-sm font-medium text-fg2">{t('settings.signIn_paste')}</label>
          {start.relay && <p className="text-sm text-muted">{t('settings.signIn_relayHint')}</p>}
          <Input id={inputId} data-paste value={pasted} onChange={(e) => onPasted(e.target.value)} placeholder="http://localhost:…/callback?code=…" className="bg-surface font-mono" />
        </div>
      )}
    </Panel>
  )
}

export function AccountChoice({ poll, choice, onChoice }: { poll: ChoosePoll; choice: string | null; onChoice: (id: string) => void }) {
  return (
    <div className="space-y-2" role="radiogroup" aria-label={poll.title}>
      <p className="text-sm font-medium text-fg2">{poll.title}</p>
      {poll.choices.map((o) => (
        <button key={o.id} type="button" role="radio" aria-checked={choice === o.id} data-choice onClick={() => onChoice(o.id)} className={cn('flex w-full items-center gap-3 rounded-control px-3 py-2.5 text-left shadow-card transition-colors', choice === o.id ? 'bg-active' : 'bg-surface hover:bg-hover')}>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-base text-fg">{o.label}</span>
            {o.detail && <span className="block truncate text-sm text-muted">{o.detail}</span>}
          </span>
          {choice === o.id && <Icon name="check" size={16} className="text-fg" />}
        </button>
      ))}
    </div>
  )
}

export function Resuming() {
  const t = useT()
  return (
    <Panel>
      <Waiting text={t('settings.signIn_resuming')} />
    </Panel>
  )
}

export function SignedIn({ connection }: { connection: Connection }) {
  const t = useT()
  return (
    <Panel>
      <p className="flex items-center gap-2 text-base font-medium text-success-fg">
        <Icon name="checkCircle" size={18} />
        {t('settings.connected')}
      </p>
      <p className="truncate text-base text-fg">{connection.account || connection.label}</p>
    </Panel>
  )
}

export function SignInError({ message }: { message: string | null }) {
  if (!message) return null
  return (
    <p role="alert" className="flex items-start gap-1.5 rounded-control bg-danger-soft px-3 py-2.5 text-sm text-danger-fg">
      <Icon name="alert" size={14} className="mt-0.5 shrink-0" />
      {message}
    </p>
  )
}
