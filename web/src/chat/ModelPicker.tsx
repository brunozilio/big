import * as RPopover from '@radix-ui/react-popover'
import { AnimatePresence, animate, motion, useMotionValue } from 'framer-motion'
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import { EFFORTS, type Effort, type Model } from '@/api/contract'
import { useStore } from '@/api/store'
import { cn } from '@/ui/cn'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { dur, easeOut, IconSwap, springSnap, tween, tweenClose } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'

const swap = { duration: dur.quick, ease: easeOut }

export const effortLabel: Record<Effort, string> = { low: 'Baixo', medium: 'Médio', high: 'Alto', xhigh: 'Muito alto', max: 'Máximo' }

export const DEFAULT_EFFORT: Effort = 'medium'

export const AI_PROVIDERS = '/settings/connections?group=Provedores%20de%20IA'

type View = 'effort' | 'model'

function shortName(name: string) {
  return name.replace(/^Claude /, '')
}

export function fittedEffort(levels: readonly Effort[], effort: Effort): Effort {
  if (!levels.length || levels.includes(effort)) return effort
  const wanted = EFFORTS.indexOf(effort)
  const below = [...levels].reverse().find((level) => EFFORTS.indexOf(level) <= wanted)
  return below ?? levels[0] ?? effort
}

function ModelGlyph({ model, size = 14, aligned }: { model: Model; size?: number; aligned?: boolean }) {
  if (!model.logo) return aligned ? <span aria-hidden style={{ width: size, height: size }} className="shrink-0" /> : null
  return <ProviderLogo logo={model.logo} name={model.name} size={size} />
}

export interface ModelGroup {
  provider: string
  name: string | null
  models: Model[]
}

export function groupByProvider(models: readonly Model[], names: ReadonlyMap<string, string>): ModelGroup[] {
  const groups: ModelGroup[] = []
  for (const model of models) {
    const provider = model.provider
    const found = groups.find((g) => g.provider === provider)
    if (found) found.models.push(model)
    else groups.push({ provider, name: names.get(provider) ?? provider, models: [model] })
  }
  return groups
}

function useProviderNames() {
  const providers = useStore((s) => s.connections?.providers)
  return new Map((providers ?? []).map((p) => [p.key, p.name]))
}

export function useModels() {
  const models = useStore((s) => s.models)
  return { loaded: models !== null, list: models?.models ?? [], defaultId: models?.default ?? null }
}

