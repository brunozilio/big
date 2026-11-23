import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render, screen } from '@testing-library/react'
import { AttemptMark } from '@/screens/items/ItemModal'

afterEach(cleanup)

describe('marca de uma execução', () => {
  test('uma execução pulada mostra o ícone de pulado, sem spinner nem falha', () => {
    const { container } = render(<AttemptMark state="skipped" />)
    expect(screen.getByLabelText('Pulado')).toBeTruthy()
    expect(container.querySelector('[role="status"], .animate-spin')).toBeNull()
    expect(container.innerHTML).not.toContain('text-danger')
  })

  test('só uma execução rodando gira', () => {
    const { container } = render(<AttemptMark state="running" />)
    expect(screen.getByLabelText('Rodando')).toBeTruthy()
    expect(container.innerHTML).not.toContain('m6 6l6 6')
  })
})
