import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { BlockContext } from '../src/blocks/context'
import { BlockView } from '../src/blocks/registry'

afterEach(cleanup)

describe('formulário', () => {
  test('envia os valores dos campos pela ação de envio da etapa', async () => {
    const sent: { action: string; payload: unknown }[] = []
    const host = { item: 'i1', step: 'release', version: 'v1', send: async (action: string, payload: unknown) => void sent.push({ action, payload }) }
    const block = {
      type: 'form' as const,
      title: 'Motivo da rejeição',
      fields: [
        { name: 'reason', label: 'Motivo', kind: 'textarea' as const },
        { name: 'ticket', label: 'Chamado', kind: 'text' as const, value: 'OPS-1' },
      ],
      submit: { action: 'submit', label: 'Rejeitar deploy' },
    }
    const view = render(
      <BlockContext.Provider value={host}>
        <BlockView block={block} />
      </BlockContext.Provider>,
    )
    expect(view.getByText('Motivo da rejeição')).toBeTruthy()
    fireEvent.change(view.getByRole('textbox', { name: 'Motivo' }), { target: { value: 'janela de congelamento' } })
    fireEvent.click(view.getByText('Rejeitar deploy'))
    await waitFor(() => expect(sent.length).toBe(1))
    expect(sent[0]).toEqual({ action: 'submit', payload: { reason: 'janela de congelamento', ticket: 'OPS-1' } })
  })
})
