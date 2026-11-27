import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from '@/api'
import type { BaseBranches, BaseGap, Item } from '@/api/contract'
import { useT } from '@/i18n'
import { Button } from '@/ui/button'
import { Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { report } from './parts'

function useBranches(item: string, repository: string): BaseBranches | null {
  const [found, setFound] = useState<BaseBranches | null>(null)
  useEffect(() => {
    let live = true
    api().baseBranches(item, repository).then((got) => { if (live) setFound(got) }).catch(report)
    return () => { live = false }
  }, [item, repository])
  return found
}

export function BaseFix({ it, gap }: { it: Item; gap: BaseGap }) {
  const t = useT()
  const navigate = useNavigate()
  const known = useBranches(it.id, gap.repository)
  const branches = known?.branches ?? []
  const fallback = known?.default ?? branches[0] ?? ''
  const [from, setFrom] = useState('')
  const [other, setOther] = useState('')
  const [busy, setBusy] = useState(false)
  const source = from || fallback
  const chosen = other || fallback
  const options = branches.map((name) => ({ value: name, label: name }))
  const send = (base: string, origin?: string) => {
    setBusy(true)
    api().chooseBase(it.id, { repository: gap.repository, base, from: origin }).catch(report).finally(() => setBusy(false))
  }
  const application = it.application?.id
  if (known && branches.length === 0) return <p className="text-sm text-muted">{t('items.baseNoBranches')}</p>
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-fg">{t('items.baseCreate', { base: gap.missing })}</span>
        <Select size="sm" value={source} onValueChange={setFrom} options={options} label={t('items.baseCreate', { base: gap.missing })} />
        <Button size="sm" variant="primary" icon={<Icon name="plus" size={15} />} disabled={busy || !source} onClick={() => send(gap.missing, source)}>{t('items.baseCreateAction')}</Button>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-fg">{t('items.baseOther')}</span>
        <Select size="sm" value={chosen} onValueChange={setOther} options={options} label={t('items.baseOther')} />
        <Button size="sm" variant="secondary" disabled={busy || !chosen} onClick={() => send(chosen)}>{t('items.baseUse')}</Button>
      </div>
      {application && (
        <Button size="sm" variant="ghost" icon={<Icon name="settings" size={15} />} onClick={() => navigate(`/settings/applications?edit=${encodeURIComponent(application)}`)}>{t('items.baseSettings')}</Button>
      )}
    </div>
  )
}
