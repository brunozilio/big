import { useEffect, useState } from 'react'
import { api } from '@/api'
import type { ApplicationRepository } from '@/api/contract'
import { Field, Select } from '@/ui/form'
import { useT } from '@/i18n'

const AUTOMATIC = 'auto'

export function useFlows(open: boolean): string[] {
  const [flows, setFlows] = useState<string[]>([])
  useEffect(() => {
    if (!open) return
    let alive = true
    api()
      .flows()
      .then((found) => alive && setFlows(found))
      .catch(() => alive && setFlows([]))
    return () => {
      alive = false
    }
  }, [open])
  return flows
}

export function withFlow(repository: ApplicationRepository, flow: string | undefined): ApplicationRepository {
  const { flow: _, ...rest } = repository
  return flow ? { ...rest, flow } : rest
}

export function FlowSelect({ value, flows, onChange }: { value: string | undefined; flows: string[]; onChange: (next: string | undefined) => void }) {
  const t = useT()
  const names = [...new Set([...flows, ...(value ? [value] : [])])]
  const options = [{ value: AUTOMATIC, label: t('settings.app_flowAuto') }, ...names.map((name) => ({ value: name, label: name }))]
  return (
    <Field label={t('settings.app_flow')}>
      <Select label={t('settings.app_flow')} className="w-full" value={value || AUTOMATIC} options={options} onValueChange={(v) => onChange(v === AUTOMATIC ? undefined : v)} />
    </Field>
  )
}
