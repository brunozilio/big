import * as RPopover from '@radix-ui/react-popover'
import { AnimatePresence, motion } from 'framer-motion'
import { useState, type ReactNode } from 'react'
import { AUTO_MODEL, type Model } from '@/api/contract'
import { useStore } from '@/api/store'
import { cn } from '@/ui/cn'
import { ProviderLogo } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { dur, easeOut, springSnap, tween, tweenClose } from '@/ui/motion'

const swap = { duration: dur.quick, ease: easeOut }

function ModelGlyph({ model, size = 14 }: { model: Model; size?: number }) {
  if (model.id === AUTO_MODEL || !model.logo) return <Icon name="sparkles" size={size} className="text-fg2" />
  return <ProviderLogo logo={model.logo} name={model.name} size={size} />
}

export function ModelPicker({ value, onChange }: { value: string | null; onChange: (id: string) => void }) {
  const [open, setOpen] = useState(false)
  const models = useStore((s) => s.models)
  const list = models?.models ?? []
  const current = list.find((m) => m.id === (value ?? models?.default)) ?? list.find((m) => m.id === AUTO_MODEL) ?? list[0]
  if (!current) return null
  return (
    <RPopover.Root open={open} onOpenChange={setOpen}>
      <RPopover.Trigger asChild>
        <motion.button layout transition={springSnap} aria-label={`Modelo: ${current.name}`} className={cn('relative inline-flex h-8 items-center gap-1.5 overflow-hidden rounded-full px-3 text-base transition-colors', open ? 'bg-inset text-muted' : 'text-fg2 hover:bg-inset')}>
          <AnimatePresence mode="popLayout" initial={false}>
            {open ? (
              <motion.span key="ph" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={swap} className="min-w-[96px] text-center">
                Selecionar modelo
              </motion.span>
            ) : (
              <motion.span key={current.id} initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 6 }} transition={swap} className="flex items-center gap-1.5">
                <ModelGlyph model={current} />
                <span className="text-fg2">{current.name}</span>
              </motion.span>
            )}
          </AnimatePresence>
        </motion.button>
      </RPopover.Trigger>
      <AnimatePresence>
        {open && (
          <RPopover.Portal forceMount>
            <RPopover.Content forceMount side="bottom" align="center" sideOffset={6} collisionPadding={12} onOpenAutoFocus={(e) => e.preventDefault()} className="z-50 outline-none">
              <motion.div initial={{ opacity: 0, scale: 0.97 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.99, transition: tweenClose }} transition={tween} style={{ borderRadius: 18, transformOrigin: 'var(--radix-popover-content-transform-origin)' }} className="w-[244px] overflow-hidden bg-surface shadow-pop">
                <div className="scrollbar-thin max-h-[300px] overflow-y-auto p-1.5">
                  <p className="px-2.5 pb-1 pt-1 text-xs text-muted">Selecionar modelo</p>
                  {list.map((m) => (
                    <ModelRow
                      key={m.id}
                      selected={m.id === current.id}
                      onSelect={() => {
                        onChange(m.id)
                        setOpen(false)
                      }}
                    >
                      <span className="flex items-center gap-2 text-base text-fg2">
                        <ModelGlyph model={m} />
                        <span title={m.name} className="truncate">{m.name}</span>
                      </span>
                      {m.description && <span className="block pl-[22px] text-xs text-muted">{m.description}</span>}
                    </ModelRow>
                  ))}
                </div>
              </motion.div>
            </RPopover.Content>
          </RPopover.Portal>
        )}
      </AnimatePresence>
    </RPopover.Root>
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
