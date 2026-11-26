import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useState, type ReactNode } from 'react'
import { api, isMock } from '@/api'
import type { Connection, FormField, Provider, SignInPoll } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { Icon } from '@/ui/icon'
import { tweenClose } from '@/ui/motion'
import { Dialog } from '@/ui/overlay'
import { type T, useT } from '@/i18n'
import {
  AccountChoice,
  BrowserWait,
  ConnectHeader,
  ConnectedAccounts,
  DeviceCode,
  Resuming,
  SignedIn,
  SignInError,
  SignInFields,
  usePolling,
  type ChoosePoll,
  type DeviceStart,
  type RedirectStart,
} from './SignInSteps'

type Step =
  | { kind: 'start' }
  | { kind: 'resume'; flow: string }
  | { kind: 'device'; start: DeviceStart }
  | { kind: 'browser'; start: RedirectStart }
  | { kind: 'choose'; flow: string; poll: ChoosePoll }
  | { kind: 'done'; connection: Connection }

interface Way {
  method?: string
  fields: FormField[]
  button: string
  site: string
}

function wayOf(provider: Provider, method: string | null, t: T): Way {
  const alternative = provider.alternatives?.find((a) => a.key === method)
  if (method && alternative) return { method, fields: alternative.fields, button: alternative.button, site: alternative.site }
  return { fields: provider.fields, button: provider.button ?? t('settings.signIn_with', { name: provider.name }), site: provider.site ?? t('settings.signIn_site', { name: provider.name }) }
}

function flowOf(step: Step): string | null {
  if (step.kind === 'resume') return step.flow
  if (step.kind === 'device') return step.start.flow
  if (step.kind === 'browser' && !step.start.paste) return step.start.flow
  return null
}

interface Primary {
  label: string
  onClick?: () => void
  disabled?: boolean
  loading?: boolean
  icon?: boolean
}

function firstStep(resume: string | null): Step {
  return resume ? { kind: 'resume', flow: resume } : { kind: 'start' }
}

function useSignIn(provider: Provider | null, way: Way | null, fields: Record<string, string>, resume: string | null, onClose: () => void) {
  const [step, setStep] = useState<Step>(() => firstStep(resume))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [choice, setChoice] = useState<string | null>(null)
  const [pasted, setPasted] = useState('')
  const key = provider?.key
  useEffect(() => {
    setStep(firstStep(resume))
    setBusy(false)
    setError(null)
    setChoice(null)
    setPasted('')
  }, [key, resume])
  const fail = (message: string) => {
    setError(message)
    setStep({ kind: 'start' })
  }
  const answered = (flow: string) => (answer: SignInPoll) => {
    if (answer.state === 'done' && resume) onClose()
    else if (answer.state === 'done') setStep({ kind: 'done', connection: answer.connection })
    else if (answer.state === 'failed') fail(answer.message)
    else if (answer.state === 'choose') setStep({ kind: 'choose', flow, poll: answer })
  }
  const flow = flowOf(step)
  usePolling(flow, step.kind === 'device' ? step.start.interval : 1, answered(flow ?? ''), fail)
  const run = (work: Promise<void>) => {
    setBusy(true)
    work.catch((e: unknown) => setError(errorText(e))).finally(() => setBusy(false))
  }
  const begin = () => {
    if (!provider || !way) return
    setError(null)
    setPasted('')
    setChoice(null)
    run(api().startSignIn(provider.key, fields, way.method).then((start) => {
      if (start.kind === 'token') return setStep({ kind: 'resume', flow: start.flow })
      if (start.kind === 'redirect' && !start.paste && !isMock()) leave(start)
      if (start.kind === 'device' && !start.code && !isMock()) globalThis.open?.(start.url, '_blank', 'noopener')
      setStep(start.kind === 'device' ? { kind: 'device', start } : { kind: 'browser', start })
    }))
  }
  const send = (flowId: string, extra: { code?: string; choice?: string }) => {
    setError(null)
    run(api().pollSignIn({ flow: flowId, ...extra }).then(answered(flowId)))
  }
  const disconnect = (id: string) => {
    setError(null)
    api().disconnect(id).catch((e: unknown) => setError(errorText(e)))
  }
  return { step, busy, error, choice, setChoice, pasted, setPasted, begin, send, disconnect }
}

function leave(start: RedirectStart) {
  if (start.returns) globalThis.location.assign(start.url)
  else globalThis.open?.(start.url, '_blank', 'noopener')
}

