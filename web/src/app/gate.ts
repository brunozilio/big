import type { Application, OnboardingState } from '@/api/contract'

export interface Resumed {
  provider: string
  flow: string | null
}

export function needsOnboarding(ready: boolean, applications: Application[] | null, onboarding: OnboardingState | null): boolean {
  return ready && applications !== null && applications.length === 0 && !onboarding?.skipped
}

export function returnedTo(path: string, search: string): Resumed | null {
  if (!path.startsWith('/settings/connections')) return null
  const params = new URLSearchParams(search)
  const provider = params.get('connect')
  return provider ? { provider, flow: params.get('flow') } : null
}
