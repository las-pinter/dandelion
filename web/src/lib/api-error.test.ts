import { describe, expect, it } from 'vitest'
import { messageOf } from './api-error'

describe('messageOf', () => {
  it('reads the message from an API error body', () => {
    expect(messageOf({ error: 'unavailable', message: 'Docker is unavailable' })).toBe(
      'Docker is unavailable',
    )
  })

  it('falls back when the failure has no message', () => {
    expect(messageOf(undefined)).toBe('the request failed')
    expect(messageOf({ error: 'unavailable' })).toBe('the request failed')
    expect(messageOf('boom')).toBe('the request failed')
    expect(messageOf(new Error('boom'), 'the action failed')).toBe('boom')
  })
})
