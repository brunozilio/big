import { type FormEvent, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { IconButton } from '@/ui/button'
import { Card } from '@/ui/display'
import { ChipPicker } from '@/ui/chipPicker'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { PermissionsDiscovery } from './PermissionsDiscovery'

export type GrantKind = 'command' | 'site' | 'tool'

const same = (a: { kind: GrantKind; value: string }, b: { kind: GrantKind; value: string }) => a.kind === b.kind && a.value === b.value

export type Behavior = 'allow' | 'ask' | 'deny'

export interface Grant {
  kind: GrantKind
  value: string
  behavior?: Behavior
}

const BEHAVIORS: Behavior[] = ['allow', 'ask', 'deny']

export function withBehavior(grants: Grant[], grant: Grant, behavior: Behavior): Grant[] {
  return grants.map((known) => (same(known, grant) ? { ...known, behavior } : known))
}

const LOOK: Record<Behavior, { icon: 'checkCircle' | 'help' | 'stop'; tone: string }> = {
  allow: { icon: 'checkCircle', tone: 'text-success-fg' },
  ask: { icon: 'help', tone: 'text-warning-fg' },
  deny: { icon: 'stop', tone: 'text-danger-fg' },
}

function BehaviorPicker({ value, onChange, name }: { value: Behavior; onChange: (behavior: Behavior) => void; name: string }) {
  const t = useT()
  const label = { allow: t('settings.perm_allow'), ask: t('settings.perm_ask'), deny: t('settings.perm_deny') }
  const options = BEHAVIORS.map((behavior) => ({ value: behavior, label: label[behavior], icon: <Icon name={LOOK[behavior].icon} size={14} className={LOOK[behavior].tone} /> }))
  return <ChipPicker label={t('settings.perm_behaviorOf', { name })} placeholder={t('settings.perm_changeTo')} value={value} options={options} onChange={onChange} />
}

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; grants: Grant[] }

export function ruleOf(grant: Grant): string {
  if (grant.kind === 'command') return grant.value === '*' ? 'Bash' : `Bash(${grant.value}:*)`
  if (grant.kind === 'site') return `Site(${grant.value})`
  return grant.value
}

const WORD = /^[A-Za-z0-9_.-]+$/

function toolNamed(tool: string): boolean {
  if (!tool.startsWith('mcp__')) return /^[A-Z][A-Za-z0-9]*$/.test(tool)
  const [server, ...rest] = tool.slice(5).split('__')
  const name = rest.join('__')
  return WORD.test(server ?? '') && rest.length === 1 && (name === '*' || WORD.test(name.replace(/\*$/, '')))
}

function siteOf(text: string): string | null {
  let host = text.trim().replace(/^domain:/, '').toLowerCase()
  if (host.includes('://')) {
    try {
      host = new URL(host).hostname
    } catch {
      return null
    }
  }
  const label = '[a-z0-9]([a-z0-9-]*[a-z0-9])?'
  const valid = new RegExp(`^(\\*\\.)?(${label}\\.)+[a-z]{2,}$`).test(host) || host === 'localhost'
  return valid ? host : null
}

export function grantOf(rule: string): Grant | null {
  const text = rule.trim()
  if (!text) return null
  if (text === 'Bash' || text === 'Bash(*)') return { kind: 'command', value: '*' }
  const call = /^(\w+)\((.*)\)$/s.exec(text)
  if (call) {
    const inner = call[2].trim()
    if (call[1] === 'Bash') {
      const command = inner.replace(/:\*$/, '').trim()
      return command && !command.includes('\n') ? { kind: 'command', value: command } : null
    }
    if (call[1] === 'Site' || call[1] === 'WebFetch') {
      const site = siteOf(inner)
      return site ? { kind: 'site', value: site } : null
    }
    return null
  }
  return toolNamed(text) ? { kind: 'tool', value: text } : null
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

function Row({ grant, onBehavior, onRemove }: { grant: Grant; onBehavior: (behavior: Behavior) => void; onRemove: () => void }) {
  const t = useT()
  const rule = ruleOf(grant)
  return (
    <li className="group flex items-center gap-3 px-4 py-2">
      <code className="min-w-0 flex-1 truncate font-mono text-sm text-fg" title={rule}>{rule}</code>
      <BehaviorPicker name={rule} value={grant.behavior ?? 'allow'} onChange={onBehavior} />
      <IconButton size="sm" tooltip={t('settings.remove')} aria-label={t('settings.removeNamed', { name: rule })} onClick={onRemove} className="opacity-60 group-hover:opacity-100">
        <Icon name="close" size={14} />
      </IconButton>
    </li>
  )
}


function Adder({ grants, onAdd }: { grants: Grant[]; onAdd: (grant: Grant) => void }) {
  const t = useT()
  const [value, setValue] = useState('')
  const [problem, setProblem] = useState<string | null>(null)
  const [behavior, setBehavior] = useState<Behavior>('allow')
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!value.trim()) return
    const grant = grantOf(value)
    if (!grant) return setProblem(t('settings.perm_invalid'))
    if (grants.some((known) => same(known, grant))) return setProblem(t('settings.perm_duplicate'))
    onAdd({ ...grant, behavior })
    setValue('')
  }
  return (
    <li>
      <form onSubmit={submit} className="px-4 py-2">
        <div className="flex items-center gap-3">
          <Icon name="plus" size={14} className="text-muted" />
          <input
            aria-label={t('settings.perm_new')}
            aria-invalid={problem !== null}
            placeholder={t('settings.perm_new')}
            value={value}
            onChange={(e) => {
              setValue(e.target.value)
              setProblem(null)
            }}
            className="min-w-0 flex-1 bg-transparent font-mono text-sm text-fg outline-none placeholder:font-sans placeholder:text-muted"
          />
          <BehaviorPicker name={t('settings.perm_new')} value={behavior} onChange={setBehavior} />
        </div>
        {problem && <p role="alert" className="mt-1 pl-[26px] text-xs text-danger-fg">{problem}</p>}
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
  return (
    <section className="space-y-3">
      <PermissionsDiscovery onAdd={(found) => save([...found.filter((grant) => !grants.some((g) => same(g, grant))), ...grants])} />
      <Card>
        <ul aria-label={t('settings.tab_permissions')} className="divide-y">
          <Adder grants={grants} onAdd={(grant) => save([grant, ...grants])} />
          {grants.length === 0 ? <li className="px-4 py-3 text-sm text-muted">{t('settings.perm_empty')}</li> : grants.map((grant) => <Row key={`${grant.kind}:${grant.value}`} grant={grant} onBehavior={(behavior) => save(withBehavior(grants, grant, behavior))} onRemove={() => save(grants.filter((g) => !same(g, grant)))} />)}
        </ul>
      </Card>
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    </section>
  )
}
