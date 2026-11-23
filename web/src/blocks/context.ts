import { createContext, useContext } from 'react'

export interface BlockHost {
  item: string
  step: string
  version: string
  send: (action: string, payload: unknown) => Promise<void>
}

export const BlockContext = createContext<BlockHost>({ item: '', step: '', version: '', send: () => Promise.resolve() })

export function useBlockHost(): BlockHost {
  return useContext(BlockContext)
}
