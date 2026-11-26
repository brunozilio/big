import { useMemo, type ReactNode } from 'react'
import type { ApplicationRepository, Binding, Bindings, RepositoryKind } from '@/api/contract'
import { useStore } from '@/api/store'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { FALLBACK_KINDS, isFallbackKind } from './applicationDraft'
import { useBoundLook } from './bindings'

export function useKinds(): RepositoryKind[] {
  const t = useT()
  const served = useStore((s) => s.connections?.repositoryKinds)
  return useMemo(() => (served && served.length > 0 ? served : FALLBACK_KINDS.map((key) => ({ key, label: t(`settings.app_kind_${key}`) }))), [served, t])
}

export function kindLabel(kinds: RepositoryKind[], name: string, t: ReturnType<typeof useT>): string {
  const known = kinds.find((k) => k.key === name)
  if (known) return known.label
  return isFallbackKind(name) ? t(`settings.app_kind_${name}`) : name
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
        const logo = source ? <ProviderLogo logo={source.provider.logo} name={source.provider.name} size={12} /> : undefined
        return <Fact key={capability} label={label} value={text} logo={logo} />
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
      {repository.kind && <Fact label={t('settings.app_kind')} value={kindLabel(kinds, repository.kind, t)} />}
      {repository.readOnly && <Fact label={t('settings.app_access')} value={t('settings.app_readOnly')} />}
      <BoundFacts bindings={repository.bindings} />
      <CommandChips icon="play" label={t('settings.app_run')} commands={repository.run} />
      <CommandChips icon="check" label={t('settings.app_verify')} commands={repository.verify} />
      <CommandChips icon="sTest" label={t('settings.app_e2e')} commands={e2e} />
    </>
  )
}
