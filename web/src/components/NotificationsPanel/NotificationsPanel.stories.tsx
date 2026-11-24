import type { Meta, StoryObj } from '@storybook/react-vite'
import { NotificationsPanel } from './NotificationsPanel'

const meta: Meta<typeof NotificationsPanel> = {
  title: 'Building blocks/NotificationsPanel',
  component: NotificationsPanel,
}
export default meta

/** Storybook runs on a secure localhost origin, so the real hook asks the browser. */
export const Default: StoryObj<typeof NotificationsPanel> = {}
