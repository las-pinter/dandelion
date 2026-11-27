import type { Meta, StoryObj } from '@storybook/react-vite'
import type { DocumentsSnapshot } from '@/lib/documents'
import { DocumentsList } from './DocumentsList'

const snapshot: DocumentsSnapshot = {
  collections: [
    {
      id: 'probe-visuals',
      name: 'Agent visuals',
      icon: 'chart',
      root: '~/agent-notes/project-notes',
      groups: [
        {
          values: ['dandelion', '2026-11-27'],
          documents: [
            {
              path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
              title: 'First release',
              kind: 'html',
              size: 41_000,
              modified_ms: 1_700_000_000_000,
            },
            {
              path: 'dandelion/visuals/2026-11-27/0900-weekly.md',
              title: 'Weekly summary',
              kind: 'markdown',
              size: 3_200,
              modified_ms: 1_699_000_000_000,
            },
          ],
        },
        {
          values: ['dandelion', '2026-11-26'],
          documents: [
            {
              path: 'dandelion/visuals/2026-11-26/1800-recap.html',
              title: 'Day recap',
              kind: 'html',
              size: 22_000,
              modified_ms: 1_698_000_000_000,
            },
          ],
        },
      ],
    },
  ],
  warnings: [{ file: 'broken.json', problem: 'not valid JSON: expected value at line 1 column 1' }],
}

const meta: Meta<typeof DocumentsList> = {
  title: 'Building blocks/DocumentsList',
  component: DocumentsList,
}
export default meta

export const Default: StoryObj<typeof DocumentsList> = {
  args: {
    snapshot,
    selected: null,
    onSelect: () => undefined,
  },
}

export const Empty: StoryObj<typeof DocumentsList> = {
  args: {
    snapshot: { collections: [], warnings: [] },
    selected: null,
    onSelect: () => undefined,
  },
}
