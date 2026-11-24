import { afterEach, describe, expect, mock, test } from 'bun:test'
import { act, cleanup, fireEvent, render, renderHook, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Api } from '../src/api/contract'
import { isItem, isLiveEvent, isSession, isStepView } from '../src/api/guards'
import { mockApi } from '../src/api/mock'
import { applyLive, boot, getState, stop, useSession } from '../src/api/store'
import { Composer } from '../src/chat/Composer'
import { SideAnswers, useSideQuestions } from '../src/chat/SideQuestions'
import { HeadsUpNote } from '../src/screens/items/ItemModal'
import { ItemsView } from '../src/screens/items/ItemsView'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

async function started() {
  const api = mockApi({ tickMs: 0, delayMs: 0 })
  setApi(api)
  await act(() => boot(api))
  return api
}

function composer(props: Partial<Parameters<typeof Composer>[0]>) {
  const onSubmit = mock(() => undefined)
  const view = render(
    <MemoryRouter>
      <TooltipProvider>
        <Composer onSubmit={onSubmit} {...props} />
      </TooltipProvider>
    </MemoryRouter>,
  )
  return { view, onSubmit }
}

describe('resumo do item', () => {
  test('o card da lista mostra só o título, sem o resumo', async () => {
    const api = await started()
    const items = await api.items()
    const recaps = items.filter((i) => i.recap)
    expect(recaps.length).toBeGreaterThan(0)
    expect(recaps.every((i) => i.group === 'you' && isItem(i))).toBe(true)
    const view = render(<TooltipProvider><ItemsView onOpen={() => undefined} onConnect={() => undefined} fresh={null} /></TooltipProvider>)
    for (const it of recaps) expect(view.queryByText(it.recap ?? '')).toBeNull()
  })
})

describe('aviso da etapa', () => {
  test('mostra título e explicação, mais forte quando urgente', () => {
    const view = render(<HeadsUpNote headsUp={{ title: 'Migração em staging', explain: 'Altera tabelas compartilhadas.', urgent: true }} />)
    const note = view.getByRole('note')
    expect(note.textContent).toContain('Você deveria saber')
    expect(view.getByText('Migração em staging').className).toContain('font-medium')
    expect(note.getAttribute('data-urgent')).toBe('true')
  })

  test('o mock só traz avisos válidos', async () => {
    const api = await started()
    const items = await api.items()
    const views = await Promise.all(items.flatMap((it) => it.steps.map((s) => api.step(it.id, s.id, null))))
    expect(views.every(isStepView)).toBe(true)
    expect(views.some((v) => v.headsUp)).toBe(true)
  })
})

describe('sugestão de próxima mensagem', () => {
  test('clicar preenche o campo sem enviar', () => {
    const { view, onSubmit } = composer({ suggestion: 'Rode os testes de novo' })
    fireEvent.click(view.getByLabelText('Usar sugestão: Rode os testes de novo'))
    expect((view.getByLabelText('Mensagem') as HTMLTextAreaElement).value).toBe('Rode os testes de novo')
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('some com texto no campo ou com o agente rodando', () => {
    const { view } = composer({ suggestion: 'Rode os testes de novo', running: true })
    expect(view.queryByLabelText('Usar sugestão: Rode os testes de novo')).toBeNull()
    cleanup()
    const idle = composer({ suggestion: 'Rode os testes de novo' })
    fireEvent.change(idle.view.getByLabelText('Mensagem'), { target: { value: 'outra coisa' } })
    expect(idle.view.queryByLabelText('Usar sugestão: Rode os testes de novo')).toBeNull()
  })

  test('sessões do mock seguem o contrato com a sugestão', async () => {
    const api = await started()
    const items = await api.items()
    const sessions = await Promise.all(items.map((i) => api.session(`s-${i.id}`)))
    expect(sessions.every(isSession)).toBe(true)
    expect(sessions.some((s) => s.status === 'idle' && s.suggestion)).toBe(true)
  })
})

describe('pergunta à parte', () => {
  test('com o agente rodando, Enter pergunta à parte em vez de enviar', () => {
    const onAside = mock((_text: string) => undefined)
    const { view, onSubmit } = composer({ running: true, onAside })
    const box = view.getByLabelText('Mensagem') as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: 'O que falta?' } })
    expect(view.getByText('Perguntar à parte')).toBeTruthy()
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onAside).toHaveBeenCalledWith('O que falta?')
    expect(onSubmit).not.toHaveBeenCalled()
    expect(box.value).toBe('')
  })

  test('a resposta aparece numa bolha efêmera com a pergunta', async () => {
    const side = mock(async () => ({ answer: 'Está rodando os testes.' }))
    setApi({ side } as unknown as Api)
    function Harness() {
      const q = useSideQuestions('s1', 'opus')
      return (
        <>
          <button onClick={() => q.ask('O que falta?')}>perguntar</button>
          <SideAnswers asked={q.asked} onDismiss={q.dismiss} />
        </>
      )
    }
    const view = render(<Harness />)
    fireEvent.click(view.getByText('perguntar'))
    await waitFor(() => expect(view.getByText('Está rodando os testes.')).toBeTruthy())
    expect(side).toHaveBeenCalledWith('s1', { text: 'O que falta?', model: 'opus' })
    const bubble = view.getByTestId('side-question')
    expect(bubble.textContent).toContain('Pergunta à parte')
    expect(bubble.textContent).toContain('O que falta?')
    fireEvent.click(view.getByLabelText('Fechar pergunta à parte'))
    await waitFor(() => expect(view.queryByTestId('side-question')).toBeNull())
  })

  test('o mock responde e recusa pergunta vazia', async () => {
    const api = await started()
    const [first] = await api.items()
    expect((await api.side(`s-${first.id}`, { text: 'Como está?' })).answer.length).toBeGreaterThan(0)
    await expect(api.side(`s-${first.id}`, { text: '  ' })).rejects.toThrow('Escreva a pergunta.')
  })
})

describe('sugestão ao vivo', () => {
  test('a sugestão que o servidor anuncia chega à sessão aberta e some quando ela volta a rodar', async () => {
    const api = await started()
    const items = await api.items()
    const id = `s-${items[0].id}`
    const opened = renderHook(() => useSession(id))
    await waitFor(() => expect(opened.result.current.session?.id).toBe(id))
    act(() => applyLive({ type: 'session.status', session: id, status: 'idle' }))
    act(() => applyLive({ type: 'session.suggestion', session: id, suggestion: 'rode os testes' }))
    expect(getState().sessions[id]?.suggestion).toBe('rode os testes')
    act(() => applyLive({ type: 'session.status', session: id, status: 'running' }))
    expect(getState().sessions[id]?.suggestion).toBeNull()
    expect(isLiveEvent({ type: 'session.suggestion', session: id, suggestion: null })).toBe(true)
    expect(isLiveEvent({ type: 'session.suggestion', session: id, suggestion: 3 })).toBe(false)
  })
})
