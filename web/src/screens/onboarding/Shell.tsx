import type { ReactNode } from 'react'
import type { T } from '@/i18n'
import { LanguagePicker } from '@/i18n/LanguagePicker'
import { cn } from '@/ui/cn'
import { ProductMark } from '@/ui/domain'
import { Icon } from '@/ui/icon'

export const INSTALLED_KEY = 'onboarding.installed'

export function markInstalled() {
  try {
    sessionStorage.setItem(INSTALLED_KEY, '1')
  } catch {
    return
  }
}

export function wasInstalled(): boolean {
  try {
    return sessionStorage.getItem(INSTALLED_KEY) === '1'
  } catch {
    return false
  }
}

export function installerLabels(t: T): string[] {
  return [t('app.stageServerLabel'), t('app.stageAdminLabel')]
}

export function journeyLabels(t: T): string[] {
  return [...installerLabels(t), t('app.stageConnectionsLabel'), t('app.stageAiLabel'), t('app.stageApplicationLabel')]
}

export function Steps({ labels, current }: { labels: string[]; current: number }) {
  const many = labels.length > 3
  return (
    <div className={cn('mb-6 flex items-center text-sm text-muted', many ? 'gap-1.5' : 'gap-2')}>
      {labels.map((label, i) => (
        <span key={label} data-step={i < current ? 'done' : i === current ? 'current' : 'next'} className={cn('flex shrink-0 items-center', many ? 'gap-1.5' : 'gap-2')}>
          {i > 0 && <span className={cn('h-px bg-border', many ? 'w-3' : 'w-6')} />}
          <span className={cn('flex size-5 items-center justify-center rounded-full text-xs tabular-nums', i < current ? 'bg-success text-white' : i === current ? 'bg-fg text-bg' : 'bg-inset text-faint')}>
            {i < current ? <Icon name="check" size={11} strokeWidth={2.6} /> : i + 1}
          </span>
          <span className={cn(i === current ? 'text-fg' : undefined, many && i !== current && 'hidden sm:inline')}>{label}</span>
        </span>
      ))}
    </div>
  )
}

export function StageHeading({ title, text }: { title: string; text?: string }) {
  return (
    <>
      <h1 className="text-2xl font-medium tracking-tighter2 text-fg">{title}</h1>
      <p className="mb-6 mt-1.5 whitespace-pre-line text-base text-muted">{text}</p>
    </>
  )
}

export function OnboardingShell({ labels, current, language, className, outside, children }: { labels: string[]; current: number; language?: ReactNode; className?: string; outside?: ReactNode; children: ReactNode }) {
  const width = labels.length > 3 ? 'max-w-[640px]' : 'max-w-[560px]'
  return (
    <div className={cn('flex min-h-full flex-col items-center bg-subtle px-4 py-10', className)}>
      <div className={cn('mb-8 flex w-full items-center justify-between', width)}>
        <ProductMark />
        {language ?? <LanguagePicker className="w-36" />}
      </div>
      <div className={cn('w-full rounded-card bg-bg shadow-card', width)}>
        <div className="flex max-h-[calc(100dvh-160px)] min-h-[520px] flex-col p-6 md:p-8">
          <Steps labels={labels} current={current} />
          {children}
        </div>
      </div>
      {outside}
    </div>
  )
}
