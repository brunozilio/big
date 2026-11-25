import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render, waitFor } from '@testing-library/react'
import { setApi } from '../src/api'
import { mockApi } from '../src/api/mock'
import { BlockView } from '../src/blocks/registry'

afterEach(() => {
  cleanup()
  setApi(null)
})

describe('merge', () => {
  test('mostra o pull request que vai entrar e o botão em português', async () => {
    const change = { key: 'o/r#3', number: 3, title: 'Loja virtual', url: null, state: 'open', author: { id: 'dev', name: 'Dev', avatar: null }, head: 'big/x', base: 'main', body: '', files: [{ path: 'a.ts', additions: 4, deletions: 1 }], comments: [] }
    setApi({ ...mockApi({ tickMs: 0, delayMs: 0 }), call: async () => change } as never)
    const block = { type: 'merge' as const, connection: 'c1', change: 'o/r#3', ready: true, methods: ['squash' as const, 'merge' as const], action: 'merge', checks: [], result: null }
    const view = render(<BlockView block={block} />)
    expect(view.getByText('Fazer merge com squash')).toBeTruthy()
    await waitFor(() => expect(view.getByText('Loja virtual')).toBeTruthy())
    expect(view.queryByText('quer fazer merge de')).toBeNull()
    expect(view.queryByText('Dev')).toBeNull()
    expect(view.queryByText('+4')).toBeNull()
    expect(view.queryByText(/and merge/)).toBeNull()
  })
})
