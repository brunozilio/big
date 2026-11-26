import { describe, expect, test } from 'bun:test'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/ui/overlay'

describe('menu', () => {
  test('aberto, não trava os cliques da página', async () => {
    render(
      <Menu>
        <MenuTrigger asChild><button>mais</button></MenuTrigger>
        <MenuContent><MenuItem>Cancelar item</MenuItem></MenuContent>
      </Menu>,
    )
    fireEvent.keyDown(screen.getByText('mais'), { key: 'Enter' })
    await act(() => new Promise((r) => setTimeout(r, 100)))
    expect(screen.getByText('Cancelar item')).toBeTruthy()
    expect(document.body.style.pointerEvents).toBe('')
  })
})
