import { plural, useT } from '@/i18n'
import { cn } from './cn'
import { Icon } from './icon'

const shell = 'inline-flex items-center rounded-full bg-surface shadow-[0_0_0_1px_rgb(var(--border)),0_1px_2px_rgb(0_0_0/0.04)]'

export function DiffStat({ files, additions, deletions, href, className }: { files: number; additions: number; deletions: number; href?: string; className?: string }) {
  const t = useT()
  const body = (
    <>
      <Icon name="pr" size={15} className="text-accent" strokeWidth={2} />
      <span className="text-fg">
        {t(`editor.files_${plural(files)}`, { n: files })}
      </span>
      <span className="font-mono text-xs tabular-nums text-success-fg">+{additions}</span>
      <span className="font-mono text-xs tabular-nums text-danger-fg">−{deletions}</span>
    </>
  )
  const cls = cn(shell, 'h-8 gap-2 px-3 text-sm', href && 'transition-shadow hover:shadow-[0_0_0_1px_rgb(var(--border-strong)),0_1px_2px_rgb(0_0_0/0.06)]', className)
  return href ? (
    <a href={href} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className={cls}>
      {body}
    </a>
  ) : (
    <span className={cls}>{body}</span>
  )
}
