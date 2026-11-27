import type { Meta, StoryObj } from '@storybook/react-vite'
import { Button } from './button'

const meta: Meta<typeof Button> = {
  title: 'Primitives/Button',
  component: Button,
}
export default meta

/** Every variant on one page; press one to feel the 1 px press. */
export const Variants: StoryObj<typeof Button> = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button>Primary</Button>
      <Button variant="outline">Outline</Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="quiet">Quiet</Button>
      <Button variant="ghost">Ghost</Button>
      <Button variant="destructive">Destructive</Button>
      <Button disabled>Disabled</Button>
      <Button variant="link">Link</Button>
    </div>
  ),
}

export const Sizes: StoryObj<typeof Button> = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button size="xs">Extra small</Button>
      <Button size="sm">Small</Button>
      <Button size="default">Default</Button>
      <Button size="lg">Large</Button>
      <Button size="icon" aria-label="Icon button">
        ⌘
      </Button>
    </div>
  ),
}

export const States: StoryObj<typeof Button> = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button aria-pressed="true">Toggled on</Button>
      <Button variant="outline" aria-pressed="true">
        Toggled outline
      </Button>
      <Button aria-expanded="true">Expanded menu</Button>
      <Button disabled>Disabled</Button>
    </div>
  ),
}

/** Toggled controls, which is how a segmented range reads. */
export const Pressed: StoryObj<typeof Button> = {
  render: () => (
    <div className="flex items-center gap-1">
      <Button variant="outline" aria-pressed="false">
        1h
      </Button>
      <Button variant="outline" aria-pressed="true">
        7d
      </Button>
    </div>
  ),
}
