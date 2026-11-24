import { useState } from 'react'
import { request } from '@/api/http'
import { Button } from '@/ui/button'
import { Card } from '@/ui/display'
import { Checkbox } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { type Grant, ruleOf } from './Permissions'

export interface Suggested {
  grant: Grant
  why: string
}

type Found = { state: 'idle' } | { state: 'running' } | { state: 'failed' } | { state: 'ready'; suggestions: Suggested[]; picked: boolean[] }

export function picked(suggestions: Suggested[], marks: boolean[]): Grant[] {
  return suggestions.filter((_, at) => marks[at]).map((suggested) => suggested.grant)
}

function Choices({ suggestions, marks, onMark }: { suggestions: Suggested[]; marks: boolean[]; onMark: (at: number, on: boolean) => void }) {
  return (
    <ul className="divide-y">
      {suggestions.map((suggested, at) => {
        const rule = ruleOf(suggested.grant)
        return (
          <li key={rule}>
            <label className="flex cursor-pointer items-center gap-3 px-4 py-2">
              <Checkbox label={rule} checked={marks[at]} onCheckedChange={(on) => onMark(at, on)} />
              <code className="min-w-0 shrink truncate font-mono text-sm text-fg" title={rule}>{rule}</code>
              {suggested.why && <span className="ml-auto truncate text-xs text-muted" title={suggested.why}>{suggested.why}</span>}
            </label>
          </li>
        )
      })}
    </ul>
  )
}

export function PermissionsDiscovery({ onAdd }: { onAdd: (grants: Grant[]) => void }) {
  const t = useT()
  const [found, setFound] = useState<Found>({ state: 'idle' })
  const discover = () => {
    setFound({ state: 'running' })
    request<{ suggestions: Suggested[] }>('POST', '/api/permissions/discover', {})
      .then((answer) => setFound({ state: 'ready', suggestions: answer.suggestions, picked: answer.suggestions.map(() => true) }))
      .catch(() => setFound({ state: 'failed' }))
  }
  const running = found.state === 'running'
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        <Button size="sm" icon={<Icon name="sparkles" size={14} />} loading={running} onClick={discover}>
          {t('settings.perm_discover')}
        </Button>
        {running && <span className="text-sm text-muted">{t('settings.perm_discovering')}</span>}
        {found.state === 'failed' && <span role="alert" className="text-sm text-danger-fg">{t('settings.perm_discoverFailed')}</span>}
        {found.state === 'ready' && found.suggestions.length === 0 && <span className="text-sm text-muted">{t('settings.perm_nothingNew')}</span>}
      </div>
      {found.state === 'ready' && found.suggestions.length > 0 && (
        <Card>
          <Choices suggestions={found.suggestions} marks={found.picked} onMark={(at, on) => setFound({ ...found, picked: found.picked.map((mark, i) => (i === at ? on : mark)) })} />
          <div className="flex justify-end gap-2 border-t px-4 py-2">
            <Button size="sm" variant="ghost" onClick={() => setFound({ state: 'idle' })}>
              {t('settings.perm_discard')}
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!found.picked.some(Boolean)}
              onClick={() => {
                onAdd(picked(found.suggestions, found.picked))
                setFound({ state: 'idle' })
              }}
            >
              {t('settings.perm_addPicked', { count: found.picked.filter(Boolean).length })}
            </Button>
          </div>
        </Card>
      )}
    </div>
  )
}
