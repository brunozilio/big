import { useEffect, useState, type FormEvent, type ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { api } from '@/api'
import { authMethods, nextPath, passkeysWork, sendCode, signInWithPasskey, ssoUrl, verifyCode, type AuthMethods } from '@/api/auth'
import { at } from '@/api/base'
import { ApiError } from '@/api/http'
import { getState, setBranding, useStore } from '@/api/store'
import { isLanguage, LANGUAGES, useLanguage, useT } from '@/i18n'
import { applyLanguage } from '@/i18n/sync'
import { Button } from '@/ui/button'
import { ProductMark } from '@/ui/domain'
import { Input, Select } from '@/ui/form'
import { Icon, type IconName } from '@/ui/icon'

type Stage = { kind: 'email' } | { kind: 'code'; email: string }

function messageOf(error: unknown, fallback: string): string {
  return error instanceof ApiError && error.message ? error.message : fallback
}

function Tile({ icon, label, onClick, busy }: { icon: IconName; label: string; onClick: () => void; busy?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      className="flex h-20 flex-col items-center justify-center gap-1.5 rounded-card border border-border bg-surface text-base text-fg shadow-card transition-colors hover:bg-hover disabled:opacity-45"
    >
      <Icon name={icon} size={20} />
      {label}
    </button>
  )
}

function Divider({ children }: { children: ReactNode }) {
  return (
    <div className="my-6 flex items-center gap-3 text-sm text-muted">
      <span className="h-px flex-1 bg-border" />
      {children}
      <span className="h-px flex-1 bg-border" />
    </div>
  )
}

function LoginLanguage() {
  const t = useT()
  const language = useLanguage()
  return (
    <Select
      size="sm"
      label={t('common.language')}
      value={language}
      leading={<Icon name="globe" size={14} className="text-muted" />}
      options={LANGUAGES.map((l) => ({ value: l.value, label: l.label }))}
      onValueChange={(value) => {
        if (!isLanguage(value)) return
        applyLanguage(value)
        const branding = getState().branding
        if (branding) setBranding({ ...branding, language: value })
      }}
    />
  )
}

function finish(search: string) {
  window.location.replace(at(nextPath(search)))
}

function EmailStage({ onSent }: { onSent: (email: string) => void }) {
  const t = useT()
  const [email, setEmail] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!email.trim()) return
    setBusy(true)
    setError(null)
    sendCode(email.trim())
      .then(() => onSent(email.trim()))
      .catch((err: unknown) => setError(messageOf(err, t('auth.noMethods'))))
      .finally(() => setBusy(false))
  }
  return (
    <form onSubmit={submit} className="space-y-3">
      <label className="block">
        <span className="mb-1.5 block text-sm font-medium text-fg2">{t('auth.email')}</span>
        <Input type="email" autoComplete="username webauthn" autoFocus value={email} onChange={(e) => setEmail(e.target.value)} placeholder={t('auth.emailPlaceholder')} />
        <span className="mt-1.5 block text-sm text-muted">{t('auth.emailHint')}</span>
      </label>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
      <Button type="submit" variant="primary" size="lg" className="w-full" loading={busy} disabled={!email.trim()}>
        {t('auth.continue')}
      </Button>
    </form>
  )
}

function CodeStage({ email, onBack }: { email: string; onBack: () => void }) {
  const t = useT()
  const location = useLocation()
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    verifyCode(email, code.trim())
      .then(() => finish(location.search))
      .catch((err: unknown) => setError(messageOf(err, t('auth.passkeyFailed'))))
      .finally(() => setBusy(false))
  }
  const resend = () => {
    setError(null)
    sendCode(email)
      .then(() => setNote(t('auth.resent')))
      .catch((err: unknown) => setError(messageOf(err, t('auth.noMethods'))))
  }
  return (
    <form onSubmit={submit} className="space-y-3">
      <div className="text-center">
        <p className="text-md font-medium text-fg">{t('auth.codeTitle')}</p>
        <p className="mt-1 text-sm text-muted">{t('auth.codeSent', { email })}</p>
      </div>
      <label className="block">
        <span className="mb-1.5 block text-sm font-medium text-fg2">{t('auth.code')}</span>
        <Input inputMode="numeric" autoComplete="one-time-code" autoFocus maxLength={6} value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, ''))} placeholder={t('auth.codePlaceholder')} className="tracking-[0.3em] tabular-nums" />
      </label>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
      {note && !error && <p className="text-sm text-muted">{note}</p>}
      <Button type="submit" variant="primary" size="lg" className="w-full" loading={busy} disabled={code.length !== 6}>
        {t('auth.verify')}
      </Button>
      <div className="flex justify-between text-sm">
        <Button type="button" variant="link" onClick={onBack}>{t('auth.otherEmail')}</Button>
        <Button type="button" variant="link" onClick={resend}>{t('auth.resend')}</Button>
      </div>
    </form>
  )
}

