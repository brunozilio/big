import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render, screen } from '@testing-library/react'
import { setApi } from '@/api'
import type { Api, Item, ItemUsage } from '@/api/contract'
import { refreshKey, UsageCard } from '@/screens/items/UsageCard'

const item = (live: boolean, updatedAt = '2026-11-24T12:00:00.000Z') => ({ id: 'i1', live, updatedAt }) as unknown as Item

const usage = (over: Partial<ItemUsage> = {}): ItemUsage => ({
  spent: { costUsd: 1.75, input: 120_000, output: 13_000, cacheRead: 0, cacheWrite: 0, calls: 12 },
  context: { used: 50_000, window: 200_000, percent: 25 },
  ...over,
})

function fake(answer: ItemUsage) {
  setApi({ itemUsage: async () => answer } as unknown as Api)
}

afterEach(() => {
  cleanup()
  setApi(null)
})

describe('quadro de uso do item', () => {
  test('mostra o contexto ocupado e o custo do item', async () => {
    fake(usage())
    render(<UsageCard it={item(false)} />)
    expect(await screen.findByText('Contexto')).toBeTruthy()
    expect(screen.getByText('50 mil de 200 mil')).toBeTruthy()
    expect(screen.getByText('US$ 1,75')).toBeTruthy()
    expect(screen.queryByRole('button')).toBeNull()
  })

  test('sem conversa ainda, avisa em vez de mostrar a barra', async () => {
    fake(usage({ context: null }))
    render(<UsageCard it={item(false)} />)
    expect(await screen.findByText('Nenhuma conversa ainda.')).toBeTruthy()
  })

  test('rodando, o quadro se atualiza no máximo a cada quinze segundos', () => {
    expect(refreshKey(item(true, '2026-11-24T12:00:01.000Z'))).toBe(refreshKey(item(true, '2026-11-24T12:00:14.000Z')))
    expect(refreshKey(item(true, '2026-11-24T12:00:01.000Z'))).not.toBe(refreshKey(item(true, '2026-11-24T12:00:16.000Z')))
    expect(refreshKey(item(false, '2026-11-24T12:00:01.000Z'))).toBe('2026-11-24T12:00:01.000Z')
  })
})
