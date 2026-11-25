import { describe, expect, it } from 'bun:test'
import { diffRows } from '../src/blocks/review'

describe('diffRows', () => {
  it('numbers each hunk from its own header and anchors only lines of the new file', () => {
    const diff = ['@@ -48,2 +48,3 @@ function a() {', ' um', '+dois', ' tres', '@@ -80,2 +97,2 @@ function b() {', '-velho', '+novo', ' fim'].join('\n')
    const rows = diffRows({ diff, firstLine: 1 })
    expect(rows.map((r) => [r.sign, r.n, r.at])).toEqual([
      ['@', null, null],
      [' ', 48, 48],
      ['+', 49, 49],
      [' ', 50, 50],
      ['@', null, null],
      ['-', 80, null],
      ['+', 97, 97],
      [' ', 98, 98],
    ])
  })

  it('starts at the first line when the patch has no header', () => {
    expect(diffRows({ diff: ' a\n+b', firstLine: 41 }).map((r) => r.at)).toEqual([41, 42])
  })
})