function primaryOf(signIn: ReturnType<typeof useSignIn>, way: Way, ready: boolean, onClose: () => void, t: T): Primary {
  const { step, busy } = signIn
  if (step.kind === 'start') return { label: way.button, onClick: signIn.begin, disabled: !ready, loading: busy, icon: true }
  if (step.kind === 'browser' && step.start.paste) return { label: t('settings.signIn_finish'), onClick: () => signIn.send(step.start.flow, { code: signIn.pasted.trim() }), disabled: !signIn.pasted.trim(), loading: busy }
  if (step.kind === 'choose') return { label: t('settings.save'), onClick: () => signIn.choice && signIn.send(step.flow, { choice: signIn.choice }), disabled: !signIn.choice, loading: busy }
  if (step.kind === 'done') return { label: t('settings.signIn_finish'), onClick: onClose }
  return { label: t('settings.signIn_waiting'), loading: true }
}

function Body({ signIn, way, accounts, fields, onField, toggle }: { signIn: ReturnType<typeof useSignIn>; way: Way; accounts: Connection[]; fields: Record<string, string>; onField: (name: string, value: string) => void; toggle: ReactNode }) {
  const { step } = signIn
  if (step.kind === 'resume') return <Resuming />
  if (step.kind === 'device') return <DeviceCode site={way.site} start={step.start} />
  if (step.kind === 'browser') return <BrowserWait site={way.site} start={step.start} pasted={signIn.pasted} onPasted={signIn.setPasted} />
  if (step.kind === 'choose') return <AccountChoice poll={step.poll} choice={signIn.choice} onChoice={signIn.setChoice} />
  if (step.kind === 'done') return <SignedIn connection={step.connection} />
  return (
    <div className="space-y-4">
      <ConnectedAccounts accounts={accounts} onDisconnect={signIn.disconnect} />
      <SignInFields fields={way.fields} values={fields} onChange={onField} />
      {toggle}
    </div>
  )
}

export function ConnectDialog({ provider: key, resume, onClose }: { provider: string | null; resume?: string | null; onClose: () => void }) {
  const t = useT()
  const provider = useStore((s) => s.connections?.providers.find((p) => p.key === key) ?? null)
  const all = useStore((s) => s.connections?.connections)
  const accounts = (all ?? []).filter((c) => c.provider === key)
  const [method, setMethod] = useState<string | null>(null)
  const [fields, setFields] = useState<Record<string, string>>({})
  const way = provider ? wayOf(provider, method, t) : null
  const signIn = useSignIn(provider, way, fields, resume ?? null, onClose)
  useEffect(() => setMethod(null), [key])
  useEffect(() => {
    setFields(Object.fromEntries((way?.fields ?? []).map((f) => [f.name, f.value ?? ''])))
  }, [way?.fields])
  const ready = !way?.fields.some((f) => !f.optional && !fields[f.name]?.trim())
  const primary = way ? primaryOf(signIn, way, ready, onClose, t) : null
  const footer = primary && (
    <>
      {signIn.step.kind !== 'done' && <Button variant="ghost" onClick={onClose}>{t('settings.cancel')}</Button>}
      <Button variant="primary" className="min-w-[132px]" loading={primary.loading} disabled={primary.disabled} onClick={primary.onClick}>
        {primary.label}
        {primary.icon && !primary.loading && <Icon name="arrowUpRight" size={15} />}
      </Button>
    </>
  )
  const ways = [
    ...(method && provider ? [{ key: null, label: t('settings.signIn_withAccount', { name: provider.name }) }] : []),
    ...(provider?.alternatives ?? []).filter((a) => a.key !== method).map((a) => ({ key: a.key, label: a.label })),
  ]
  const toggle = ways.length > 0 && (
    <div className="flex flex-wrap gap-x-4 gap-y-1">
      {ways.map((w) => (
        <button key={w.key ?? ''} type="button" onClick={() => setMethod(w.key)} className="text-sm text-muted underline decoration-line-strong underline-offset-4 transition-colors hover:text-fg">
          {w.label}
        </button>
      ))}
    </div>
  )
  return (
    <Dialog open={!!provider} onOpenChange={(o) => !o && !signIn.busy && onClose()} title={provider ? t('settings.signIn_title', { name: provider.name }) : ''} footer={footer}>
      {provider && way && (
        <div className="flex min-h-[242px] flex-col gap-4">
          <ConnectHeader logo={provider.logo} name={provider.name} description={provider.description} />
          <AnimatePresence mode="wait" initial={false}>
            <motion.div key={`${signIn.step.kind}-${method ?? ''}`} initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -2 }} transition={tweenClose}>
              <Body signIn={signIn} way={way} accounts={accounts} fields={fields} onField={(name, value) => setFields((v) => ({ ...v, [name]: value }))} toggle={toggle} />
            </motion.div>
          </AnimatePresence>
          <SignInError message={signIn.error} />
        </div>
      )}
    </Dialog>
  )
}
