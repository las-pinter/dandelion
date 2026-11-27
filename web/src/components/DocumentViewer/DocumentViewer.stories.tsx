import type { Meta, StoryObj } from '@storybook/react-vite'
import { DocumentViewer } from './DocumentViewer'

const meta: Meta<typeof DocumentViewer> = {
  title: 'Building blocks/DocumentViewer',
  component: DocumentViewer,
  decorators: [
    (Story) => (
      <div className="h-[36rem]">
        <Story />
      </div>
    ),
  ],
}
export default meta

export const Markdown: StoryObj<typeof DocumentViewer> = {
  args: {
    collection: 'probe-visuals',
    document: {
      path: 'dandelion/visuals/2026-11-27/0900-weekly.md',
      title: 'Weekly summary',
      kind: 'markdown',
      size: 3_200,
      modified_ms: 1_700_000_000_000,
    },
    className: 'h-full',
  },
}
