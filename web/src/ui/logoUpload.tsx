import { AnimatePresence, motion } from 'framer-motion'
import { tween, tweenClose } from './motion'
import { useRef, useState, type ReactNode } from 'react'
import { Button } from './button'
import { cn } from './cn'
import { readImageFile } from './domain'
import { Icon } from './icon'

export const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/svg+xml'] as const
const MAX_IMAGE_BYTES = 1024 * 1024

export function imageProblem(file: { type: string; size: number }): string | null {
  if (!(IMAGE_TYPES as readonly string[]).includes(file.type)) return 'Use uma imagem PNG, JPG ou SVG'
  if (file.size > MAX_IMAGE_BYTES) return 'A imagem pode ter até 1 MB'
  return null
}

export function LogoUpload({ value, onChange, preview, label = 'Logo' }: { value: string | null; onChange: (logo: string | null) => void; preview: ReactNode; label?: string }) {
  const input = useRef<HTMLInputElement>(null)
  const [over, setOver] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const take = async (file: File | undefined) => {
    if (!file) return
    const refused = imageProblem(file)
    setProblem(refused)
    if (refused) return
    onChange(await readImageFile(file))
  }
  return (
    <div
      className={cn('flex items-center gap-5 rounded-card p-1 transition-colors', over && 'bg-info-soft')}
      onDragOver={(e) => {
        e.preventDefault()
        setOver(true)
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault()
        setOver(false)
        take(e.dataTransfer.files[0])
      }}
    >
      <AnimatePresence mode="wait" initial={false}>
        <motion.span key={value ?? 'default'} initial={{ scale: 0.96, opacity: 0, filter: 'blur(2px)' }} animate={{ scale: 1, opacity: 1, filter: 'blur(0px)' }} exit={{ scale: 0.96, opacity: 0, filter: 'blur(0px)', transition: tweenClose }} transition={tween}>
          {preview}
        </motion.span>
      </AnimatePresence>
      <div className="min-w-0 flex-1">
        <p className="text-base font-medium text-fg">{label}</p>
        <p className={cn('mt-0.5 text-sm', problem ? 'text-danger-fg' : 'text-muted')}>{problem ?? 'PNG, JPG ou SVG · mínimo 400×400 px · até 1 MB'}</p>
        <div className="mt-2.5 flex gap-2">
          <Button size="sm" icon={<Icon name="upload" size={15} />} onClick={() => input.current?.click()}>
            {value ? 'Trocar' : 'Enviar logo'}
          </Button>
          {value && (
            <Button size="sm" variant="ghost" onClick={() => onChange(null)}>
              Remover
            </Button>
          )}
        </div>
      </div>
      <input
        ref={input}
        type="file"
        hidden
        accept={IMAGE_TYPES.join(',')}
        onChange={(e) => {
          take(e.target.files?.[0])
          e.target.value = ''
        }}
      />
    </div>
  )
}
