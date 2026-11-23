import { createAvatar } from '@dicebear/core'
import { notionists } from '@dicebear/collection'
import { useMemo } from 'react'
import { cn } from './cn'
import { iconData, logoData } from './iconData'

export type IconName = keyof typeof iconData

export function isIconName(name: string): name is IconName {
  return name in iconData
}

export function NamedIcon({ name, fallback = 'dashedCircle', ...rest }: { name: string; fallback?: IconName; size?: number; className?: string; strokeWidth?: number }) {
  return <Icon name={isIconName(name) ? name : fallback} {...rest} />
}

export function Icon({ name, size = 18, className, strokeWidth = 1.5 }: { name: IconName; size?: number; className?: string; strokeWidth?: number }) {
  const def = iconData[name]
  const body = def.body.replace(/stroke-width="[\d.]+"/g, `stroke-width="${strokeWidth}"`)
  return (
    <svg
      aria-hidden
      width={size}
      height={size}
      viewBox={`0 0 ${def.w} ${def.h}`}
      className={cn('shrink-0', className)}
      dangerouslySetInnerHTML={{ __html: body }}
    />
  )
}

const customLogos: Record<string, IconDefLike> = {
  grok: { w: 24, h: 24, body: '<rect width="24" height="24" rx="6" fill="#0a0a0a"/><g transform="translate(4.2 4.2) scale(0.65)"><path fill="#fff" fill-rule="evenodd" d="M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815"/></g>' },
  kiro: { w: 24, h: 24, body: '<rect width="24" height="24" rx="6" fill="#7c4dff"/><path d="M7 17.5V10a5 5 0 0 1 10 0v7.5l-1.7-1.3-1.6 1.3-1.7-1.3-1.7 1.3-1.6-1.3z" fill="#fff"/><circle cx="10.3" cy="10.4" r="1.1" fill="#7c4dff"/><circle cx="13.7" cy="10.4" r="1.1" fill="#7c4dff"/>' },
}

export interface IconDefLike {
  body: string
  w: number
  h: number
}

export function svgDef(markup: string): IconDefLike | null {
  const open = markup.match(/^\s*<svg\b([^>]*)>/i)
  const box = open?.[1].match(/viewBox="\s*[-\d.]+\s+[-\d.]+\s+([\d.]+)\s+([\d.]+)\s*"/i)
  const end = markup.lastIndexOf('</svg>')
  if (!open || !box || end < 0) return null
  return { w: Number(box[1]), h: Number(box[2]), body: markup.slice(open[0].length + markup.indexOf(open[0]), end) }
}

export function Logo({ name, size = 20, className, tile, def: given }: { name: string; size?: number; className?: string; tile?: boolean; def?: IconDefLike | null }) {
  const def = given ?? logoData[name] ?? customLogos[name]
  const svg = def ? (
    <svg aria-label={name} fill="currentColor" className="text-fg" width={tile ? size * 0.62 : size} height={tile ? size * 0.62 : size} viewBox={`0 0 ${def.w} ${def.h}`} preserveAspectRatio="xMidYMid meet" dangerouslySetInnerHTML={{ __html: def.body }} />
  ) : (
    <span className="font-semibold text-muted" style={{ fontSize: size * 0.45 }}>
      {name.slice(0, 1).toUpperCase()}
    </span>
  )
  if (!tile) return <span className={cn('inline-flex shrink-0 items-center justify-center', className)} style={{ width: size, height: size }}>{svg}</span>
  return (
    <span className={cn('inline-flex shrink-0 items-center justify-center rounded-[28%] bg-surface shadow-card', className)} style={{ width: size, height: size }}>
      {svg}
    </span>
  )
}

const avatarCache = new Map<string, string>()

export function avatarUri(seed: string): string {
  const hit = avatarCache.get(seed)
  if (hit) return hit
  const uri = createAvatar(notionists, { seed, backgroundColor: ['f4e8ff', 'ffe9d6', 'dff4ea', 'e3efff', 'fff3c4', 'ffe1ea'], backgroundType: ['solid'] }).toDataUri()
  avatarCache.set(seed, uri)
  return uri
}

export function AvatarImg({ seed, size = 24, className, ring = true }: { seed: string; size?: number; className?: string; ring?: boolean }) {
  const src = useMemo(() => avatarUri(seed), [seed])
  return <img src={src} alt="" width={size} height={size} className={cn('shrink-0 rounded-full object-cover', ring && 'ring-2 ring-bg', className)} style={{ width: size, height: size }} />
}
