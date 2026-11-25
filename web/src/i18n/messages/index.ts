import type { Language } from '@/api/contract'
import type { Flat, Namespace } from './define'
import common from './common'
import app from './app'
import settings from './settings'
import items from './items'
import chat from './chat'
import plan from './plan'
import blocks from './blocks'
import editor from './editor'
import server from './server'
import mock from './mock'

export const NAMESPACES = [common, app, settings, items, chat, plan, blocks, editor, server, mock] as const

type KeysOf<S> = S extends Namespace<infer N, infer T> ? `${N}.${keyof T & string}` : never

export type MessageKey = KeysOf<(typeof NAMESPACES)[number]>

const LANGUAGES: Language[] = ['pt-BR', 'en', 'es']

function flatten(language: Language): Record<string, string> {
  const flat: Record<string, string> = {}
  for (const { name, messages } of NAMESPACES) {
    for (const [key, text] of Object.entries(messages[language])) flat[`${name}.${key}`] = text
  }
  return flat
}

export const MESSAGES: Flat = Object.fromEntries(LANGUAGES.map((language) => [language, flatten(language)])) as Flat
