import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { authMethods } from '@/api/auth'
import { ApiError, request } from '@/api/http'
import { useLocale, useT } from '@/i18n'
import { Button } from '@/ui/button'
import { Input } from '@/ui/form'
import { Icon } from '@/ui/icon'

export interface PersonRow {
  id: string
  email: string
  name: string
  created: number
  seen: number
  sessions: number
  admin: boolean
}

export const PEOPLE = '/api/admin/people'

function day(locale: string, seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(locale, { day: 'numeric', month: 'short', year: 'numeric' })
}

function messageOf(error: unknown, fallback: string): string {
  return error instanceof ApiError && error.message ? error.message : fallback
}

function AddPerson({ onAdded }: { onAdded: () => void }) {
  const t = useT()
  const [login, setLogin] = useState('')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const ready = !!login.trim() && !!password
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!ready) return
    setBusy(true)
    setError(null)
    request<{ id: string }>('POST', PEOPLE, { login: login.trim(), name: name.trim(), password })
      .then(() => {
        setLogin('')
        setName('')
        setPassword('')
        onAdded()
      })
      .catch((err: unknown) => setError(messageOf(err, t('auth.personNotAdded'))))
      .finally(() => setBusy(false))
  }
  return (
    <form onSubmit={submit} className="space-y-2 rounded-card bg-surface px-3 py-3 shadow-card">
      <p className="text-base font-medium text-fg">{t('auth.addPerson')}</p>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
        <Input aria-label={t('auth.login')} placeholder={t('auth.login')} autoComplete="off" autoCapitalize="none" spellCheck={false} value={login} onChange={(e) => setLogin(e.target.value)} />
        <Input aria-label={t('auth.personName')} placeholder={t('auth.personName')} autoComplete="off" value={name} onChange={(e) => setName(e.target.value)} />
        <Input aria-label={t('auth.initialPassword')} placeholder={t('auth.initialPassword')} type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
      </div>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
      <Button type="submit" variant="primary" icon={<Icon name="plus" size={14} />} loading={busy} disabled={!ready}>{t('auth.addPerson')}</Button>
    </form>
  )
}

function SetPassword({ id, onDone }: { id: string; onDone: () => void }) {
  const t = useT()
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!password) return
    setBusy(true)
    setError(null)
    request('PUT', `${PEOPLE}/${encodeURIComponent(id)}/password`, { password })
      .then(onDone)
      .catch((err: unknown) => setError(messageOf(err, t('auth.passwordNotSaved'))))
      .finally(() => setBusy(false))
  }
  return (
    <form onSubmit={submit} className="space-y-2 pt-2">
      <div className="flex gap-2">
        <Input aria-label={t('auth.newPassword')} placeholder={t('auth.newPassword')} type="password" autoComplete="new-password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} />
        <Button type="submit" variant="primary" loading={busy} disabled={!password}>{t('auth.savePersonPassword')}</Button>
      </div>
      {error && <p className="text-sm text-danger-fg">{error}</p>}
    </form>
  )
}

export function PeopleView() {
  const t = useT()
  const locale = useLocale()
  const [people, setPeople] = useState<PersonRow[] | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [passwords, setPasswords] = useState(false)
  const load = useCallback(() => {
    request<PersonRow[]>('GET', PEOPLE).then(setPeople).catch(() => setPeople([]))
  }, [])
  useEffect(load, [load])
  useEffect(() => {
    authMethods()
      .then((offered) => setPasswords(offered.methods.includes('password')))
      .catch(() => undefined)
  }, [])
  const signOut = (id: string) => void request('DELETE', `${PEOPLE}/${encodeURIComponent(id)}/sessions`).then(load)
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-md font-medium text-fg">{t('auth.people')}</h2>
        <p className="text-sm text-muted">{t('auth.peopleHint')}</p>
      </div>
      {passwords && <AddPerson onAdded={load} />}
      {people?.length === 0 && <p className="text-sm text-faint">{t('auth.noPeople')}</p>}
      <ul className="space-y-2">
        {people?.map((person) => (
          <li key={person.id} className="rounded-card bg-surface px-3 py-2.5 shadow-card">
            <div className="flex items-center gap-3">
              <Icon name={person.admin ? 'shield' : 'user'} size={18} className="text-muted" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-base text-fg">{person.name || person.email}</p>
                <p className="truncate text-sm text-muted">
                  {person.name ? `${person.email} · ` : ''}
                  {person.seen ? t('auth.lastSignIn', { date: day(locale, person.seen) }) : t('auth.neverSignedIn')} · {t('auth.openSessions', { count: person.sessions })}
                </p>
              </div>
              {passwords && <Button size="sm" variant="ghost" onClick={() => setEditing(editing === person.id ? null : person.id)}>{editing === person.id ? t('auth.cancel') : t('auth.setPassword')}</Button>}
              {person.sessions > 0 && <Button size="sm" variant="ghost" icon={<Icon name="logout" size={14} />} onClick={() => signOut(person.id)}>{t('auth.signOutPerson')}</Button>}
            </div>
            {editing === person.id && (
              <SetPassword
                id={person.id}
                onDone={() => {
                  setEditing(null)
                  load()
                }}
              />
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
