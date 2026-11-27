import { createContext, useContext, useState } from 'react'
import { api } from '@/api'
import type { Finding, LinkRef } from '@/api/contract'
import { errorText, useProviderOf } from '@/api/store'
import { toast } from '@/ui/toast'
import { Button } from '@/ui/button'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'

export interface TaskTarget {
  connection: string
  project: string
}

const TaskContext = createContext<TaskTarget | null>(null)

export const TaskTargetProvider = TaskContext.Provider

export const useTaskTarget = () => useContext(TaskContext)

export function taskBody(f: Finding): string {
  const place = f.file ? (f.line ? `${f.file}:${f.line}` : f.file) : f.component ? [f.component, f.version].filter(Boolean).join(' ') : ''
  const lines = [
    `Id: ${f.id}`,
    `Severity: ${f.severity}`,
    f.cwe ? `CWE-${f.cwe.id} · ${f.cwe.name}` : '',
    f.cve ? f.cve.id : '',
    place ? `Where: ${place}` : '',
    f.fixedIn ? `Fixed in: ${f.fixedIn}` : '',
    f.url ?? '',
  ].filter(Boolean)
  return f.description ? `${lines.join('\n')}\n\n${f.description}` : lines.join('\n')
}

function Mark({ connection }: { connection: string }) {
  const provider = useProviderOf(connection)
  return provider ? <ProviderLogo logo={provider.logo} name={provider.name} size={14} /> : <Icon name="sTicket" size={14} />
}

export function TaskButton({ f, target }: { f: Finding; target: TaskTarget }) {
  const t = useT()
  const [made, setMade] = useState<LinkRef | null>(null)
  const [busy, setBusy] = useState(false)
  if (made) {
    const body = <><Mark connection={target.connection} />{t('blocks.findingsTask', { key: made.key })}</>
    if (!made.url) return <span className="inline-flex items-center gap-1.5 text-sm text-fg2">{body}</span>
    return <a href={made.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 text-sm text-accent-fg hover:underline">{body}</a>
  }
  const create = () => {
    setBusy(true)
    api()
      .call(target.connection, 'tracker.create_issue', { title: `${t('blocks.findingsTaskPrefix')}: ${f.title}`, description: taskBody(f), project: target.project })
      .then(setMade)
      .catch((e: unknown) => toast.error(errorText(e)))
      .finally(() => setBusy(false))
  }
  return (
    <span className="inline-flex items-center gap-1.5">
      <Button size="sm" loading={busy} icon={<Mark connection={target.connection} />} onClick={create}>{t('blocks.findingsChoiceTask')}</Button>
    </span>
  )
}
