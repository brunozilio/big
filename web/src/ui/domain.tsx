import { motion } from 'framer-motion'
import { useStore } from '@/api/store'
import type { Person } from '@/api/contract'
import { AgentAvatar } from './agentAvatar'
import { cn } from './cn'
import { Avatar } from './display'
import { Logo } from './icon'

export function ProductMark({ size = 22, className }: { size?: number; className?: string }) {
  const branding = useStore((s) => s.branding)
  const name = branding?.name ?? ''
  if (branding?.logo) {
    return <img src={branding.logo} alt={name} className={cn('shrink-0 object-contain', className)} style={{ width: size, height: size, borderRadius: size * 0.23 }} />
  }
  return (
    <span role="img" aria-label={name} className={cn('inline-flex shrink-0 items-center justify-center bg-primary font-semibold text-primary-fg', className)} style={{ width: size, height: size, borderRadius: size * 0.23, fontSize: size * 0.48 }}>
      {name.slice(0, 1).toUpperCase()}
    </span>
  )
}

export type StatusGlyph = 'todo' | 'in_progress' | 'in_review' | 'done'

const glyphColor: Record<StatusGlyph, string> = { todo: 'text-st-todo', in_progress: 'text-st-progress', in_review: 'text-st-review', done: 'text-st-done' }

export function StatusIcon({ status, className, size = 16 }: { status: StatusGlyph; className?: string; size?: number }) {
  const common = { width: size, height: size, viewBox: '0 0 16 16', className: cn('shrink-0', glyphColor[status], className) }
  if (status === 'todo')
    return (
      <svg {...common}>
        <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray="2.2 2.1" />
      </svg>
    )
  if (status === 'in_progress')
    return (
      <svg {...common}>
        <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <motion.path d="M8 8V4.6M8 8l2.4 1.4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none" style={{ originX: '8px', originY: '8px' }} />
      </svg>
    )
  if (status === 'in_review')
    return (
      <svg {...common}>
        <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 3.6a4.4 4.4 0 0 1 0 8.8z" fill="currentColor" />
      </svg>
    )
  return (
    <svg {...common}>
      <circle cx="8" cy="8" r="7" fill="currentColor" />
      <path d="M5.2 8.2l1.9 1.9 3.8-4" fill="none" stroke="white" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function PersonAvatar({ person, size = 22 }: { person: Person; size?: number }) {
  if (person.avatar?.kind === 'agent') return <span className="flex shrink-0"><AgentAvatar look={person.avatar} name={person.name} size={size} /></span>
  return <Avatar name={person.name} photo={person.avatar?.kind === 'image' ? person.avatar.url : null} size={size} />
}

export function personName(person: Person): string {
  return person.you ? 'Você' : person.name
}

export function isImageSource(logo: string): boolean {
  return /^(https?:|data:|\/)/.test(logo)
}

export function ProviderLogo({ logo, name, size = 20, className, tile }: { logo: string; name?: string; size?: number; className?: string; tile?: boolean }) {
  if (isImageSource(logo)) {
    return <img src={logo} alt={name ?? ''} className={cn('shrink-0 object-contain', tile && 'rounded-[28%] bg-surface p-[18%] shadow-card', className)} style={{ width: size, height: size }} />
  }
  return <Logo name={logo} size={size} className={className} tile={tile} />
}

export function LogoTile({ logo, name, size = 44, className }: { logo: string; name?: string; size?: number; className?: string }) {
  return (
    <span className={cn('inline-flex shrink-0 items-center justify-center rounded-card bg-surface shadow-card', className)} style={{ width: size, height: size, borderRadius: size * 0.28 }}>
      <ProviderLogo logo={logo} name={name} size={size * 0.55} />
    </span>
  )
}

export function readImageFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
}
