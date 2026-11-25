import { currentLanguage, translate } from '@/i18n'

let source: () => number = () => Date.now()

export function now(): number {
  return source()
}

export function setClock(next: () => number) {
  source = next
}

export function minutesSince(iso: string): number {
  return Math.max(0, Math.floor((now() - new Date(iso).getTime()) / 60_000))
}

export function mins(t: number): string {
  return t < 60 ? `${t}m` : `${Math.floor(t / 60)}h${String(t % 60).padStart(2, '0')}`
}

export function ago(iso: string): string {
  return translate(currentLanguage(), 'app.ago', { time: mins(Math.max(1, minutesSince(iso))) })
}
