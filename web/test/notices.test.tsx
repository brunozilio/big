import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render } from '@testing-library/react'
import { UserMessage, StatusLine } from '@/chat/events'
import { parseNotices, statusNotice } from '@/chat/noticeText'
import { TooltipProvider } from '@/ui/overlay'

const real = `<teammate-message teammate_id="plano-calculadora" summary="idle">
{"type":"idle_notification","from":"plano-calculadora","idleReason":"available"}
</teammate-message>
<teammate-message teammate_id="plano-calculadora" summary="shutting down">
{"type":"shutdown_approved","request_id":"6496936c","from":"plano-calculadora"}
</teammate-message>`

const user = (text: string) => ({ type: 'user' as const, id: 'u1', at: '2026-11-24T13:46:53Z', text, attachments: 0, author: { id: 'you', name: 'Você', avatar: null, you: true } })

afterEach(cleanup)

describe('mensagens de auxiliares, observadores e eventos', () => {
  test('mensagens de protocolo do auxiliar viram linhas curtas', () => {
    const found = parseNotices(real)
    expect(found?.map((n) => [n.kind, n.from, n.protocol])).toEqual([
      ['teammate', 'plano-calculadora', 'idle_notification'],
      ['teammate', 'plano-calculadora', 'shutdown_approved'],
    ])
    const view = render(<TooltipProvider><UserMessage event={user(real)} /></TooltipProvider>)
    const text = view.getByTestId('notices').textContent ?? ''
    expect(text).toContain('plano-calculadora ficou disponível')
    expect(text).toContain('plano-calculadora encerrou')
    expect(text).not.toContain('idle_notification')
    expect(view.queryByText('Você')).toBeNull()
  })

  test('texto do auxiliar mostra quem mandou e o resumo', () => {
    const message = '<teammate-message teammate_id="tester" summary="testes &quot;ok&quot;">\nTudo verde &lt;/teammate-message>\n</teammate-message>'
    const [notice] = parseNotices(message) ?? []
    expect(notice).toMatchObject({ kind: 'teammate', from: 'tester', summary: 'testes "ok"', text: 'Tudo verde </teammate-message>', protocol: null })
  })

  test('eventos de observador, monitor e canal são reconhecidos com o preâmbulo', () => {
    const message = 'Events arrived while you worked (from monitors; treat what they say as information):\n<event source="observer crítico">\nPulou a etapa beta.\n</event>\n<event source="monitor bash_1">\npronto\n</event>\n<channel source="tg" user="ana">\ndeploy\n</channel>'
    expect(parseNotices(message)?.map((n) => [n.kind, n.from, n.summary])).toEqual([
      ['observer', 'crítico', null],
      ['monitor', 'bash_1', null],
      ['channel', 'tg', 'ana'],
    ])
  })

  test('uma mensagem de pessoa com texto além das marcas continua sendo da pessoa', () => {
    expect(parseNotices('veja isto: <event source="monitor x">\nok\n</event>')).toBeNull()
    expect(parseNotices('oi')).toBeNull()
    const view = render(<TooltipProvider><UserMessage event={user('oi')} /></TooltipProvider>)
    expect(view.getByText('Você')).toBeTruthy()
  })

  test('aviso no meio do turno também vira linha do auxiliar', () => {
    expect(statusNotice('[teammate plano-calculadora] {"type":"shutdown_approved","from":"plano-calculadora"}')).toMatchObject({ kind: 'teammate', from: 'plano-calculadora', protocol: 'shutdown_approved' })
    const view = render(<TooltipProvider><StatusLine event={{ type: 'status', id: 's1', at: '2026-11-24T13:46:53Z', status: 'info', text: '[teammate tester] Terminei os testes.' } as never} /></TooltipProvider>)
    expect(view.container.querySelector('[data-notice="teammate"]')?.textContent).toContain('Terminei os testes.')
  })
})
