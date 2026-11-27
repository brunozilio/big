import { type ReactNode } from 'react'
import type { ApplicationRepository, Binding, Bindings, RepositoryKind } from '@/api/contract'
import { useStore } from '@/api/store'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { useBoundLook } from './bindings'

const NO_KINDS: RepositoryKind[] = []

export function useKinds(): RepositoryKind[] {
  return useStore((s) => s.connections?.repositoryKinds) ?? NO_KINDS
}

export function kindLabel(kinds: RepositoryKind[], name: string): string {
  return kinds.find((k) => k.key === name)?.label ?? name
}

export function Fact({ label, value, logo }: { label: string; value: string; logo?: ReactNode }) {
  return (
    <span className="inline-flex h-6 min-w-0 max-w-full items-center gap-1.5 rounded-full border bg-surface px-2 text-xs">
      <span className="shrink-0 text-faint">{label}</span>
      {logo}
      <span className="truncate font-medium text-fg2">{value}</span>
    </span>
  )
}

export function BoundFacts({ bindings }: { bindings: Bindings | undefined }) {
  const look = useBoundLook()
  return (
    <>
      {Object.entries(bindings ?? {}).map(([capability, binding]: [string, Binding]) => {
        const { label, text, source } = look(capability, binding)
        if (!source) return <Fact key={capability} label={label} value={text} />
        return (
          <span key={capability} title={label} className="inline-flex h-6 min-w-0 max-w-full items-center gap-1.5 rounded-full border bg-surface px-2 text-xs">
            <ProviderLogo logo={source.provider.logo} name={source.provider.name} size={14} />
            <span className="truncate font-medium text-fg2">{text}</span>
          </span>
        )
      })}
    </>
  )
}

function CommandChips({ icon, label, commands }: { icon: 'play' | 'check' | 'sTest'; label: string; commands: string[] }) {
  return (
    <>
      {commands.filter((c) => c.trim()).map((c, i) => (
        <span key={`${c}-${i}`} title={label} className="inline-flex h-6 min-w-0 max-w-[16rem] items-center gap-1 rounded-control-sm bg-inset px-1.5 font-mono text-xs text-fg2">
          <Icon name={icon} size={11} className="shrink-0 text-faint" />
          <span className="truncate">{c}</span>
        </span>
      ))}
    </>
  )
}

export function RepositoryFacts({ repository }: { repository: ApplicationRepository }) {
  const t = useT()
  const kinds = useKinds()
  const e2e = (repository.e2e ?? []).map((c) => (repository.e2eDirectory ? `${repository.e2eDirectory}: ${c}` : c))
  return (
    <>
      {repository.kind && <Fact label={t('settings.app_kind')} value={kindLabel(kinds, repository.kind)} />}
      {repository.readOnly && <Fact label={t('settings.app_access')} value={t('settings.app_readOnly')} />}
      <BoundFacts bindings={repository.bindings} />
      <CommandChips icon="play" label={t('settings.app_run')} commands={repository.run} />
      <CommandChips icon="check" label={t('settings.app_verify')} commands={repository.verify} />
      <CommandChips icon="sTest" label={t('settings.app_e2e')} commands={e2e} />
    </>
  )
}
