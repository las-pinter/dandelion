import { describe, expect, it } from 'vitest'
import {
  canAct,
  containerLabel,
  isRunning,
  logTime,
  splitLogTimestamp,
  type ContainerInfo,
} from './containers'

const web: ContainerInfo = {
  id: 'abc123def4567890',
  name: 'web',
  image: 'nginx:1',
  command: 'nginx -g daemon off;',
  state: 'running',
  status: 'Up 2 minutes',
  created_at_unix: 1_700_000_000,
  labels: {},
}

describe('containers helpers', () => {
  it('gates each action on the container state', () => {
    expect(canAct('running', 'start')).toBe(false)
    expect(canAct('running', 'stop')).toBe(true)
    expect(canAct('running', 'restart')).toBe(true)

    expect(canAct('exited', 'start')).toBe(true)
    expect(canAct('exited', 'stop')).toBe(false)
    expect(canAct('exited', 'restart')).toBe(true)

    expect(canAct('created', 'start')).toBe(true)
    expect(canAct('created', 'restart')).toBe(false)

    expect(canAct('paused', 'stop')).toBe(true)
    expect(canAct('restarting', 'stop')).toBe(true)

    for (const action of ['start', 'stop', 'restart'] as const) {
      expect(canAct('dead', action)).toBe(false)
      expect(canAct('unknown', action)).toBe(false)
    }
  })

  it('counts running, restarting, and paused as running', () => {
    expect(isRunning('running')).toBe(true)
    expect(isRunning('restarting')).toBe(true)
    expect(isRunning('paused')).toBe(true)
    expect(isRunning('exited')).toBe(false)
    expect(isRunning('created')).toBe(false)
  })

  it('labels a container by name, then by a short id', () => {
    expect(containerLabel(web)).toBe('web')
    expect(containerLabel({ ...web, name: '' })).toBe('abc123def456')
    expect(containerLabel({ ...web, name: '', id: '' })).toBe('unknown')
  })

  it('splits a Docker timestamp from a log line', () => {
    expect(splitLogTimestamp('2026-11-23T12:34:56.789012345Z listening on :80')).toEqual({
      timestamp: '2026-11-23T12:34:56.789012345Z',
      text: 'listening on :80',
    })
    expect(splitLogTimestamp('no timestamp here')).toEqual({
      timestamp: null,
      text: 'no timestamp here',
    })
    expect(splitLogTimestamp('2026-11-23T12:34:56Z')).toEqual({
      timestamp: null,
      text: '2026-11-23T12:34:56Z',
    })
  })

  it('renders a timestamp or an em dash', () => {
    expect(logTime(null)).toBe('—')
    expect(logTime('not a date')).toBe('not a date')
  })
})
