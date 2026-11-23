import type { ProcessInfo } from './processes'
import { PROCESS_STATE_LABELS, formatCpuTime, isControllable, matchesQuery } from './processes'

const process: ProcessInfo = {
  pid: 1234,
  parent_pid: 1,
  name: 'node',
  user: 'dev',
  state: 'sleeping',
  cpu_seconds: 12.5,
  memory_bytes: 2048,
  command: 'node vite --host 0.0.0.0',
  started_at_unix: 1_700_000_000,
  niceness: 0,
}

describe('processes helpers', () => {
  it('labels every scheduling state', () => {
    expect(PROCESS_STATE_LABELS.sleeping).toBe('Sleeping')
    expect(PROCESS_STATE_LABELS.disk_sleep).toBe('Disk sleep')
    expect(PROCESS_STATE_LABELS.unknown).toBe('Unknown')
  })

  it('refuses to control pid 0 and pid 1', () => {
    expect(isControllable({ ...process, pid: 0 })).toBe(false)
    expect(isControllable({ ...process, pid: 1 })).toBe(false)
    expect(isControllable(process)).toBe(true)
  })

  it('formats CPU time at second, minute and hour scale', () => {
    expect(formatCpuTime(1.234)).toBe('1.2s')
    expect(formatCpuTime(75)).toBe('1m 15s')
    expect(formatCpuTime(3_725)).toBe('1h 2m')
  })

  it('matches pid, name, command and user, case-insensitively', () => {
    expect(matchesQuery(process, '')).toBe(true)
    expect(matchesQuery(process, '1234')).toBe(true)
    expect(matchesQuery(process, 'NODE')).toBe(true)
    expect(matchesQuery(process, 'vite')).toBe(true)
    expect(matchesQuery(process, 'DEV')).toBe(true)
    expect(matchesQuery(process, 'postgres')).toBe(false)
  })
})
