import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { AgentAction } from '@/lib/agents'
import { AnswerBar } from './AnswerBar'

function setup(busy = false) {
  const onAct = vi.fn<(action: AgentAction) => void>()
  render(<AnswerBar busy={busy} onAct={onAct} />)
  return { onAct }
}

describe('AnswerBar', () => {
  it('moves the highlight and accepts or skips the dialog', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.click(screen.getByRole('button', { name: 'Up' }))
    await user.click(screen.getByRole('button', { name: 'Down' }))
    await user.click(screen.getByRole('button', { name: 'Accept (Enter)' }))
    await user.click(screen.getByRole('button', { name: 'Skip (Escape)' }))

    expect(onAct).toHaveBeenNthCalledWith(1, { kind: 'keys', keys: ['up'] })
    expect(onAct).toHaveBeenNthCalledWith(2, { kind: 'keys', keys: ['down'] })
    expect(onAct).toHaveBeenNthCalledWith(3, { kind: 'approve' })
    expect(onAct).toHaveBeenNthCalledWith(4, { kind: 'deny' })
  })

  it('types an answer into the dialog and clears the box', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.type(screen.getByLabelText('Answer'), 'use option two{Enter}')

    expect(onAct).toHaveBeenCalledWith({ kind: 'answer', text: 'use option two' })
    expect(screen.getByLabelText('Answer')).toHaveValue('')
  })

  it('keeps Shift+Enter as a newline', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.type(screen.getByLabelText('Answer'), 'first{Shift>}{Enter}{/Shift}second')

    expect(onAct).not.toHaveBeenCalled()
    expect(screen.getByLabelText('Answer')).toHaveValue('first\nsecond')
  })

  it('disables its controls while an answer is in flight', () => {
    setup(true)

    expect(screen.getByRole('button', { name: 'Up' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Accept (Enter)' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Skip (Escape)' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Answer' })).toBeDisabled()
  })
})
