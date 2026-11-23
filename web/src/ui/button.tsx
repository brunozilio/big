import { Slot } from '@radix-ui/react-slot'
import { cva, type VariantProps } from 'class-variance-authority'
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { cn } from './cn'
import { Kbd, Spinner } from './display'
import { Tooltip } from './overlay'

const button = cva(
  'relative inline-flex select-none items-center justify-center gap-1.5 whitespace-nowrap font-medium transition-[background,box-shadow,transform,color] duration-150 ease-out active:scale-[0.97] disabled:pointer-events-none disabled:opacity-45',
  {
    variants: {
      variant: {
        primary: 'bg-primary text-primary-fg shadow-[0_1px_2px_rgb(0_0_0/0.15),inset_0_1px_0_rgb(255_255_255/0.08)] hover:bg-primary/88',
        secondary: 'bg-surface text-fg shadow-card hover:bg-hover',
        ghost: 'text-fg2 hover:bg-hover hover:text-fg',
        danger: 'bg-danger-solid text-white shadow-[0_1px_2px_rgb(0_0_0/0.15)] hover:bg-danger-solid/90',
        link: 'px-0 text-info-fg hover:underline active:scale-100',
        soft: 'bg-inset text-fg hover:bg-active',
      },
      size: { sm: 'h-8 rounded-control-sm px-2.5 text-sm', md: 'h-9 rounded-control px-3.5 text-base', lg: 'h-11 rounded-card px-5 text-md' },
    },
    defaultVariants: { variant: 'secondary', size: 'md' },
  },
)

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof button> {
  asChild?: boolean
  loading?: boolean
  icon?: ReactNode
  kbd?: string
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ className, variant, size, asChild, loading, icon, kbd, children, ...props }, ref) {
  if (asChild) {
    return (
      <Slot ref={ref} className={cn(button({ variant, size }), className)} {...props}>
        {children}
      </Slot>
    )
  }
  return (
    <button ref={ref} className={cn(button({ variant, size }), className)} disabled={loading || props.disabled} {...props}>
      {loading ? <Spinner size={14} className="text-current" /> : icon}
      {children}
      {kbd && <Kbd className={variant === 'primary' ? 'ml-1 bg-white/15 text-primary-fg shadow-none' : 'ml-1'}>{kbd}</Kbd>}
    </button>
  )
})

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tooltip: string
  variant?: 'ghost' | 'outline'
  size?: 'sm' | 'md'
  active?: boolean
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton({ tooltip, variant = 'ghost', size = 'md', active, className, children, ...props }, ref) {
  return (
    <Tooltip content={tooltip}>
      <button
        ref={ref}
        aria-label={tooltip}
        className={cn(
          'relative inline-flex items-center justify-center rounded-control-sm text-muted transition-[background,color,transform] duration-150 hover:bg-hover hover:text-fg active:scale-[0.94] disabled:opacity-45',
          size === 'sm' ? 'hit-44 size-7' : 'size-9',
          variant === 'outline' && 'bg-surface shadow-card',
          active && 'bg-active text-fg',
          className,
        )}
        {...props}
      >
        {children}
      </button>
    </Tooltip>
  )
})
