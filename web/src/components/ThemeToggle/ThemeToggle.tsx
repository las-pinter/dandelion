import { Moon, Sun } from 'lucide-react'
import type { Theme } from '@/app/theme'
import { chooseTheme, useTheme } from '@/app/theme-store'
import { Button } from '@/components/ui/button'

/**
 * Switch between light and dark, remembering the choice. Until the owner
 * chooses, the app follows the OS; the toggle shows whichever theme is on.
 */
export function ThemeToggle() {
  const theme = useTheme()
  const next: Theme = theme === 'dark' ? 'light' : 'dark'

  return (
    <Button
      type="button"
      variant="outline"
      size="icon"
      className="size-11"
      onClick={() => chooseTheme(next)}
      aria-label={`Switch to ${next} theme`}
      title={`Switch to the ${next} theme`}
    >
      {theme === 'dark' ? <Sun aria-hidden /> : <Moon aria-hidden />}
    </Button>
  )
}
