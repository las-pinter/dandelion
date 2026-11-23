import type { Meta, StoryObj } from '@storybook/react-vite'
import type { SetupStatus } from '@/lib/auth'
import { SetupForm } from './SetupForm'

const empty: SetupStatus = { valid: true, has_password: false, has_passkey: false }

const meta: Meta<typeof SetupForm> = {
  title: 'Auth/SetupForm',
  component: SetupForm,
  args: {
    token: 'demo-token',
    onComplete: () => undefined,
    loadStatus: async () => empty,
  },
}
export default meta

export const Fresh: StoryObj<typeof SetupForm> = {}

export const AlmostDone: StoryObj<typeof SetupForm> = {
  args: {
    loadStatus: async () => ({ valid: true, has_password: true, has_passkey: false }),
  },
}

export const Expired: StoryObj<typeof SetupForm> = {
  args: {
    loadStatus: async () => ({ valid: false, has_password: false, has_passkey: false }),
  },
}