export function ModelPicker({
  value,
  onChange,
  effort,
  onEffort,
  fast,
  onFast,
}: {
  value: string | null
  onChange: (id: string | null) => void
  effort: Effort
  onEffort: (e: Effort) => void
  fast: boolean
  onFast: (v: boolean) => void
}) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<View>('effort')
  const navigate = useNavigate()
  const { loaded, list, defaultId } = useModels()
  const names = useProviderNames()
  const current = list.find((m) => m.id === (value ?? defaultId)) ?? list[0]

  useEffect(() => {
    if (!open) {
      const t = setTimeout(() => setView('effort'), 180)
      return () => clearTimeout(t)
    }
  }, [open])

  if (!loaded) return null
  if (!current) {
    return (
      <Tooltip content="Nenhum modelo disponível">
        <button onClick={() => navigate(AI_PROVIDERS)} className="inline-flex h-8 items-center gap-1.5 rounded-full bg-danger-soft px-3 text-sm text-danger-fg transition-opacity hover:opacity-80">
          Conecte um provedor de IA
          <Icon name="right" size={12} />
        </button>
      </Tooltip>
    )
  }

  const levels = current.efforts
  const dialed = levels.length > 0
  const isDefault = (value === null || value === defaultId) && effort === DEFAULT_EFFORT && !fast
  const choose = (m: Model) => {
    onChange(m.id === defaultId ? null : m.id)
    const fitted = fittedEffort(m.efforts, effort)
    if (fitted !== effort) onEffort(fitted)
    if (!m.fast && fast) onFast(false)
    setView('effort')
  }

  return (
    <RPopover.Root open={open} onOpenChange={setOpen}>
      <RPopover.Trigger asChild>
        <motion.button
          layout
          transition={springSnap}
          aria-label={`Modelo: ${current.name}`}
          className={cn('relative inline-flex h-8 items-center gap-1.5 overflow-hidden rounded-full px-3 text-base transition-colors', open ? 'bg-inset text-muted' : 'text-fg2 hover:bg-inset')}
        >
          <AnimatePresence mode="popLayout" initial={false}>
            {open ? (
              <motion.span key={`ph-${view}`} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={swap} className="min-w-[96px] text-center">
                {view === 'effort' ? 'Selecionar esforço' : 'Selecionar modelo'}
              </motion.span>
            ) : (
              <motion.span key="value" initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 6 }} transition={swap} className="flex items-center gap-1.5">
                {fast ? <Bolt filled size={13} className="text-fg" /> : <ModelGlyph model={current} />}
                <span className="text-fg2">{shortName(current.name)}</span>
                {dialed && <span className="text-muted">{effortLabel[effort]}</span>}
              </motion.span>
            )}
          </AnimatePresence>
        </motion.button>
      </RPopover.Trigger>
      <AnimatePresence>
        {open && (
          <RPopover.Portal forceMount>
            <RPopover.Content forceMount side="bottom" align="center" sideOffset={6} collisionPadding={12} onOpenAutoFocus={(e) => e.preventDefault()} className="z-50 outline-none">
              <motion.div
                layout
                initial={{ opacity: 0, scale: 0.97 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.99, transition: tweenClose }}
                transition={tween}
                style={{ borderRadius: 18, transformOrigin: 'var(--radix-popover-content-transform-origin)' }}
                className="overflow-hidden bg-surface shadow-pop"
              >
                <AnimatePresence mode="popLayout" initial={false}>
                  {view === 'effort' ? (
                    <motion.div key="effort" initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -8 }} transition={swap} className="w-[244px] p-2.5">
                      <div className="flex items-start gap-1">
                        <FastToggle fast={fast} available={current.fast} onFast={onFast} />
                        <div className="min-w-0 flex-1 pt-0.5 text-center">
                          <AnimatePresence mode="popLayout" initial={false}>
                            <motion.p key={dialed ? effort : 'none'} initial={{ opacity: 0, y: 5 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -5 }} transition={swap} className={cn('text-md font-medium leading-5', dialed ? 'text-fg' : 'text-muted')}>
                              {dialed ? effortLabel[effort] : 'Sem ajuste de esforço'}
                            </motion.p>
                          </AnimatePresence>
                          <button onClick={() => setView('model')} className="inline-flex max-w-full items-center gap-0.5 rounded-sm px-1 text-xs text-muted transition-colors hover:text-fg">
                            <span title={current.name} className="truncate">{current.name}</span>
                            <Icon name="right" size={12} className="shrink-0" />
                          </button>
                        </div>
                        <Tooltip content="Restaurar padrão">
                          <motion.button
                            whileTap={{ rotate: -180 }}
                            aria-label="Restaurar padrão"
                            disabled={isDefault}
                            onClick={() => {
                              onChange(null)
                              onEffort(DEFAULT_EFFORT)
                              onFast(false)
                            }}
                            className="flex size-7 shrink-0 items-center justify-center rounded-full text-faint transition-colors hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
                          >
                            <Icon name="refresh" size={14} />
                          </motion.button>
                        </Tooltip>
                      </div>
                      <EffortSlider value={effort} levels={dialed ? levels : EFFORTS} disabled={!dialed} onChange={onEffort} />
                    </motion.div>
                  ) : (
                    <motion.div key="model" initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 8 }} transition={swap} className="w-[244px]">
                      <div className="scrollbar-thin max-h-[300px] overflow-y-auto p-1.5">
                        {groupByProvider(list, names).map((group) => (
                          <div key={group.provider} role="group" aria-label={group.name ?? undefined} className="[&+&]:mt-1">
                            {group.name && <p className="px-2.5 pb-1 pt-1.5 text-xs text-muted">{group.name}</p>}
                            {group.models.map((m) => (
                              <ModelRow key={m.id} selected={m.id === current.id} onSelect={() => choose(m)}>
                                <span className="flex items-center gap-2 text-base text-fg2">
                                  <ModelGlyph model={m} aligned />
                                  <span title={m.name} className="truncate">{m.name}</span>
                                </span>
                              </ModelRow>
                            ))}
                          </div>
                        ))}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </motion.div>
            </RPopover.Content>
          </RPopover.Portal>
        )}
      </AnimatePresence>
    </RPopover.Root>
  )
}

function FastToggle({ fast, available, onFast }: { fast: boolean; available: boolean; onFast: (v: boolean) => void }) {
  return (
    <Tooltip content={!available ? 'Modo rápido indisponível para este modelo' : fast ? 'Modo rápido ligado' : 'Modo rápido'}>
      <motion.button
        whileTap={available ? { scale: 0.97 } : undefined}
        aria-pressed={fast}
        aria-disabled={!available}
        aria-label="Modo rápido"
        onClick={() => available && onFast(!fast)}
        className={cn('flex size-7 shrink-0 items-center justify-center rounded-full transition-colors', available ? 'hover:bg-hover' : 'cursor-default opacity-40', fast ? 'text-fg' : 'text-faint')}
      >
        <IconSwap k={fast ? 'on' : 'off'}>
          <Bolt filled={fast} size={15} />
        </IconSwap>
      </motion.button>
    </Tooltip>
  )
}

