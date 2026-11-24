import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { TerminalInfo } from '@/lib/terminal'
import { useTerminals } from './use-terminals'

const { post, del } = vi.hoisted(() => ({
  post: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
  del: vi.fn<(path: string, options?: unknown) => Promise<{ error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { POST: post, DELETE: del } }))

const terminal: TerminalInfo = {
  id: 'aaa111bbb222',
  shell: '/bin/bash',
  cols: 80,
  rows: 24,
  created_at_unix: 1_700_000_000,
  attached: false,
}

beforeEach(() => {
  post.mockReset()
  del.mockReset()
})

describe('useTerminals', () => {
  it('opens a shell with the requested size', async () => {
    post.mockResolvedValue({ data: terminal })

    const { result } = renderHook(() => useTerminals())
    let opened: TerminalInfo | null = null
    await act(async () => {
      opened = await result.current.open(100, 40)
    })

    expect(post).toHaveBeenCalledWith('/api/terminals', { body: { cols: 100, rows: 40 } })
    expect(opened).toEqual(terminal)
    expect(result.current.busy).toBe(false)
    expect(result.current.error).toBeNull()
  })

  it('shows a refused open', async () => {
    post.mockResolvedValue({
      error: { error: 'limit_reached', message: 'at most 8 terminals may be open' },
    })

    const { result } = renderHook(() => useTerminals())
    let opened: TerminalInfo | null = terminal
    await act(async () => {
      opened = await result.current.open()
    })

    expect(opened).toBeNull()
    expect(result.current.error).toContain('at most 8 terminals')
  })

  it('closes a shell', async () => {
    del.mockResolvedValue({})

    const { result } = renderHook(() => useTerminals())
    await act(async () => {
      await result.current.close(terminal.id)
    })

    expect(del).toHaveBeenCalledWith('/api/terminals/{id}', {
      params: { path: { id: terminal.id } },
    })
    expect(result.current.error).toBeNull()
  })

  it('shows a failed close', async () => {
    del.mockResolvedValue({ error: { message: 'the server said no' } })

    const { result } = renderHook(() => useTerminals())
    await act(async () => {
      await result.current.close(terminal.id)
    })

    expect(result.current.error).toBe('the server said no')
  })

  it('treats an already-closed shell as closed', async () => {
    del.mockResolvedValue({ error: { error: 'not_found', message: 'no such terminal' } })

    const { result } = renderHook(() => useTerminals())
    await act(async () => {
      await result.current.close(terminal.id)
    })

    expect(result.current.error).toBeNull()
  })
})
