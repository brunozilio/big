import { useEffect, useState } from 'react'
import { api } from '@/api'
import type { ApplicationRepository, Flows } from '@/api/contract'
import { Field, Select } from '@/ui/form'
import { useT } from '@/i18n'

const NONE: Flows = { names: [], default: '' }

export function useFlows(open: boolean): Flows {
  const [flows, setFlows] = useState<Flows>(NONE)
  useEffect(() => {
    if (!open) return
    let alive = true
    api()
      .flows()
      .then((found) => alive && setFlows(found))
      .catch(() => alive && setFlows(NONE))
    return () => {
      alive = false
    }
  }, [open])
  return flows
}

export function withFlow(repository: ApplicationRepository, flow: string): ApplicationRepository {
  return { ...repository, flow }
}

export function FlowSelect({ value, flows, onChange }: { value: string | undefined; flows: Flows; onChange: (next: string) => void }) {
  const t = useT()
  const shown = value || flows.default
  const names = [...new Set([...flows.names, ...(shown ? [shown] : [])])]
  const options = names.map((name) => ({ value: name, label: name }))
  return (
    <Field label={t('settings.app_flow')}>
      <Select label={t('settings.app_flow')} className="w-full" value={shown} options={options} onValueChange={onChange} />
    </Field>
  )
}
