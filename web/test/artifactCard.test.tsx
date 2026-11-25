import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render } from '@testing-library/react'
import type { SessionEvent } from '@/api/contract'
import { ArtifactCard } from '@/chat/ArtifactCard'
import { segmentsOf } from '@/chat/ChatView'
import { TooltipProvider } from '@/ui/overlay'

type ArtifactEvent = Extract<SessionEvent, { type: 'artifact' }>

const card = (extra: Partial<ArtifactEvent> = {}): ArtifactEvent => ({
  id: 'e4', at: '2026-11-24T13:46:53Z', type: 'artifact', artifactId: '0a1b', title: 'Vendas Q3', name: 'q3.html', kind: 'page', ...extra,
})

afterEach(cleanup)

describe('artefato publicado no chat', () => {
  test('fica fora do grupo de ferramentas, como resposta para a pessoa', () => {
    const tools = { id: 'k1', at: '', type: 'tools', calls: [], seconds: 0, live: false } as unknown as SessionEvent
    const order = segmentsOf([tools, card()], false).map((s) => s.kind)
    expect(order).toEqual(['work', 'human'])
  })

  test('mostra título e tipo, baixa pelo endereço do arquivo e abre a página isolada', () => {
    const view = render(<TooltipProvider><ArtifactCard event={card()} /></TooltipProvider>)
    expect(view.getByText('Vendas Q3')).toBeTruthy()
    expect(view.getByText('Página · q3.html')).toBeTruthy()
    expect(view.getByLabelText('Baixar').getAttribute('href')).toBe('/api/artifacts/0a1b/content?download=1')
    fireEvent.click(view.getByText('Abrir'))
    const frame = document.querySelector('iframe')
    expect(frame?.getAttribute('src')).toBe('/api/artifacts/0a1b/content')
    expect(frame?.getAttribute('sandbox')).not.toContain('allow-same-origin')
  })
})
