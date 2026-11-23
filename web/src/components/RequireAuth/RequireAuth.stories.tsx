import type { Meta, StoryObj } from '@storybook/react-vite'
import { RequireAuth } from './RequireAuth'

const meta: Meta<typeof RequireAuth> = {
  title: 'Auth/RequireAuth',
  component: RequireAuth,
  args: {
    loadStatus: async () => ({ setup_required: false, authenticated: true }),
    children: <p className="p-6 text-sm">The signed-in app renders here.</p>,
  },
}
export default meta

export const SignedIn: StoryObj<typeof RequireAuth> = {}
