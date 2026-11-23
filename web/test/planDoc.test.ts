import { describe, expect, test } from 'bun:test'
import type { Item, ItemStep } from '../src/api/contract'
import { fileEntry, parsePlan, structured } from '../src/chat/planDoc'
import { planDestination } from '../src/screens/items/ItemModal'

const text = `# Login com SSO

**Contexto**: o login só aceita senha.

## Abordagem
1. Criar o provedor \`saml\`
2. Ligar a rota de callback
   com teste de integração

## Arquivos críticos
- \`crates/server/src/sign_in.rs\` — rota nova
- web/src/screens/Login.tsx

## Riscos
- Sessões antigas expiram

## Verificação
- \`cargo nextest run -p server\`

\`\`\`
## não é seção
\`\`\``

describe('plano estruturado', () => {
  test('separa título, seções e itens', () => {
    const doc = parsePlan(text)
    expect(doc.title).toBe('Login com SSO')
    expect(doc.sections.map((s) => s.kind)).toEqual(['context', 'steps', 'files', 'risks', 'checks'])
    expect(doc.sections[1].items).toEqual(['Criar o provedor `saml`', 'Ligar a rota de callback\ncom teste de integração'])
    expect(doc.sections[4].body).toContain('## não é seção')
    expect(structured(doc)).toBe(true)
  })

  test('texto sem seções fica como markdown', () => {
    expect(structured(parsePlan('Só um parágrafo.'))).toBe(false)
  })

  test('arquivo com e sem nota', () => {
    expect(fileEntry('`crates/a.rs` — rota nova')).toEqual({ path: 'crates/a.rs', note: 'rota nova' })
    expect(fileEntry('web/b.tsx')).toEqual({ path: 'web/b.tsx', note: '' })
  })
})

const stepOf = (id: string, extra: Partial<ItemStep> = {}): ItemStep => ({ id, label: id, icon: 'sCode', state: 'pending', attempts: 0, seconds: 0, ...extra })
const itemWith = (steps: ItemStep[], issue: Item['issue'] = null) => ({ issue, steps }) as Item

describe('destino do plano', () => {
  const steps = [stepOf('plan', { state: 'waiting_human' }), stepOf('ticket', { needs: 'tracker', provider: 'Tracker', logo: '<svg/>' }), stepOf('develop')]

  test('a próxima etapa que cria a issue dá nome ao botão', () => {
    expect(planDestination(itemWith(steps), 'plan')).toEqual({ name: 'Tracker', logo: '<svg/>' })
  })

  test('sem etapa de issue, com issue já ligada ou com a issue já criada, só aprova', () => {
    expect(planDestination(itemWith([steps[0], steps[2]]), 'plan')).toBeNull()
    expect(planDestination(itemWith(steps, { connection: 'c', key: 'K-1' } as Item['issue']), 'plan')).toBeNull()
    expect(planDestination(itemWith([steps[0], { ...steps[1], state: 'passed' }]), 'plan')).toBeNull()
  })
})

describe('o modal acompanha o workflow', () => {
  test('quem olhava a etapa atual vai para a próxima quando ela começa', async () => {
    const { followed } = await import('../src/screens/items/ItemModal')
    expect(followed('plan', 'ticket', 'plan')).toBe('ticket')
    expect(followed('plan', 'ticket', 'review')).toBeNull()
    expect(followed('plan', 'plan', 'plan')).toBeNull()
    expect(followed('plan', null, 'plan')).toBeNull()
  })
})
