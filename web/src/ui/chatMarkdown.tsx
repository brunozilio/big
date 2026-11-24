import { isValidElement, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { CodeBlock } from './display'

function textOf(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')
  if (isValidElement<{ children?: ReactNode }>(node)) return textOf(node.props.children)
  return ''
}

export function fencedCode(children: ReactNode): { code: string; language?: string } {
  const inner = Array.isArray(children) ? children[0] : children
  const className = isValidElement<{ className?: string }>(inner) ? inner.props.className ?? '' : ''
  const language = /language-([\w+#-]+)/.exec(className)?.[1]?.toLowerCase()
  const code = textOf(children).replace(/\n$/, '')
  const looksJson = !language && /^\s*[[{]/.test(code) && /[\]}]\s*$/.test(code)
  return { code, language: language ?? (looksJson ? 'json' : undefined) }
}

const components: Components = {
  pre: ({ children }) => {
    const { code, language } = fencedCode(children)
    return <CodeBlock code={code} language={language} className="not-prose my-3 border" />
  },
  table: ({ children }) => (
    <div className="my-3 overflow-x-auto">
      <table>{children}</table>
    </div>
  ),
}

export function ChatMarkdown({ text }: { text: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
      {text}
    </ReactMarkdown>
  )
}
