import type { Meta, StoryObj } from '@storybook/react-vite'
import { AuthLayout } from './AuthLayout'

const meta: Meta<typeof AuthLayout> = {
  title: 'Auth/AuthLayout',
  component: AuthLayout,
}
export default meta

export const Default: StoryObj<typeof AuthLayout> = {
  args: {
    title: 'Sign in',
    description: 'Use a passkey or the owner password.',
    children: <p className="text-sm text-muted-foreground">Form content lives here.</p>,
  },
}
