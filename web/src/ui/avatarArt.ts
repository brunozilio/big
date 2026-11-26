import { createAvatar } from '@dicebear/core'
import { notionists } from '@dicebear/collection'

export function drawAvatar(seed: string): string {
  return createAvatar(notionists, { seed, backgroundColor: ['f4e8ff', 'ffe9d6', 'dff4ea', 'e3efff', 'fff3c4', 'ffe1ea'], backgroundType: ['solid'] }).toDataUri()
}
