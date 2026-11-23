import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import { mockApi } from '../src/api/mock'
import { boot, stop } from '../src/api/store'
import { Composer } from '../src/chat/Composer'
import { fittedEffort, groupByProvider } from '../src/chat/ModelPicker'
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

  test('mostra o primeiro modelo disponível com o esforço padrão', async () => {
    await started()
    const view = composer()
    expect(view.getByLabelText('Modelo: Claude Opus').textContent).toContain('Médio')
  })

  test('sem provedor de IA pede conexão e bloqueia o envio', async () => {
    await started(['c-claude', 'c-copilot'])
    const view = composer()
    expect(view.queryByLabelText('Modelo: Claude Opus')).toBeNull()
    expect(view.getByText('Conecte um provedor de IA')).toBeTruthy()
    fireEvent.change(view.getByLabelText('Mensagem'), { target: { value: 'algo' } })
    expect((view.getByLabelText('Enviar') as HTMLButtonElement).disabled).toBe(true)
  })
})

describe('agrupamento por provedor', () => {
  const model = (id: string, provider: string) => ({ id, name: id, provider, logo: provider, efforts: [], fast: false })

  test('modelos, inclusive os autos nativos, seguem por provedor', () => {
    const groups = groupByProvider(
      [model('kiro-subscription:auto', 'kiro-subscription'), model('opus', 'claude-subscription'), model('gpt', 'openai-subscription'), model('sonnet', 'claude-subscription')],
      new Map([['kiro-subscription', 'Kiro'], ['claude-subscription', 'Claude'], ['openai-subscription', 'ChatGPT']]),
    )
    expect(groups.map((g) => [g.name, g.models.map((m) => m.id)])).toEqual([
      ['Kiro', ['kiro-subscription:auto']],
      ['Claude', ['opus', 'sonnet']],
      ['ChatGPT', ['gpt']],
    ])
  })

  test('provedor sem nome conhecido usa o id', () => {
    expect(groupByProvider([model('x', 'kiro-subscription')], new Map())[0]?.name).toBe('kiro-subscription')
  })
})
