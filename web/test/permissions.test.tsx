import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { type Grant, PermissionsView, grantOf, ruleOf, withBehavior } from '../src/screens/settings/Permissions'
import { PermissionBlock } from '../src/chat/actionBlocks'
import { TooltipProvider } from '../src/ui/overlay'

const realFetch = globalThis.fetch

afterEach(() => {
  cleanup()
  globalThis.fetch = realFetch
})

function serve(start: Grant[], suggestions: unknown[] = []) {
  let grants = start
  const puts: Grant[][] = []
  let discovery: unknown = { state: 'idle' }
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    if (String(path).endsWith('/discover')) {
      discovery = init?.method === 'POST' ? { state: 'ready', suggestions } : init?.method === 'DELETE' ? { state: 'idle' } : discovery
      return new Response(JSON.stringify(discovery), { status: 200 })
    }
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
    expect(grantOf('Site(*.gov.br)')).toEqual({ kind: 'site', value: '*.gov.br' })
    expect(grantOf('mcp__atlassian__*')).toEqual({ kind: 'tool', value: 'mcp__atlassian__*' })
    expect(grantOf('   ')).toBeNull()
    expect(grantOf('Site(https://www.gov.br/pt-br)')).toEqual({ kind: 'site', value: 'www.gov.br' })
    expect(grantOf('mcp__jira__search')).toEqual({ kind: 'tool', value: 'mcp__jira__search' })
    for (const bad of ['qualquer coisa', 'Bash()', 'Site(gov)', 'Site(não é site)', 'Foo(bar)', 'mcp__jira', 'mcp__x__y__z', 'rm -rf /', 'webfetch']) {
      expect(grantOf(bad)).toBeNull()
    }
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
    fireEvent.click(view.getByLabelText("Remover Bash(cargo test:*)"))
    await waitFor(() => expect(list.textContent).not.toContain('cargo test'))
    expect(puts[0]).toEqual([{ kind: 'command', value: 'git status', behavior: 'allow' }, { kind: 'command', value: 'cargo test' }, { kind: 'site', value: 'gov.br' }])
    expect(puts[puts.length - 1]).toEqual([{ kind: 'command', value: 'git status', behavior: 'allow' }, { kind: 'site', value: 'gov.br' }])
  })

  test('descobrir traz tudo marcado e só entra o que ficou marcado', async () => {
    const suggestions = [
      { grant: { kind: 'command', value: 'bun test' }, why: 'script test do package.json' },
      { grant: { kind: 'site', value: 'docs.rs' }, why: 'lido 9× em 4 sessões' },
    ]
    const puts = serve([{ kind: 'command', value: 'cargo test' }], suggestions)
    const view = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    fireEvent.click(await view.findByRole('button', { name: /Descobrir permissões/ }))
    const bun = await view.findByRole('checkbox', { name: 'Bash(bun test:*)' })
    const docs = view.getByRole('checkbox', { name: 'Site(docs.rs)' })
    expect(bun.getAttribute('data-state')).toBe('checked')
    expect(docs.getAttribute('data-state')).toBe('checked')
    expect(view.getByText('script test do package.json')).toBeTruthy()
    fireEvent.click(docs)
    fireEvent.click(view.getByRole('button', { name: 'Adicionar 1' }))
    await waitFor(() => expect(puts.length).toBe(1))
    expect(puts[0]).toEqual([{ kind: 'command', value: 'bun test' }, { kind: 'command', value: 'cargo test' }])
    expect(view.queryByRole('checkbox')).toBeNull()
  })

  test('ao voltar à tela, a descoberta que rodou em segundo plano aparece', async () => {
    const suggestions = [{ grant: { kind: 'command', value: 'bun test' }, why: '' }]
    serve([], suggestions)
    const first = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    fireEvent.click(await first.findByRole('button', { name: /Descobrir permissões/ }))
    await first.findByRole('checkbox', { name: 'Bash(bun test:*)' })
    first.unmount()
    const again = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    expect(await again.findByRole('checkbox', { name: 'Bash(bun test:*)' })).toBeTruthy()
  })

  test('regra inválida ou repetida não entra e diz o porquê', async () => {
    const puts = serve([{ kind: 'command', value: 'cargo test' }])
    const view = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    const box = await view.findByLabelText('Adicionar regra…')
    fireEvent.change(box, { target: { value: 'qualquer coisa' } })
    fireEvent.submit(box)
    expect((await view.findByRole('alert')).textContent).toContain('Regra inválida')
    fireEvent.change(box, { target: { value: 'Bash(cargo test:*)' } })
    expect(view.queryByRole('alert')).toBeNull()
    fireEvent.submit(box)
    expect((await view.findByRole('alert')).textContent).toContain('já está na lista')
    expect(puts).toEqual([])
  })

  test('cada regra diz se permite, sempre pergunta ou nega', async () => {
    const puts = serve([{ kind: 'command', value: 'cargo test' }, { kind: 'command', value: 'git push', behavior: 'ask' }, { kind: 'site', value: 'evil.dev', behavior: 'deny' }])
    const view = render(<TooltipProvider><PermissionsView /></TooltipProvider>)
    const push = await view.findByRole('button', { name: 'O que fazer com Bash(git push:*)' })
    expect(push.textContent).toContain('Sempre perguntar')
    expect(view.getByRole('button', { name: 'O que fazer com Bash(cargo test:*)' }).textContent).toContain('Permitir')
    expect(view.getByRole('button', { name: 'O que fazer com Site(evil.dev)' }).textContent).toContain('Negar')
    fireEvent.click(push)
    const choices = await view.findAllByRole('menuitemradio')
    expect(choices.map((c) => c.textContent)).toEqual(['Permitir1', 'Sempre perguntar2', 'Negar3'])
    expect(choices[1].getAttribute('aria-checked')).toBe('true')
    fireEvent.click(choices[2])
    await waitFor(() => expect(puts.length).toBe(1))
    expect(puts[0][1]).toEqual({ kind: 'command', value: 'git push', behavior: 'deny' })
    fireEvent.click(view.getByRole('button', { name: 'O que fazer com Site(evil.dev)' }))
    fireEvent.keyDown(await view.findByLabelText('Mudar para…'), { key: '1' })
    await waitFor(() => expect(puts.length).toBe(2))
    expect(puts[1][2]).toEqual({ kind: 'site', value: 'evil.dev', behavior: 'allow' })
    const grants: Grant[] = [{ kind: 'command', value: 'ls' }, { kind: 'site', value: 'gov.br' }]
    expect(withBehavior(grants, { kind: 'site', value: 'gov.br' }, 'deny')).toEqual([{ kind: 'command', value: 'ls' }, { kind: 'site', value: 'gov.br', behavior: 'deny' }])
  })

  test('pedido de uma regra "sempre perguntar" não oferece "Sempre permitir"', () => {
    const event = (rememberable?: boolean) => ({ id: 'p', at: '', type: 'permission', tool: 'Bash', detail: 'git push', risk: 'medium', resolution: 'pending', rememberable }) as never
    const respond = async () => {}
    const asked = render(<TooltipProvider><PermissionBlock event={event(false)} respond={respond} /></TooltipProvider>)
    expect(asked.queryByRole('button', { name: /Sempre permitir/ })).toBeNull()
    expect(asked.getByRole('button', { name: /Negar/ })).toBeTruthy()
    cleanup()
    const plain = render(<TooltipProvider><PermissionBlock event={event(undefined)} respond={respond} /></TooltipProvider>)
    expect(plain.getByRole('button', { name: /Sempre permitir/ })).toBeTruthy()
  })
})
