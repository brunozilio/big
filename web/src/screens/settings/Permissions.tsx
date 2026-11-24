import { type FormEvent, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { IconButton } from '@/ui/button'
import { Card } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { PermissionsDiscovery } from './PermissionsDiscovery'

export type GrantKind = 'command' | 'site' | 'tool'

export interface Grant {
  kind: GrantKind
  value: string
}

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; grants: Grant[] }

export function ruleOf(grant: Grant): string {
  if (grant.kind === 'command') return grant.value === '*' ? 'Bash' : `Bash(${grant.value}:*)`
  if (grant.kind === 'site') return `Site(${grant.value})`
  return grant.value
}

export function grantOf(rule: string): Grant | null {
  const text = rule.trim()
  if (!text) return null
  if (text === 'Bash' || text === 'Bash(*)') return { kind: 'command', value: '*' }
  const call = /^(\w+)\((.*)\)$/.exec(text)
  const inner = call?.[2].trim() ?? ''
  if (call?.[1] === 'Bash' && inner) return { kind: 'command', value: inner.replace(/:\*$/, '').trim() }
  if ((call?.[1] === 'Site' || call?.[1] === 'WebFetch') && inner) return { kind: 'site', value: inner.replace(/^domain:/, '').trim() }
  return { kind: 'tool', value: text }
}

function usePermissions() {
  const t = useT()
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' })
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    request<{ grants: Grant[] }>('GET', '/api/permissions')
      .then((answer) => alive && setLoaded({ state: 'ready', grants: answer.grants }))
      .catch(() => alive && setLoaded({ state: 'failed' }))
    return () => {
      alive = false
    }
  }, [])
  const save = (grants: Grant[]) => {
    setError(null)
    request<{ grants: Grant[] }>('PUT', '/api/permissions', { grants })
      .then((answer) => setLoaded({ state: 'ready', grants: answer.grants }))
      .catch(() => setError(t('settings.perm_saveFailed')))
  }
  return { loaded, error, save }
}

function Row({ rule, onRemove }: { rule: string; onRemove: () => void }) {
  const t = useT()
  return (
    <li className="group flex items-center gap-3 px-4 py-2">
      <code className="min-w-0 flex-1 truncate font-mono text-sm text-fg" title={rule}>{rule}</code>
      <IconButton size="sm" tooltip={t('settings.remove')} aria-label={t('settings.removeNamed', { name: rule })} onClick={onRemove} className="opacity-60 group-hover:opacity-100">
        <Icon name="close" size={14} />
      </IconButton>
    </li>
  )
}

function Adder({ onAdd }: { onAdd: (grant: Grant) => void }) {
  const t = useT()
  const [value, setValue] = useState('')
  const submit = (e: FormEvent) => {
    e.preventDefault()
    const grant = grantOf(value)
    if (!grant) return
    onAdd(grant)
    setValue('')
  }
  return (
    <li>
      <form onSubmit={submit} className="flex items-center gap-3 px-4 py-2">
        <Icon name="plus" size={14} className="text-muted" />
        <input aria-label={t('settings.perm_new')} placeholder={t('settings.perm_new')} value={value} onChange={(e) => setValue(e.target.value)} className="min-w-0 flex-1 bg-transparent font-mono text-sm text-fg outline-none placeholder:font-sans placeholder:text-muted" />
      </form>
    </li>
  )
}

export function PermissionsView() {
  const t = useT()
  const { loaded, error, save } = usePermissions()
  if (loaded.state === 'loading') return null
  if (loaded.state === 'failed') return <p role="alert" className="text-sm text-danger-fg">{t('settings.perm_loadFailed')}</p>
  const { grants } = loaded
  const same = (a: Grant, b: Grant) => a.kind === b.kind && a.value === b.value
  return (
    <section className="space-y-3">
      <PermissionsDiscovery onAdd={(found) => save([...grants, ...found.filter((grant) => !grants.some((g) => same(g, grant)))])} />
      <Card>
        <ul aria-label={t('settings.tab_permissions')} className="divide-y">
          <Adder onAdd={(grant) => save([...grants, grant])} />
          {grants.length === 0 ? <li className="px-4 py-3 text-sm text-muted">{t('settings.perm_empty')}</li> : grants.map((grant) => <Row key={`${grant.kind}:${grant.value}`} rule={ruleOf(grant)} onRemove={() => save(grants.filter((g) => !same(g, grant)))} />)}
        </ul>
      </Card>
      <p className="text-xs text-muted">{t('settings.perm_help')}</p>
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    </section>
  )
}
