import { describe, expect, it } from 'vitest'
import { resizeMessage, shellLabel, terminalSocketUrl } from './terminal'

describe('terminal helpers', () => {
  it('builds a same-origin socket URL', () => {
    expect(terminalSocketUrl('abc')).toBe('ws://localhost:3000/api/terminals/abc/ws')
  })

  it('sends resize as a control frame', () => {
    expect(resizeMessage(120, 40)).toBe('{"type":"resize","cols":120,"rows":40}')
  })

  it('labels a tab from the shell path', () => {
    expect(shellLabel('/usr/bin/fish')).toBe('fish')
    expect(shellLabel('/bin/bash')).toBe('bash')
    expect(shellLabel('')).toBe('shell')
  })
})
