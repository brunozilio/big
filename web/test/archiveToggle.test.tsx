import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import { setApi } from '../src/api'
import type { Api, Item } from '../src/api/contract'
import { getState, setItems } from '../src/api/store'
import { archiveToggle } from '../src/screens/items/parts'
import { clearToasts, Toaster } from '../src/ui/toast'

afterEach(() => {
  cleanup()
  clearToasts()
  setItems(() => [])
  setApi(null)
})

const item = (archivedAt: string | null) => ({ id: 'i1', group: 'delivered', archivedAt }) as Item
const kept = () => getState().items.find((x) => x.id === 'i1')?.archivedAt ?? null

function gate() {
  let open: (fail?: Error) => void = () => undefined
  const wait = new Promise<void>((resolve, reject) => {
    open = (fail) => (fail ? reject(fail) : resolve())
  })
  return { wait, open }
}

describe('arquivar otimista', () => {
  test('arquiva na hora e mantém quando o servidor confirma', async () => {
    const call = gate()
    setApi({ archive: () => call.wait } as unknown as Api)
    setItems(() => [item(null)])
    const done = archiveToggle(item(null))
    expect(kept()).not.toBeNull()
    call.open()
    expect(await done).toBe(true)
    expect(kept()).not.toBeNull()
  })

  test('a falha desfaz e avisa em um toast', async () => {
    const call = gate()
    setApi({ archive: () => call.wait } as unknown as Api)
    setItems(() => [item(null)])
    render(<Toaster closeLabel="Fechar" />)
    const done = archiveToggle(item(null))
    expect(kept()).not.toBeNull()
    await act(async () => {
      call.open(new Error('sem rede'))
      expect(await done).toBe(false)
    })
    expect(kept()).toBeNull()
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('sem rede'))
  })

  test('desarquivar também volta atrás na falha', async () => {
    const at = '2026-01-01T00:00:00.000Z'
    setApi({ unarchive: () => Promise.reject(new Error('negado')) } as unknown as Api)
    setItems(() => [item(at)])
    const done = archiveToggle(item(at))
    expect(kept()).toBeNull()
    expect(await done).toBe(false)
    expect(kept()).toBe(at)
  })

  test('não desfaz por cima de um estado mais novo do servidor', async () => {
    const call = gate()
    setApi({ archive: () => call.wait } as unknown as Api)
    setItems(() => [item(null)])
    const done = archiveToggle(item(null))
    setItems((all) => all.map((x) => ({ ...x, archivedAt: '2030-01-01T00:00:00.000Z' })))
    call.open(new Error('sem rede'))
    await done
    expect(kept()).toBe('2030-01-01T00:00:00.000Z')
  })
})
