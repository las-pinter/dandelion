import type { Meta, StoryObj } from '@storybook/react-vite'
import { MemoryRouter } from 'react-router'
import { NotFound } from './NotFound'

const meta: Meta<typeof NotFound> = {
  title: 'Building blocks/NotFound',
  component: NotFound,
  decorators: [
    (Story) => (
      <MemoryRouter>
        <Story />
      </MemoryRouter>
    ),
  ],
}
export default meta

export const Default: StoryObj<typeof NotFound> = {}
