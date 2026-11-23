import { describe, expect, test } from 'bun:test'
import { render, screen } from '@testing-library/react'
import type { Item } from '@/api/contract'
import { EmptyStep } from '@/screens/items/ItemModal'

const item = (impediment: Item['impediment']) =>
  ({ id: 'i1', current: 'pr', doneAt: null, impediment, steps: [{ id: 'pr', label: 'Pull request' }] }) as unknown as Item

describe('etapa sem conteúdo', () => {
  test('uma etapa parada mostra o motivo e como sair dela', () => {
    render(<EmptyStep it={item({ reason: 'o código do workflow mudou', connect: null })} step="pr" />)
    expect(screen.getByText('o código do workflow mudou')).toBeTruthy()
    expect(screen.getByText('Rodar de novo a partir de Pull request')).toBeTruthy()
    expect(screen.getByText('Cancelar item')).toBeTruthy()
  })

  test('sem impedimento avisa que ainda não há nada', () => {
    render(<EmptyStep it={item(null)} step="pr" />)
    expect(screen.getByText('Nada para mostrar nesta etapa ainda.')).toBeTruthy()
  })
})
