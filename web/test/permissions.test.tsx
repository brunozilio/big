import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { type Grant, PermissionsView } from '../src/screens/settings/Permissions'
import { TooltipProvider } from '../src/ui/overlay'

const realFetch = globalThis.fetch

afterEach(() => {
  cleanup()
  globalThis.fetch = realFetch
})

function serve(start: Grant[]) {
  let grants = start
  const puts: Grant[][] = []
  globalThis.fetch = (async (_path: string, init?: RequestInit) => {
    if (init?.method === 'PUT') {
      grants = (JSON.parse(String(init.body)) as { grants: Grant[] }).grants
      puts.push(grants)
    }
    return new Response(JSON.stringify({ grants }), { status: 200 })
  }) as typeof fetch
  return puts
}

describe('aba de permissões', () => {
  test('lista comandos, sites e ferramentas, adiciona e remove', async () => {
    const puts = serve([{ kind: 'command', value: 'cargo test' }, { kind: 'site', value: 'gov.br' }])
    const view = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    await waitFor(() => expect(within(view.getByTestId('permissions-command')).getByText('cargo test')).toBeTruthy())
    expect(within(view.getByTestId('permissions-site')).getByText('gov.br')).toBeTruthy()
    expect(within(view.getByTestId('permissions-tool')).getByText('Nenhuma ferramenta permitida.')).toBeTruthy()
    fireEvent.change(view.getByLabelText('Nova permissão'), { target: { value: 'git status' } })
    fireEvent.click(view.getByText('Permitir'))
    await waitFor(() => expect(within(view.getByTestId('permissions-command')).getByText('git status')).toBeTruthy())
    fireEvent.click(view.getByLabelText('Remover cargo test'))
    await waitFor(() => expect(within(view.getByTestId('permissions-command')).queryByText('cargo test')).toBeNull())
    expect(puts[puts.length - 1]).toEqual([{ kind: 'site', value: 'gov.br' }, { kind: 'command', value: 'git status' }])
  })
})
