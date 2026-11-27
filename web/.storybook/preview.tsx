import type { Preview } from '@storybook/react-vite'
import { useEffect } from 'react'
import { resetThemeStore } from '../src/app/theme-store'
import '../src/index.css'

/**
 * The toolbar drives the same `.dark` class the app uses, then re-reads the
 * theme store so a story that shows a theme control agrees with it.
 */
const preview: Preview = {
  globalTypes: {
    theme: {
      description: 'Theme',
      toolbar: {
        title: 'Theme',
        icon: 'circlehollow',
        items: [
          { value: 'light', title: 'Light' },
          { value: 'dark', title: 'Dark' },
        ],
        dynamicTitle: true,
      },
    },
  },
  initialGlobals: { theme: 'light' },
  decorators: [
    (Story, context) => {
      const dark = context.globals.theme === 'dark'
      useEffect(() => {
        document.documentElement.classList.toggle('dark', dark)
        document.documentElement.style.colorScheme = dark ? 'dark' : 'light'
        resetThemeStore()
      }, [dark])
      return <Story />
    },
  ],
  parameters: {
    layout: 'fullscreen',
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
  },
}

export default preview
