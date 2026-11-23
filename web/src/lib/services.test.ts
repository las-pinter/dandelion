import type { AllowRule } from './services'
import {
  CONFIRMED_ACTIONS,
  UNIT_ACTION_LABELS,
  UNIT_ACTIONS,
  allows,
  journalPriority,
  journalTime,
  unitType,
} from './services'

const rules: AllowRule[] = [{ unit: 'nginx.service', verbs: ['restart'] }]

describe('services helpers', () => {
  it('labels every action and confirms only interrupting ones', () => {
    expect(UNIT_ACTIONS).toHaveLength(5)
    expect(UNIT_ACTION_LABELS.restart).toBe('Restart')
    expect(CONFIRMED_ACTIONS.has('restart')).toBe(true)
    expect(CONFIRMED_ACTIONS.has('start')).toBe(false)
  })

  it('reads the unit type from the suffix', () => {
    expect(unitType('nginx.service')).toBe('service')
    expect(unitType('app.socket')).toBe('socket')
    expect(unitType('backup.timer')).toBe('timer')
    expect(unitType('odd-name')).toBe('other')
  })

  it('lets user units act and gates system units on the allowlist', () => {
    expect(allows(rules, 'user', 'anything.service', 'stop')).toBe(true)
    expect(allows(rules, 'system', 'nginx.service', 'restart')).toBe(true)
    expect(allows(rules, 'system', 'nginx.service', 'stop')).toBe(false)
    expect(allows(rules, 'system', 'other.service', 'restart')).toBe(false)
  })

  it('formats journal time and priority', () => {
    expect(journalTime(null)).toBe('—')
    expect(journalTime(1_700_000_000_000_000)).not.toBe('—')
    expect(journalPriority(3)).toBe('err')
    expect(journalPriority(7)).toBe('debug')
    expect(journalPriority(null)).toBe('—')
  })
})
