import { useCallback, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { useLocale, useT } from '@/i18n'
import { Button } from '@/ui/button'
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

export function PeopleView() {
  const t = useT()
  const locale = useLocale()
  const [people, setPeople] = useState<PersonRow[] | null>(null)
  const load = useCallback(() => {
    request<PersonRow[]>('GET', PEOPLE).then(setPeople).catch(() => setPeople([]))
  }, [])
  useEffect(load, [load])
  const signOut = (id: string) => void request('DELETE', `${PEOPLE}/${encodeURIComponent(id)}/sessions`).then(load)
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-md font-medium text-fg">{t('auth.people')}</h2>
        <p className="text-sm text-muted">{t('auth.peopleHint')}</p>
      </div>
      {people?.length === 0 && <p className="text-sm text-faint">{t('auth.noPeople')}</p>}
      <ul className="space-y-2">
        {people?.map((person) => (
          <li key={person.id} className="flex items-center gap-3 rounded-card bg-surface px-3 py-2.5 shadow-card">
            <Icon name={person.admin ? 'shield' : 'user'} size={18} className="text-muted" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-base text-fg">{person.name || person.email}</p>
              <p className="truncate text-sm text-muted">
                {person.name ? `${person.email} · ` : ''}
                {t('auth.lastSignIn', { date: day(locale, person.seen) })} · {t('auth.openSessions', { count: person.sessions })}
              </p>
            </div>
            {person.sessions > 0 && <Button size="sm" variant="ghost" icon={<Icon name="logout" size={14} />} onClick={() => signOut(person.id)}>{t('auth.signOutPerson')}</Button>}
          </li>
        ))}
      </ul>
    </section>
  )
}
