import type { Meta, StoryObj } from '@storybook/react-vite'
import { AnswerBar } from './AnswerBar'

const meta: Meta<typeof AnswerBar> = {
  title: 'Building blocks/AnswerBar',
  component: AnswerBar,
}
export default meta

export const Question: StoryObj<typeof AnswerBar> = {
  args: { onAct: () => undefined },
}

export const Busy: StoryObj<typeof AnswerBar> = {
  args: { busy: true, onAct: () => undefined },
}
