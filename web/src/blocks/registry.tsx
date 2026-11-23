import type { ComponentType } from 'react'
import type { Block, BlockType } from '@/api/contract'
import { ChatView } from '@/chat/ChatView'
import { PlanSplit } from '@/chat/PlanSplit'
import { Icon } from '@/ui/icon'
import { CasesBlock } from './cases'
import { ChangeBlock, MergeBlock } from './change'
import { IssueBlock } from './issue'
import { PipelineBlock } from './pipeline'
import { ReviewBlock } from './review'
import { ActionsBlock, FormBlock, LogsBlock, MarkdownBlock } from './simple'

type Of<T extends BlockType> = Extract<Block, { type: T }>

function ChatBlock({ block }: { block: Of<'chat'> }) {
  if (!block.title) return <PlanSplit sessionId={block.session} composer={block.composer} />
  return (
    <div className="overflow-hidden rounded-card bg-bg shadow-card">
      <div className="flex h-11 items-center gap-2.5 border-b px-4 text-base font-medium text-fg"><Icon name="chat" size={16} className="text-muted" />{block.title}</div>
      <div className="h-[440px]"><ChatView sessionId={block.session} composer={block.composer} /></div>
    </div>
  )
}

export const registry: { [K in BlockType]: ComponentType<{ block: Of<K> }> } = {
  chat: ChatBlock,
  issue: IssueBlock,
  change: ChangeBlock,
  review: ReviewBlock,
  pipeline: PipelineBlock,
  merge: MergeBlock,
  markdown: MarkdownBlock,
  cases: CasesBlock,
  logs: LogsBlock,
  form: FormBlock,
  actions: ActionsBlock,
}

export function BlockView({ block }: { block: Block }) {
  const View = registry[block.type] as ComponentType<{ block: Block }> | undefined
  if (!View) return <p className="text-base text-muted">Bloco desconhecido: {block.type}</p>
  return <View block={block} />
}

export const fillsPanel = (blocks: Block[]) => blocks.length === 1 && blocks[0].type === 'chat' && !blocks[0].title
