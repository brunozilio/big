import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { api } from '@/api'
import { addPasskey, authMethods, changePassword, logout, PASSWORD_MIN, passkeys, passkeysWork, removePasskey, whoAmI, type Me, type PasskeyInfo } from '@/api/auth'
import { at } from '@/api/base'
import { ApiError } from '@/api/http'
import { getState, setBranding } from '@/api/store'
import { useLocale, useT } from '@/i18n'
import { Button } from '@/ui/button'
import { ProductMark } from '@/ui/domain'
import { Input } from '@/ui/form'
import { Icon } from '@/ui/icon'

function day(locale: string, seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(locale, { day: 'numeric', month: 'short', year: 'numeric' })
}

function PasswordField({ label, value, onChange, autoComplete }: { label: string; value: string; onChange: (value: string) => void; autoComplete: string }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium text-fg2">{label}</span>
      <Input type="password" autoComplete={autoComplete} value={value} onChange={(e) => onChange(e.target.value)} />
    </label>
  )
}

function Password() {
  const t = useT()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [again, setAgain] = useState('')
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    setNote(null)
    if (next.length < PASSWORD_MIN) return setError(t('auth.passwordTooShort', { min: PASSWORD_MIN }))
    if (next !== again) return setError(t('auth.passwordMismatch'))
    setBusy(true)
    setError(null)
    changePassword(current, next)
      .then(() => {
        setCurrent('')
        setNext('')
        setAgain('')
        setNote(t('auth.passwordSaved'))
      })
      .catch((err: unknown) => setError(err instanceof ApiError ? err.message : t('auth.passwordNotSaved')))
      .finally(() => setBusy(false))
  }
  return (
    <form onSubmit={submit} className="space-y-3">
      <div>
        <h2 className="text-md font-medium text-fg">{t('auth.password')}</h2>
        <p className="text-sm text-muted">{t('auth.passwordHint')}</p>
      </div>
      <PasswordField label={t('auth.currentPassword')} value={current} onChange={setCurrent} autoComplete="current-password" />
      <PasswordField label={t('auth.newPassword')} value={next} onChange={setNext} autoComplete="new-password" />
      <PasswordField label={t('auth.confirmPassword')} value={again} onChange={setAgain} autoComplete="new-password" />
      {error && <p className="text-sm text-danger-fg">{error}</p>}
      {note && !error && <p className="text-sm text-muted">{note}</p>}
      <Button type="submit" variant="primary" loading={busy} disabled={!current || !next || !again}>{t('auth.savePassword')}</Button>
    </form>
  )
}

function Passkeys() {
  const t = useT()
  const locale = useLocale()
  const [keys, setKeys] = useState<PasskeyInfo[] | null>(null)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const load = useCallback(() => {
    passkeys().then(setKeys).catch(() => setKeys([]))
  }, [])
  useEffect(load, [load])
  const add = () => {
    if (!passkeysWork()) return setError(t('auth.passkeyUnsupported'))
    setBusy(true)
    setError(null)
    addPasskey(name.trim())
      .then(() => {
        setName('')
        load()
      })
      .catch((err: unknown) => setError(err instanceof ApiError ? err.message : t('auth.passkeyFailed')))
      .finally(() => setBusy(false))
  }
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-md font-medium text-fg">{t('auth.passkeys')}</h2>
        <p className="text-sm text-muted">{t('auth.passkeysHint')}</p>
      </div>
      {keys?.length === 0 && <p className="text-sm text-faint">{t('auth.noPasskeys')}</p>}
      <ul className="space-y-2">
        {keys?.map((key) => (
          <li key={key.id} className="flex items-center gap-3 rounded-card bg-surface px-3 py-2.5 shadow-card">
            <Icon name="fingerprint" size={18} className="text-muted" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-base text-fg">{key.name}</p>
              <p className="text-sm text-muted">{key.used ? t('auth.lastUsed', { date: day(locale, key.used) }) : t('auth.created', { date: day(locale, key.created) })}</p>
            </div>
            <Button size="sm" variant="ghost" onClick={() => void removePasskey(key.id).then(load)}>{t('auth.removePasskey')}</Button>
          </li>
        ))}
      </ul>
      <div className="flex gap-2">
        <Input aria-label={t('auth.passkeyName')} value={name} onChange={(e) => setName(e.target.value)} placeholder={t('auth.passkeyNamePlaceholder')} />
        <Button variant="primary" icon={<Icon name="plus" size={14} />} loading={busy} onClick={add}>{t('auth.addPasskey')}</Button>
      </div>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
    </section>
  )
}

export function AccountPanel() {
  const t = useT()
  const [me, setMe] = useState<Me | null>(null)
  const [password, setPassword] = useState(false)
  useEffect(() => {
    whoAmI().then(setMe).catch(() => undefined)
    authMethods()
      .then((offered) => setPassword(offered.methods.includes('password')))
      .catch(() => undefined)
  }, [])
  const out = () => void logout().then(() => window.location.replace(at('/login')))
  const local = me?.mode === 'local'
  return (
    <div className="space-y-8">
      <section className="flex items-center gap-3 rounded-card bg-surface px-4 py-3 shadow-card">
        <Icon name="user" size={18} className="text-muted" />
        <p className="flex-1 text-base text-fg">{local ? t('auth.localUser') : me ? t('auth.signedInAs', { email: me.user.email }) : ''}</p>
        {!local && me && <Button size="sm" icon={<Icon name="logout" size={14} />} onClick={out}>{t('auth.logout')}</Button>}
      </section>
      {!local && me && password && <Password />}
      {!local && me && <Passkeys />}
    </div>
  )
}

export function Account() {
  const t = useT()
  useEffect(() => {
    if (!getState().branding) api().branding().then(setBranding).catch(() => undefined)
  }, [])
  return (
    <div className="min-h-dvh bg-bg px-4">
      <main className="mx-auto w-full max-w-[520px] space-y-8 py-12">
        <div className="flex items-center gap-3">
          <ProductMark />
          <h1 className="flex-1 text-lg font-semibold text-fg">{t('auth.account')}</h1>
          <Button asChild variant="ghost" size="sm">
            <Link to="/">{t('auth.back')}</Link>
          </Button>
        </div>
        <AccountPanel />
      </main>
    </div>
  )
}
