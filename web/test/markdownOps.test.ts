import { describe, expect, test } from 'bun:test'
import { currentLanguage, translate } from '@/i18n'
import { improveMarkdown } from '@/ui/markdownOps'

describe('improveMarkdown', () => {
  test('keeps a text that already has goal and criteria headings in any of the three languages', () => {
    const texts = [
      '## Objetivo\n\nx\n\n## Critérios de aceite\n\n- [ ] y',
      '## Goal\n\nx\n\n## Acceptance criteria\n\n- [ ] y',
      '## Objetivo\n\nx\n\n## Criterios de aceptación\n\n- [ ] y',
    ]
    for (const text of texts) expect(improveMarkdown(text)).toBe(text)
  })

  test('writes the headings in the current language', () => {
    const out = improveMarkdown('Cadastrar clientes. Validar CPF.')
    const lang = currentLanguage()
    expect(out.startsWith(`## ${translate(lang, 'editor.goalHeading')}\n\nCadastrar clientes.`)).toBe(true)
    expect(out).toContain(`## ${translate(lang, 'editor.scopeHeading')}\n\n- Validar CPF.`)
    expect(out).toContain(`## ${translate(lang, 'editor.criteriaHeading')}\n\n- [ ] Cadastrar clientes`)
  })

  test('an empty text gets the placeholder goal and an undefined scope', () => {
    const lang = currentLanguage()
    const out = improveMarkdown('')
    expect(out).toContain(translate(lang, 'editor.goalPlaceholder'))
    expect(out).toContain(`- ${translate(lang, 'editor.toDefine')}`)
  })
})
