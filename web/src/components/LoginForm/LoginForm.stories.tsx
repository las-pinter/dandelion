import type { Meta, StoryObj } from '@storybook/react-vite'
import { LoginForm } from './LoginForm'

const meta: Meta<typeof LoginForm> = {
  title: 'Auth/LoginForm',
  component: LoginForm,
}
export default meta

export const Default: StoryObj<typeof LoginForm> = {
  args: { onSuccess: () => undefined },
}
