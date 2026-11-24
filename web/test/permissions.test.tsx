import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { type Grant, PermissionsView, grantOf, ruleOf } from '../src/screens/settings/Permissions'
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
  test('as regras se escrevem como no Claude Code', () => {
    expect(grantOf('Bash(cargo test:*)')).toEqual({ kind: 'command', value: 'cargo test' })
    expect(grantOf('Bash(git status)')).toEqual({ kind: 'command', value: 'git status' })
    expect(grantOf('Bash')).toEqual({ kind: 'command', value: '*' })
    expect(grantOf('WebFetch(domain:gov.br)')).toEqual({ kind: 'site', value: 'gov.br' })
    expect(grantOf('Site(docs.rs)')).toEqual({ kind: 'site', value: 'docs.rs' })
    expect(grantOf('mcp__atlassian__*')).toEqual({ kind: 'tool', value: 'mcp__atlassian__*' })
    expect(grantOf('   ')).toBeNull()
    for (const grant of [{ kind: 'command', value: 'cargo test' }, { kind: 'site', value: 'gov.br' }, { kind: 'tool', value: 'WebSearch' }] as Grant[]) {
      expect(grantOf(ruleOf(grant))).toEqual(grant)
    }
  })

  test('uma lista só: adiciona pelo campo do topo e remove pela linha', async () => {
    const puts = serve([{ kind: 'command', value: 'cargo test' }, { kind: 'site', value: 'gov.br' }])
    const view = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    const list = await view.findByRole('list', { name: 'Permissões' })
    await waitFor(() => expect(list.textContent).toContain('Bash(cargo test:*)'))
    expect(list.textContent).toContain('Site(gov.br)')
    const box = view.getByLabelText('Adicionar regra…')
    fireEvent.change(box, { target: { value: 'Bash(git status)' } })
    fireEvent.submit(box)
    await waitFor(() => expect(list.textContent).toContain('Bash(git status:*)'))
    fireEvent.click(view.getByLabelText(/Bash\(cargo test:\*\)/))
    await waitFor(() => expect(list.textContent).not.toContain('cargo test'))
    expect(puts[puts.length - 1]).toEqual([{ kind: 'site', value: 'gov.br' }, { kind: 'command', value: 'git status' }])
  })
})
