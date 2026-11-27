import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from '@/api'
import type { BaseBranches, BaseGap, Item } from '@/api/contract'
import { useT } from '@/i18n'
import { Button } from '@/ui/button'
import { Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { report } from './parts'

const ADD = '\u0000add'

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
  const [picked, setPicked] = useState('')
  const [adding, setAdding] = useState(false)
  const [name, setName] = useState(gap.missing)
  const [busy, setBusy] = useState(false)
  const source = picked || fallback
  const fresh = name.trim()
  const options = [...branches.map((value) => ({ value, label: value })), { value: ADD, label: t('items.baseAdd') }]
  const choose = (value: string) => {
    if (value === ADD) return setAdding(true)
    setAdding(false)
    setPicked(value)
  }
  const ready = adding ? fresh !== '' && source !== '' : source !== ''
  const proceed = () => {
    setBusy(true)
    const choice = adding ? { repository: gap.repository, base: fresh, from: source } : { repository: gap.repository, base: source }
    api().chooseBase(it.id, choice).catch(report).finally(() => setBusy(false))
  }
  const application = it.application?.id
  if (known && branches.length === 0) return <p className="text-sm text-muted">{t('items.baseNoBranches')}</p>
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-fg">{t('items.basePick')}</span>
        <Select size="sm" value={adding ? ADD : source} onValueChange={choose} options={options} label={t('items.basePick')} />
      </div>
      {adding && (
        <div className="flex flex-wrap items-center gap-2">
          <Input className="h-8 w-56" value={name} onChange={(e) => setName(e.target.value)} aria-label={t('items.baseNewName')} placeholder={t('items.baseNewName')} autoFocus />
          <span className="text-sm text-muted">{t('items.baseFrom', { base: source })}</span>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="primary" disabled={busy || !ready} onClick={proceed}>{t('items.baseContinue')}</Button>
        {application && (
          <Button size="sm" variant="ghost" icon={<Icon name="settings" size={15} />} onClick={() => navigate(`/settings/applications?edit=${encodeURIComponent(application)}`)}>{t('items.baseSettings')}</Button>
        )}
      </div>
    </div>
  )
}
