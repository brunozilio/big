import { motion } from 'framer-motion'
import type { AgentShape } from '@/api/contract'
import { useT } from '@/i18n'
import { cn } from './cn'

type Shape = AgentShape

const BODIES: Record<Shape, string> = {
  circle: 'M12 1.5a10.5 10.5 0 1 1 0 21a10.5 10.5 0 0 1 0-21z',
  squircle: 'M8 2h8c3.8 0 6 2.2 6 6v8c0 3.8-2.2 6-6 6H8c-3.8 0-6-2.2-6-6V8c0-3.8 2.2-6 6-6z',
  triangle: 'M10.2 3.4c.8-1.4 2.8-1.4 3.6 0l8.6 15.2c.8 1.4-.2 3.1-1.8 3.1H3.4c-1.6 0-2.6-1.7-1.8-3.1z',
  cloud: 'M7.5 21.5a5.5 5.5 0 0 1-1.3-10.8A6 6 0 0 1 17.6 8.4a5.6 5.6 0 0 1-1.1 13.1z',
  drop: 'M12 1.8c2.8 3.6 8.2 9 8.2 13a8.2 8.2 0 0 1-16.4 0c0-4 5.4-9.4 8.2-13z',
  pill: 'M7.5 2h9C19.5 2 22 4.5 22 7.5v9c0 3-2.5 5.5-5.5 5.5h-9C4.5 22 2 19.5 2 16.5v-9C2 4.5 4.5 2 7.5 2z',
}

const EYES: Record<Shape, { y: number; gap: number; h: number }> = {
  circle: { y: 8.6, gap: 4.6, h: 6 },
  squircle: { y: 8.4, gap: 4.4, h: 6.2 },
  triangle: { y: 12, gap: 3.8, h: 5 },
  cloud: { y: 11.4, gap: 4.4, h: 5.4 },
  drop: { y: 11.6, gap: 4.2, h: 5.2 },
  pill: { y: 8.8, gap: 4.6, h: 6 },
}

const COLORS = ['#1c1c1e', '#4fb39c', '#8b5cf6', '#3b82f6', '#ef6c2f', '#f0a235', '#e94b9b', '#8a5a3b']
const SHAPES: Shape[] = ['circle', 'squircle', 'triangle', 'cloud', 'drop', 'pill']

function hash(s: string) {
  let h = 2166136261
  for (const c of s) {
    h ^= c.charCodeAt(0)
    h = Math.imul(h, 16777619)
  }
  return Math.abs(h)
}

export function agentLook(seed: string): { shape: Shape; color: string } {
  const h = hash(seed)
  return { shape: SHAPES[h % SHAPES.length], color: COLORS[(h >> 3) % COLORS.length] }
}

export function AgentAvatar({ look, seed = 'agente', name, size = 22, working, className }: { look?: { shape: Shape; color: string } | null; seed?: string; name?: string; size?: number; working?: boolean; className?: string }) {
  const t = useT()
  const { shape, color } = look ?? agentLook(seed)
  const e = EYES[shape]
  const w = 2.6
  const x1 = 12 - e.gap / 2 - w / 2
  const x2 = 12 + e.gap / 2 - w / 2
  return (
    <motion.svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      aria-label={name ?? t('editor.agent')}
      className={cn('shrink-0', className)}
      animate={working ? { y: [0, -1.5, 0] } : { y: 0 }}
      transition={working ? { repeat: Infinity, duration: 1.2, ease: 'easeInOut' } : { duration: 0.2 }}
    >
      <path d={BODIES[shape]} fill={color} />
      <motion.g animate={working ? { scaleY: [1, 1, 0.15, 1] } : { scaleY: 1 }} transition={working ? { repeat: Infinity, duration: 3, times: [0, 0.9, 0.95, 1] } : undefined} style={{ originY: `${e.y + e.h / 2}px` }}>
        <rect x={x1} y={e.y} width={w} height={e.h} rx={w / 2} fill="#fff" transform={`rotate(-12 ${x1 + w / 2} ${e.y + e.h / 2})`} />
        <rect x={x2} y={e.y} width={w} height={e.h} rx={w / 2} fill="#fff" transform={`rotate(-12 ${x2 + w / 2} ${e.y + e.h / 2})`} />
      </motion.g>
    </motion.svg>
  )
}
