import { describe, expect, test } from 'bun:test'
import { interpolate, languageOf, translate } from '@/i18n'
import { MESSAGES, NAMESPACES } from '@/i18n/messages'

describe('i18n', () => {
  test('the three dictionaries have the same keys and no empty text', () => {
    const source = Object.keys(MESSAGES['pt-BR']).sort()
    expect(source.length).toBeGreaterThan(0)
    for (const language of ['en', 'es'] as const) {
      expect(Object.keys(MESSAGES[language]).sort()).toEqual(source)
    }
    for (const language of ['pt-BR', 'en', 'es'] as const) {
      for (const [key, text] of Object.entries(MESSAGES[language])) expect([key, text.trim()]).not.toEqual([key, ''])
    }
  })

  test('each namespace is registered once and keys carry its name', () => {
    const names = NAMESPACES.map((n) => n.name)
    expect(new Set(names).size).toBe(names.length)
    expect(Object.keys(MESSAGES.en).every((key) => names.some((name) => key.startsWith(`${name}.`)))).toBe(true)
  })

  test('a text is found in the chosen language and fills its variables', () => {
    expect(translate('en', 'common.language')).toBe('Language')
    expect(translate('pt-BR', 'common.language')).toBe('Idioma')
    expect(translate('es', 'common.languageHint')).toContain('agentes')
    expect(translate('en', 'common.language', { unused: 1 })).toBe('Language')
    expect(interpolate('{count} itens de {name}', { count: 3, name: 'Ana' })).toBe('3 itens de Ana')
    expect(interpolate('{missing} fica', {})).toBe('{missing} fica')
  })

  test('a browser tag maps to one of the three languages', () => {
    expect(languageOf('pt-PT')).toBe('pt-BR')
    expect(languageOf('es-AR')).toBe('es')
    expect(languageOf('en-GB')).toBe('en')
    expect(languageOf('fr-FR')).toBe(null)
  })
})