function Others({ methods }: { methods: AuthMethods }) {
  const t = useT()
  const location = useLocation()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const passkey = methods.methods.includes('passkey')
  const sso = methods.methods.includes('sso')
  if (!passkey && !sso) return null
  const withPasskey = () => {
    if (!passkeysWork()) return setError(t('auth.passkeyUnsupported'))
    setBusy(true)
    setError(null)
    signInWithPasskey()
      .then(() => finish(location.search))
      .catch((err: unknown) => setError(messageOf(err, t('auth.passkeyFailed'))))
      .finally(() => setBusy(false))
  }
  const withSso = () => window.location.assign(ssoUrl(nextPath(location.search)))
  return (
    <>
      {methods.methods.includes('email') && <Divider>{t('auth.orContinueWith')}</Divider>}
      <div className={passkey && sso ? 'grid grid-cols-2 gap-2' : 'grid grid-cols-1 gap-2'}>
        {passkey && <Tile icon="fingerprint" label={t('auth.passkey')} onClick={withPasskey} busy={busy} />}
        {sso && <Tile icon="building" label={methods.ssoLabel || t('auth.sso')} onClick={withSso} />}
      </div>
      {error && <p className="mt-3 text-center text-sm text-danger-fg">{error}</p>}
    </>
  )
}

function useBrandingAndMethods(): AuthMethods | null {
  const [methods, setMethods] = useState<AuthMethods | null>(null)
  useEffect(() => {
    if (!getState().branding) api().branding().then(setBranding).catch(() => undefined)
    authMethods().then(setMethods).catch(() => setMethods({ mode: 'shared', methods: [], ssoLabel: '' }))
  }, [])
  return methods
}

export function Login() {
  const t = useT()
  const location = useLocation()
  const name = useStore((s) => s.branding?.name) ?? ''
  const methods = useBrandingAndMethods()
  const [stage, setStage] = useState<Stage>({ kind: 'email' })
  const failed = new URLSearchParams(location.search).get('error') === 'sso'
  const email = methods?.methods.includes('email') ?? false
  return (
    <div className="flex min-h-dvh flex-col bg-bg px-4">
      <main className="mx-auto flex w-full max-w-[392px] flex-1 flex-col justify-center py-16">
        <div className="mb-8 flex flex-col items-center text-center">
          <ProductMark className="mb-6" />
          <h1 className="text-xl font-semibold leading-tight text-fg">{t('auth.headline')}</h1>
          <p className="text-xl font-semibold leading-tight text-muted">{t('auth.subtitle', { name })}</p>
        </div>
        {failed && <p className="mb-4 rounded-control bg-danger/10 px-3 py-2 text-sm text-danger-fg">{t('auth.ssoFailed')}</p>}
        {methods?.mode === 'local' && (
          <div className="mb-6 rounded-card bg-inset px-4 py-3 text-center text-sm text-fg2">
            {t('auth.localNotice')}
            <Button variant="link" className="ml-1" onClick={() => finish(location.search)}>{t('auth.localEnter')}</Button>
          </div>
        )}
        {methods && email && (stage.kind === 'email' ? <EmailStage onSent={(sent) => setStage({ kind: 'code', email: sent })} /> : <CodeStage email={stage.email} onBack={() => setStage({ kind: 'email' })} />)}
        {methods && stage.kind === 'email' && <Others methods={methods} />}
        {methods && methods.methods.length === 0 && <p className="text-center text-sm text-muted">{t('auth.noMethods')}</p>}
      </main>
      <footer className="flex justify-center pb-6">
        <LoginLanguage />
      </footer>
    </div>
  )
}
