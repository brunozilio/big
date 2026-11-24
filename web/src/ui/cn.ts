import { clsx, type ClassValue } from 'clsx'
import { extendTailwindMerge } from 'tailwind-merge'
import { currentLocale } from '@/i18n'

const radiusRoles = ['control-sm', 'control', 'card', 'overlay']

const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      'font-size': [{ text: ['2xs', 'md', 'body'] }],
      shadow: [{ shadow: ['card', 'pop', 'float'] }],
      rounded: [{ rounded: radiusRoles }],
      'rounded-t': [{ 'rounded-t': radiusRoles }],
      'rounded-b': [{ 'rounded-b': radiusRoles }],
      'rounded-l': [{ 'rounded-l': radiusRoles }],
      'rounded-r': [{ 'rounded-r': radiusRoles }],
    },
  },
})

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : many
}

export function relTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const future = diff < 0
  const m = Math.round(Math.abs(diff) / 60_000)
  const fmt = (v: string) => (future ? `em ${v}` : v)
  if (m < 1) return 'agora'
  if (m < 60) return fmt(`${m}min`)
  const h = Math.round(m / 60)
  if (h < 24) return fmt(`${h}h`)
  const d = Math.round(h / 24)
  return fmt(`${d}d`)
}

export function money(usd: number, currency: 'BRL' | 'USD' = 'BRL', usdToBrl = 5.4): string {
  const value = currency === 'BRL' ? usd * usdToBrl : usd
  return new Intl.NumberFormat(currentLocale(), { style: 'currency', currency, maximumFractionDigits: value < 10 ? 2 : 0 }).format(value)
}
