import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { cn } from './cn'

export const proseClass = cn(
  'prose-chat md-prose text-body leading-[28px]',
  '[&_h1]:mb-3 [&_h1]:mt-8 [&_h1]:text-2xl [&_h1]:font-medium [&_h1]:tracking-tighter2 [&_h1]:text-fg first:[&_h1]:mt-0',
  '[&_h2]:mb-2 [&_h2]:mt-8 [&_h2]:text-lg [&_h2]:font-medium [&_h2]:text-fg first:[&_h2]:mt-0',
  '[&_h3]:mb-2 [&_h3]:mt-6 [&_h3]:text-body [&_h3]:font-semibold [&_h3]:text-fg first:[&_h3]:mt-0',
  '[&_blockquote]:my-4 [&_blockquote]:border-l-2 [&_blockquote]:border-line-strong [&_blockquote]:pl-4 [&_blockquote]:text-muted',
  '[&_hr]:my-8 [&_table]:my-4 [&_table]:w-full [&_td]:border-b [&_td]:py-2 [&_td]:pr-4 [&_th]:border-b [&_th]:py-2 [&_th]:pr-4 [&_th]:text-left [&_th]:font-medium',
  '[&_pre]:my-4 [&_pre]:overflow-x-auto [&_pre]:rounded-card [&_pre]:bg-inset [&_pre]:p-4 [&_pre_code]:bg-transparent [&_pre_code]:p-0',
  '[&_img]:my-3 [&_img]:max-w-full [&_img]:rounded-card [&_s]:text-muted',
  '[&_li:has(input)]:list-none [&_li_input]:mr-2 [&_li_input]:translate-y-[1px] [&_li_input]:accent-[rgb(var(--info))]',
)

export function MarkdownPreview({ text, className, empty = 'Nada para visualizar ainda.' }: { text: string; className?: string; empty?: string }) {
  return <div className={cn(proseClass, className)}>{text.trim() ? <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown> : <p className="text-muted">{empty}</p>}</div>
}