function ModelRow({ selected, onSelect, children }: { selected: boolean; onSelect: () => void; children: ReactNode }) {
  return (
    <button onClick={onSelect} className="flex w-full items-center gap-2 rounded-control px-2.5 py-[7px] text-left transition-colors hover:bg-hover">
      <span className="min-w-0 flex-1">{children}</span>
      {selected && <Icon name="check" size={15} className="shrink-0 text-muted" />}
    </button>
  )
}

const THUMB = 22
const PAD = 2

export function EffortSlider({ value, levels, disabled, onChange }: { value: Effort; levels: readonly Effort[]; disabled?: boolean; onChange: (e: Effort) => void }) {
  const track = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const [dragging, setDragging] = useState(false)
  const index = Math.max(0, levels.indexOf(fittedEffort(levels, value)))
  const steps = Math.max(1, levels.length - 1)
  const usable = Math.max(0, width - THUMB - PAD * 2)
  const x = useMotionValue(0)
  const fill = useWidth(x)

  useEffect(() => {
    const el = track.current
    if (!el) return
    const ro = new ResizeObserver(() => setWidth(el.clientWidth))
    ro.observe(el)
    setWidth(el.clientWidth)
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    if (dragging) return
    const controls = animate(x, (usable * index) / steps, springSnap)
    return () => controls.stop()
  }, [index, usable, steps, dragging, x])

  const pick = (clientX: number) => {
    const el = track.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    const raw = Math.min(usable, Math.max(0, clientX - rect.left - PAD - THUMB / 2))
    x.set(raw)
    const next = levels[Math.round((raw / (usable || 1)) * steps)]
    if (next && next !== value) onChange(next)
  }

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId)
    setDragging(true)
    pick(e.clientX)
  }

  return (
    <div
      ref={track}
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label="Esforço de raciocínio"
      aria-disabled={disabled}
      aria-valuemin={0}
      aria-valuemax={steps}
      aria-valuenow={index}
      aria-valuetext={effortLabel[levels[index] ?? value]}
      onPointerDown={onPointerDown}
      onPointerMove={(e) => dragging && pick(e.clientX)}
      onPointerUp={() => setDragging(false)}
      onPointerCancel={() => setDragging(false)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowRight' || e.key === 'ArrowUp') onChange(levels[Math.min(steps, index + 1)] ?? value)
        if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') onChange(levels[Math.max(0, index - 1)] ?? value)
      }}
      className={cn('relative mt-2.5 h-[26px] cursor-pointer touch-none select-none rounded-full bg-inset outline-none focus-visible:shadow-[0_0_0_2px_rgb(var(--focus))]', disabled && 'pointer-events-none opacity-40')}
    >
      <motion.span style={{ width: fill }} className="absolute inset-y-0 left-0 rounded-full bg-primary" />
      {levels.map((e, i) => (
        <span
          key={e}
          style={{ left: PAD + THUMB / 2 + (usable * i) / steps - 2 }}
          className={cn('pointer-events-none absolute top-1/2 size-1 -translate-y-1/2 rounded-full transition-colors', i < index ? 'bg-primary-fg/60' : 'bg-faint/60', i === index && 'opacity-0')}
        />
      ))}
      <motion.span
        style={{ x, width: THUMB, height: THUMB, top: PAD, left: PAD }}
        animate={{ scale: dragging ? 1.08 : 1 }}
        transition={springSnap}
        className="absolute rounded-full bg-white shadow-[0_1px_4px_rgb(0_0_0/0.25),0_0_0_0.5px_rgb(0_0_0/0.08)]"
      />
    </div>
  )
}

function useWidth(x: ReturnType<typeof useMotionValue<number>>) {
  const w = useMotionValue(0)
  useEffect(() => {
    const sync = (v: number) => w.set(v + PAD * 2 + THUMB)
    sync(x.get())
    return x.on('change', sync)
  }, [x, w])
  return w
}

function Bolt({ filled, size = 15, className }: { filled?: boolean; size?: number; className?: string }) {
  return <Icon name="bolt" size={size} strokeWidth={1.8} className={cn(filled && '[&_path]:fill-current', className)} />
}
