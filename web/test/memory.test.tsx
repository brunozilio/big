import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryView } from '../src/screens/settings/Memory'
import { type Memory, type MemoryTree, forgottenOf, matches, placesOf } from '../src/screens/settings/memoryTree'
import { TooltipProvider } from '../src/ui/overlay'

const realFetch = globalThis.fetch

afterEach(() => {
  cleanup()
  globalThis.fetch = realFetch
  document.body.innerHTML = ''
})

const memory = (id: number, text: string): Memory => ({ id, kind: 'semantic', text, tags: [] })
const none = { memories: [], forgotten: [] }

const tree = (): MemoryTree => ({
  global: { memories: [memory(1, 'responde em português')], forgotten: [] },
  applications: [
    {
      id: 'a',
      name: 'Foguete',
      memories: [memory(1, 'deploy só às terças')],
      forgotten: [],
      repositories: [
        { directory: '/work/rocket/api', memories: [memory(1, 'o build usa bun')], forgotten: [{ memory: memory(2, 'usa yarn'), at: 5 }], team: [memory(1_000_000, 'revisão em dupla')] },
        { directory: '/work/rocket/web', ...none, team: [] },
      ],
    },
  ],
})

function serve(start: MemoryTree) {
  let current = start
  const posts: { path: string; body: unknown }[] = []
  globalThis.fetch = (async (path: string, init?: RequestInit) => {
    if (init?.method === 'POST') {
      const body = JSON.parse(String(init.body)) as unknown
      posts.push({ path: String(path), body })
      current = { ...current, global: { memories: [...current.global.memories, memory(2, 'o build usa bun')], forgotten: [] } }
    }
    return new Response(JSON.stringify(current), { status: 200 })
  }) as typeof fetch
  return posts
}

describe('aba de memória', () => {
  test('os lugares vão do global à aplicação e aos repositórios dela', () => {
    expect(placesOf(tree(), 'Global').map((p) => p.label)).toEqual(['Global', 'Foguete', 'Foguete · api', 'Foguete · web'])
    expect(forgottenOf(tree(), 'Global').map((f) => [f.label, f.forgotten.memory.text])).toEqual([['Foguete · api', 'usa yarn']])
    expect(matches('PORTUGUES')(memory(1, 'responde em português'))).toBe(true)
    expect(matches('pnpm')(memory(1, 'responde em português'))).toBe(false)
  })

  test('mostra tudo numa página, sem escolher projeto, e sem dizer quem aprendeu nem quantas vezes usou', async () => {
    serve(tree())
    const view = render(<TooltipProvider><MemoryView /></TooltipProvider>)
    const global = await view.findByRole('list', { name: 'Global' })
    expect(global.textContent).toContain('responde em português')
    expect(view.getByRole('list', { name: 'Foguete' }).textContent).toContain('deploy só às terças')
    const api = view.getByRole('list', { name: 'api' })
    expect(api.textContent).toContain('o build usa bun')
    expect(api.textContent).toContain('revisão em dupla')
    expect(view.container.textContent).not.toMatch(/Aprendida|usada|Escrita por/)
    expect(view.queryByRole('combobox')).toBeNull()
  })

  test('a busca deixa só o que bate e esquecer manda o lugar da memória', async () => {
    const posts = serve(tree())
    const view = render(<TooltipProvider><MemoryView /></TooltipProvider>)
    await view.findByRole('list', { name: 'Global' })
    fireEvent.change(view.getByLabelText('Buscar na memória…'), { target: { value: 'bun' } })
    expect(view.queryByRole('list', { name: 'Global' })).toBeNull()
    const api = view.getByRole('list', { name: 'api' })
    expect(api.textContent).not.toContain('revisão em dupla')
    fireEvent.click(within(api).getByLabelText('Esquecer: o build usa bun'))
    await waitFor(() => expect(posts).toHaveLength(1))
    expect(posts[0]).toEqual({ path: '/api/memories', body: { shelf: { scope: 'repository', directory: '/work/rocket/api' }, edit: { op: 'forget', id: 1 } } })
  })

  test('mover leva a memória para outro lugar', async () => {
    const posts = serve(tree())
    const view = render(<TooltipProvider><MemoryView /></TooltipProvider>)
    const api = await view.findByRole('list', { name: 'api' })
    fireEvent.keyDown(within(api).getByLabelText('Mover: o build usa bun'), { key: 'Enter' })
    fireEvent.click(await view.findByRole('menuitem', { name: 'Foguete' }))
    await waitFor(() => expect(posts).toHaveLength(1))
    expect(posts[0]).toEqual({ path: '/api/memories/move', body: { from: { scope: 'repository', directory: '/work/rocket/api' }, id: 1, to: { scope: 'application', id: 'a' } } })
    await waitFor(() => expect(view.getByRole('list', { name: 'Global' }).textContent).toContain('o build usa bun'))
  })
})
