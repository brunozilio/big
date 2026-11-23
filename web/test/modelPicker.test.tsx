import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import { mockApi } from '../src/api/mock'
import { boot, stop } from '../src/api/store'
import { Composer } from '../src/chat/Composer'
import { fittedEffort } from '../src/chat/ModelPicker'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
})

async function started(disconnect: string[] = []) {
  const api = mockApi({ tickMs: 0, delayMs: 0 })
  for (const id of disconnect) await api.disconnect(id)
  setApi(api)
  await act(() => boot(api))
}

function composer() {
  return render(
    <MemoryRouter>
      <TooltipProvider>
        <Composer onSubmit={() => undefined} />
      </TooltipProvider>
    </MemoryRouter>,
  )
}

describe('seletor de modelo', () => {
  test('o esforço cai no nível aceito mais próximo abaixo', () => {
    expect(fittedEffort(['low', 'medium', 'high', 'xhigh'], 'max')).toBe('xhigh')
    expect(fittedEffort(['high'], 'low')).toBe('high')
    expect(fittedEffort([], 'max')).toBe('max')
  })

  test('mostra Auto com o esforço padrão', async () => {
    await started()
    const view = composer()
    expect(view.getByLabelText('Modelo: Auto').textContent).toContain('Médio')
  })

  test('sem provedor de IA pede conexão e bloqueia o envio', async () => {
    await started(['c-claude', 'c-copilot'])
    const view = composer()
    expect(view.queryByLabelText('Modelo: Auto')).toBeNull()
    expect(view.getByText('Conecte um provedor de IA')).toBeTruthy()
    fireEvent.change(view.getByLabelText('Mensagem'), { target: { value: 'algo' } })
    expect((view.getByLabelText('Enviar') as HTMLButtonElement).disabled).toBe(true)
  })
})
