import { useState } from 'react'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { Popover } from '@/ui/overlay'

const TIMES = ['18:00', '19:00', '20:00', '21:00', '22:00', '23:00', '00:00', '01:00', '02:00']

function iso(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

function days(): Date[] {
  const base = new Date()
  return Array.from({ length: 14 }, (_, i) => new Date(base.getFullYear(), base.getMonth(), base.getDate() + i + 1))
}

export function windowLabel(value: string): string {
  const [day, time] = value.split('T')
  const [y, m, d] = day.split('-').map(Number)
  const date = new Date(y, m - 1, d)
  const text = date.toLocaleDateString('pt-BR', { weekday: 'short', day: 'numeric', month: 'short' }).replace(/\./g, '')
  return `${text} · ${time}`
}

function Day({ d, on, onPick }: { d: Date; on: boolean; onPick: () => void }) {
  const weekend = d.getDay() === 0 || d.getDay() === 6
  return (
    <button onClick={onPick} className={cn('flex h-12 flex-col items-center justify-center rounded-control-sm text-sm transition-colors', on ? 'bg-primary text-primary-fg' : 'text-fg2 hover:bg-hover', weekend && !on && 'text-muted')}>
      <span className={cn('text-2xs uppercase', on ? 'text-primary-fg/70' : 'text-faint')}>{d.toLocaleDateString('pt-BR', { weekday: 'short' }).replace('.', '')}</span>
      <span className="tabular-nums">{d.getDate()}</span>
    </button>
  )
}

export function WindowPicker({ value, onChange }: { value: string | undefined; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false)
  const [day, setDay] = useState(value?.split('T')[0] ?? iso(days()[0]))
  const time = value?.split('T')[1]
  const trigger = (
    <button className={cn('inline-flex h-8 items-center gap-1.5 rounded-control-sm px-2.5 text-sm transition-colors', value ? 'bg-inset text-fg hover:bg-active' : 'border border-dashed border-line-strong text-muted hover:border-fg/30 hover:text-fg2')}>
      <Icon name="calendar" size={14} className={value ? 'text-muted' : undefined} />
      {value ? windowLabel(value) : 'Definir janela'}
    </button>
  )
  return (
    <Popover open={open} onOpenChange={setOpen} trigger={trigger} className="w-[312px] p-3">
      <div className="mb-2 text-xs font-medium text-muted">Dia</div>
      <div className="grid grid-cols-7 gap-1">
        {days().map((d) => <Day key={iso(d)} d={d} on={iso(d) === day} onPick={() => setDay(iso(d))} />)}
      </div>
      <div className="mb-2 mt-3 text-xs font-medium text-muted">Horário</div>
      <div className="grid grid-cols-5 gap-1">
        {TIMES.map((t) => (
          <button key={t} onClick={() => { onChange(`${day}T${t}`); setOpen(false) }} className={cn('h-8 rounded-control-sm text-sm tabular-nums transition-colors', value === `${day}T${t}` ? 'bg-primary text-primary-fg' : time === t ? 'bg-inset text-fg' : 'text-fg2 hover:bg-hover')}>
            {t}
          </button>
        ))}
      </div>
    </Popover>
  )
}
