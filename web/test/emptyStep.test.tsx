import { describe, expect, test } from 'bun:test'
import { render, screen } from '@testing-library/react'
import type { Item } from '@/api/contract'
import { EmptyStep } from '@/screens/items/ItemModal'

const item = (impediment: Item['impediment'], state = 'pending') =>
  ({ id: 'i1', current: 'pr', doneAt: null, impediment, steps: [{ id: 'pr', label: 'Pull request', state }] }) as unknown as Item

describe('etapa sem conteúdo', () => {
  test('uma etapa parada mostra o motivo e como sair dela', () => {
    render(<EmptyStep it={item({ reason: 'O workflow mudou', detail: 'o código do workflow mudou e não reproduz o histórico', connect: null })} step="pr" />)
    expect(screen.getByText('O workflow mudou')).toBeTruthy()
    expect(screen.getByText('o código do workflow mudou e não reproduz o histórico')).toBeTruthy()
    expect(screen.getByText('Rodar de novo a partir de Pull request')).toBeTruthy()
    expect(screen.getByText('Cancelar item')).toBeTruthy()
  })

  test('uma etapa rodando sem conteúdo diz que está em andamento', () => {
    render(<EmptyStep it={item(null, 'running')} step="pr" />)
    expect(screen.getByText('Em andamento…')).toBeTruthy()
  })

  test('sem impedimento avisa que ainda não há nada', () => {
    render(<EmptyStep it={item(null)} step="pr" />)
    expect(screen.getByText('Nada para mostrar nesta etapa ainda.')).toBeTruthy()
  })
})
