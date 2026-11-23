import { describe, expect, test } from 'bun:test'
import type { Block } from '../src/api/contract'
import { dockedChat } from '../src/screens/items/ItemModal'

const chat: Block = { type: 'chat', session: 's-1', composer: true }
const reason: Block = { type: 'markdown', text: '**Plano falhou.** motivo', muted: false }
const buttons: Block = { type: 'actions', buttons: [{ action: 'retry', label: 'Tentar de novo' }] }

describe('decisão acima do composer', () => {
  test('o chat da etapa leva o motivo e os botões para cima do composer', () => {
    expect(dockedChat([chat, reason, buttons])).toEqual({ chat, rest: [reason, buttons] })
  })

  test('sem decisão, ou com blocos que não cabem na barra, o painel fica como era', () => {
    expect(dockedChat([chat])).toBeNull()
    expect(dockedChat([reason, buttons])).toBeNull()
    expect(dockedChat([{ ...chat, title: 'Conversa' }, buttons])).toBeNull()
    expect(dockedChat([chat, { type: 'logs', commands: [] }])).toBeNull()
  })
})
