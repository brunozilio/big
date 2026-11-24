import type { Language } from '@/api/contract'

export type Shaped<T> = { readonly [K in keyof T]: string }

export interface Namespace<N extends string, T extends Record<string, string>> {
  readonly name: N
  readonly messages: { readonly 'pt-BR': T; readonly en: Shaped<T>; readonly es: Shaped<T> }
}

export type Flat = Readonly<Record<Language, Readonly<Record<string, string>>>>

export function namespace<N extends string, const T extends Record<string, string>>(name: N, pt: T, en: Shaped<T>, es: Shaped<T>): Namespace<N, T> {
  return { name, messages: { 'pt-BR': pt, en, es } }
}
