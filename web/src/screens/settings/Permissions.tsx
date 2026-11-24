import { type FormEvent, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { Button, IconButton } from '@/ui/button'
import { Card } from '@/ui/display'
import { Input, SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { type T, useT } from '@/i18n'

export type GrantKind = 'command' | 'site' | 'tool'

export interface Grant {
  kind: GrantKind
  value: string
}

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; grants: Grant[] }

const KINDS = (t: T): { kind: GrantKind; title: string; empty: string; placeholder: string; help: string }[] => [
  { kind: 'command', title: t('settings.perm_commands'), empty: t('settings.perm_commandsEmpty'), placeholder: 'cargo test', help: t('settings.perm_commandsHelp') },
  { kind: 'site', title: t('settings.perm_sites'), empty: t('settings.perm_sitesEmpty'), placeholder: 'gov.br', help: t('settings.perm_sitesHelp') },
  { kind: 'tool', title: t('settings.perm_tools'), empty: t('settings.perm_toolsEmpty'), placeholder: 'mcp__atlassian__*', help: t('settings.perm_toolsHelp') },
]

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

function Row({ grant, onRemove }: { grant: Grant; onRemove: () => void }) {
  const t = useT()
  return (
    <div className="flex items-center gap-3 px-4 py-2.5">
      <code className="min-w-0 flex-1 truncate font-mono text-sm text-fg" title={grant.value}>{grant.value}</code>
      <IconButton tooltip={t('settings.remove')} aria-label={t('settings.removeNamed', { name: grant.value })} onClick={onRemove}>
        <Icon name="close" size={14} />
      </IconButton>
    </div>
  )
}

function Adder({ onAdd }: { onAdd: (grant: Grant) => void }) {
  const t = useT()
  const kinds = KINDS(t)
  const [kind, setKind] = useState<GrantKind>('command')
  const [value, setValue] = useState('')
  const shown = kinds.find((k) => k.kind === kind) ?? kinds[0]
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!value.trim()) return
    onAdd({ kind, value: value.trim() })
    setValue('')
  }
  return (
    <Card className="space-y-3 px-4 py-4">
      <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
        <SegmentedControl<GrantKind> size="sm" value={kind} onChange={setKind} options={kinds.map((k) => ({ value: k.kind, label: k.title }))} />
        <Input aria-label={t('settings.perm_new')} className="min-w-[12rem] flex-1" placeholder={shown.placeholder} value={value} onChange={(e) => setValue(e.target.value)} />
        <Button size="sm" variant="primary" type="submit" disabled={!value.trim()}>
          {t('settings.perm_allow')}
        </Button>
      </form>
      <p className="text-sm text-muted">{shown.help}</p>
    </Card>
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
    <section className="space-y-5">
      <p className="text-sm text-muted">{t('settings.perm_intro')}</p>
      <Adder onAdd={(grant) => save([...grants, grant])} />
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
      {KINDS(t).map(({ kind, title, empty }) => {
        const listed = grants.filter((g) => g.kind === kind)
        return (
          <div key={kind} data-testid={`permissions-${kind}`}>
            <h3 className="mb-2 text-sm font-medium text-fg">{title}</h3>
            <Card className="divide-y">
              {listed.length === 0 ? <p className="px-4 py-3 text-sm text-muted">{empty}</p> : listed.map((grant) => <Row key={grant.value} grant={grant} onRemove={() => save(grants.filter((g) => !same(g, grant)))} />)}
            </Card>
          </div>
        )
      })}
    </section>
  )
}
