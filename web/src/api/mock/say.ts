import { currentLanguage, translate, type MessageKey, type Vars } from '@/i18n'

type MockKey = MessageKey extends infer K ? (K extends `mock.${infer R}` ? R : never) : never

export function say(key: MockKey, vars?: Vars): string {
  return translate(currentLanguage(), `mock.${key}` as MessageKey, vars)
}
