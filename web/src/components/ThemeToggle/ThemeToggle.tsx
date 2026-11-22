import { Moon, Sun } from 'lucide-react'
import { useState } from 'react'
import { applyTheme, currentTheme, storeTheme, type Theme } from '@/app/theme'
import { Button } from '@/components/ui/button'

/** Switch between the dark default and the light alternate, remembering the choice. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(() => currentTheme())
  const next: Theme = theme === 'dark' ? 'light' : 'dark'

  function toggle() {
    applyTheme(next)
    storeTheme(next)
    setTheme(next)
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className="size-11"
      onClick={toggle}
      aria-label={`Switch to ${next} theme`}
    >
      {theme === 'dark' ? <Sun aria-hidden /> : <Moon aria-hidden />}
    </Button>
  )
}
