import * as RDialog from '@radix-ui/react-dialog'
import * as RDropdown from '@radix-ui/react-dropdown-menu'
import * as RPopover from '@radix-ui/react-popover'
import * as RTooltip from '@radix-ui/react-tooltip'
import { AnimatePresence, motion } from 'framer-motion'
import type { ReactNode } from 'react'
import { useT } from '@/i18n'
import { cn } from './cn'
import { Icon } from './icon'
import { dur, easeOut, tween, tweenClose } from './motion'

export function TooltipProvider({ children }: { children: ReactNode }) {
  return <RTooltip.Provider delayDuration={300} skipDelayDuration={500}>{children}</RTooltip.Provider>
}

export function Tooltip({ content, children, side = 'top' }: { content: ReactNode; children: ReactNode; side?: 'top' | 'right' | 'bottom' | 'left' }) {
  return (
    <RTooltip.Root>
      <RTooltip.Trigger asChild>{children}</RTooltip.Trigger>
      <RTooltip.Portal>
        <RTooltip.Content side={side} sideOffset={6} className="z-[60] rounded-control-sm bg-primary px-2.5 py-1.5 text-xs font-medium text-primary-fg shadow-pop origin-[var(--radix-tooltip-content-transform-origin)] data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-[0.98] data-[state=delayed-open]:duration-150 data-[state=delayed-open]:ease-[cubic-bezier(0.22,1,0.36,1)]">
          {content}
        </RTooltip.Content>
      </RTooltip.Portal>
    </RTooltip.Root>
  )
}

export const popIn = 'ease-[cubic-bezier(0.22,1,0.36,1)] duration-150 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-[0.97] data-[state=open]:duration-[250ms] data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-[0.99] data-[state=closed]:duration-150'

export function Popover({ trigger, children, align = 'start', className, open, onOpenChange, side }: { trigger: ReactNode; children: ReactNode; align?: 'start' | 'center' | 'end'; className?: string; open?: boolean; onOpenChange?: (o: boolean) => void; side?: 'top' | 'bottom' | 'right' | 'left' }) {
  return (
    <RPopover.Root open={open} onOpenChange={onOpenChange}>
      <RPopover.Trigger asChild>{trigger}</RPopover.Trigger>
      <RPopover.Portal>
        <RPopover.Content align={align} side={side} sideOffset={8} className={cn('z-50 rounded-card bg-surface p-1.5 shadow-pop outline-none origin-[var(--radix-popover-content-transform-origin)]', popIn, className)}>
          {children}
        </RPopover.Content>
      </RPopover.Portal>
    </RPopover.Root>
  )
}

export function Menu({ modal = false, ...props }: RDropdown.DropdownMenuProps) {
  return <RDropdown.Root modal={modal} {...props} />
}
export const MenuTrigger = RDropdown.Trigger

export function MenuContent({ children, align = 'start', className, side, onCloseAutoFocus }: { children: ReactNode; align?: 'start' | 'end'; className?: string; side?: 'top' | 'bottom' | 'right'; onCloseAutoFocus?: (e: Event) => void }) {
  return (
    <RDropdown.Portal>
      <RDropdown.Content align={align} side={side} onCloseAutoFocus={onCloseAutoFocus} sideOffset={8} className={cn('z-50 min-w-56 rounded-card bg-surface p-1.5 shadow-pop outline-none origin-[var(--radix-dropdown-menu-content-transform-origin)]', popIn, className)}>
        {children}
      </RDropdown.Content>
    </RDropdown.Portal>
  )
}

export function MenuItem({ children, icon, onSelect, danger, right, disabled }: { children: ReactNode; icon?: ReactNode; onSelect?: () => void; danger?: boolean; right?: ReactNode; disabled?: boolean }) {
  return (
    <RDropdown.Item
      disabled={disabled}
      onSelect={onSelect}
      className={cn('flex h-9 cursor-pointer items-center gap-2.5 rounded-control-sm px-2.5 text-base text-fg2 outline-none transition-colors data-[disabled]:opacity-45 data-[highlighted]:bg-hover data-[highlighted]:text-fg', danger && 'text-danger-fg data-[highlighted]:text-danger-fg')}
    >
      {icon && <span className="flex text-muted">{icon}</span>}
      <span className="flex-1 truncate">{children}</span>
      {right}
    </RDropdown.Item>
  )
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <RDropdown.Label className="px-2.5 pb-1 pt-2 text-xs text-muted">{children}</RDropdown.Label>
}

export function MenuSeparator() {
  return <RDropdown.Separator className="-mx-1.5 my-1.5 h-px bg-line" />
}

const overlayFade = { duration: dur.quick, ease: easeOut }

export function Dialog({ open, onOpenChange, title, description, children, size = 'md', footer, bare, static: still }: { open: boolean; onOpenChange: (o: boolean) => void; title?: ReactNode; description?: ReactNode; children: ReactNode; size?: 'sm' | 'md' | 'lg' | 'xl'; footer?: ReactNode; bare?: boolean; static?: boolean }) {
  const t = useT()
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open && (
          <RDialog.Portal forceMount>
            <RDialog.Overlay asChild forceMount>
              <motion.div
                className="fixed inset-0 z-40 bg-[rgb(20_20_20/0.28)] backdrop-blur-[2px]"
                initial={still ? false : { opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={still ? undefined : { opacity: 0 }}
                transition={overlayFade}
              />
            </RDialog.Overlay>
            <div className="pointer-events-none fixed inset-0 z-50 grid place-items-center">
            <RDialog.Content asChild forceMount {...(description && !bare ? {} : { 'aria-describedby': undefined })}>
              <motion.div
                initial={still ? false : { opacity: 0, scale: 0.96 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={still ? undefined : { opacity: 0, scale: 0.96, transition: tweenClose }}
                transition={tween}
                className={cn(
                  'pointer-events-auto max-h-[88dvh] w-[calc(100vw-32px)] overflow-y-auto rounded-overlay bg-surface shadow-float outline-none',
                  size === 'sm' && 'max-w-md',
                  size === 'md' && 'max-w-xl',
                  size === 'lg' && 'max-w-3xl',
                  size === 'xl' && 'max-w-5xl',
                )}
              >
                {bare ? (
                  <>
                    <RDialog.Title className="sr-only">{title ?? t('editor.dialog')}</RDialog.Title>
                    {children}
                  </>
                ) : (
                  <>
                    <div className="flex items-start justify-between gap-4 px-6 pt-6">
                      <div>
                        {title ? <RDialog.Title className="text-lg font-medium tracking-tightish text-fg">{title}</RDialog.Title> : <RDialog.Title className="sr-only">{t('editor.dialog')}</RDialog.Title>}
                        {description && <RDialog.Description className="mt-1 text-base text-muted">{description}</RDialog.Description>}
                      </div>
                      <RDialog.Close className="-mr-2 -mt-1 rounded-control-sm p-1.5 text-muted transition-colors hover:bg-hover hover:text-fg" aria-label={t('editor.close')}>
                        <Icon name="close" size={18} />
                      </RDialog.Close>
                    </div>
                    <div className="px-6 py-5">{children}</div>
                    {footer && <div className="flex justify-end gap-2 border-t px-6 py-4">{footer}</div>}
                  </>
                )}
              </motion.div>
            </RDialog.Content>
            </div>
          </RDialog.Portal>
        )}
      </AnimatePresence>
    </RDialog.Root>
  )
}
