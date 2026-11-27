import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { contrastRatio } from '@/lib/contrast'

/**
 * The tokens are the one place a colour is chosen, so this suite reads the
 * stylesheet itself and checks the pairs that carry text.
 */
const css = readFileSync(resolve(process.cwd(), 'src/styles/tokens.css'), 'utf8')

function block(selector: string): Map<string, string> {
  const start = css.indexOf(selector)
  if (start < 0) throw new Error(`missing block ${selector}`)
  const open = css.indexOf('{', start)
  const close = css.indexOf('}', open)
  const body = css.slice(open + 1, close)
  const tokens = new Map<string, string>()
  for (const match of body.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{3,6})/gi)) {
    const name = match[1]
    const value = match[2]
    if (name !== undefined && value !== undefined) tokens.set(name, value)
  }
  return tokens
}

const light = block(':root {')
const dark = block('.dark {')

/** Text pairs that must meet WCAG AA (4.5:1) in both themes. */
const PAIRS: Array<[foreground: string, background: string]> = [
  ['foreground', 'background'],
  ['muted-foreground', 'background'],
  ['muted-foreground', 'card'],
  ['primary-foreground', 'primary'],
  ['card-foreground', 'card'],
  ['popover-foreground', 'popover'],
  ['sidebar-foreground', 'sidebar'],
  ['status-working', 'background'],
  ['status-waiting', 'background'],
  ['status-idle', 'background'],
  ['status-done', 'background'],
  ['status-error', 'background'],
  ['secondary-foreground', 'secondary'],
  ['muted-foreground', 'muted'],
  ['accent-foreground', 'accent'],
  ['sidebar-accent-foreground', 'sidebar-accent'],
  ['destructive', 'card'],
  ['status-working', 'card'],
  ['status-waiting', 'card'],
  ['status-idle', 'card'],
  ['status-done', 'card'],
  ['status-error', 'card'],
  ['muted-foreground', 'popover'],
]

/**
 * An input's or an outline button's border is its whole affordance, so it
 * needs WCAG 1.4.11 non-text contrast (3:1) against the surface behind it.
 */
const BOUNDARIES: Array<[edge: string, surface: string]> = [
  ['input', 'card'],
  ['input', 'background'],
]

describe.each([
  ['light', light],
  ['dark', dark],
] as const)('%s tokens', (theme, tokens) => {
  it(`defines every pair (${theme})`, () => {
    for (const [foreground, background] of PAIRS) {
      expect(tokens.get(foreground), `${theme} --${foreground}`).toBeDefined()
      expect(tokens.get(background), `${theme} --${background}`).toBeDefined()
    }
  })

  it(`keeps text at AA contrast (${theme})`, () => {
    for (const [foreground, background] of PAIRS) {
      const ratio = contrastRatio(tokens.get(foreground) ?? '', tokens.get(background) ?? '')
      expect(ratio, `${theme} ${foreground} on ${background}`).not.toBeNull()
      expect(ratio ?? 0, `${theme} ${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5)
    }
  })

  it(`keeps control boundaries at 3:1 (${theme})`, () => {
    for (const [edge, surface] of BOUNDARIES) {
      const ratio = contrastRatio(tokens.get(edge) ?? '', tokens.get(surface) ?? '')
      expect(ratio, `${theme} ${edge} against ${surface}`).not.toBeNull()
      expect(ratio ?? 0, `${theme} ${edge} against ${surface}`).toBeGreaterThanOrEqual(3)
    }
  })
})

describe('contrast maths', () => {
  it('matches the WCAG reference values', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 0)
    expect(contrastRatio('#ffffff', '#ffffff')).toBeCloseTo(1, 5)
    expect(contrastRatio('#777777', '#ffffff')).toBeCloseTo(4.48, 1)
  })

  it('returns null for a value it cannot read', () => {
    expect(contrastRatio('not-a-colour', '#ffffff')).toBeNull()
  })
})

/**
 * The pre-paint script, the page shell, and the PWA manifest have to hardcode
 * a background because they run before or outside the stylesheet; this keeps
 * them from drifting away from the tokens.
 */
describe('pre-paint chrome', () => {
  const read = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8')
  const lightBackground = light.get('background') ?? ''
  const darkBackground = dark.get('background') ?? ''

  it('carries both backgrounds in the theme script', () => {
    const script = read('public/theme-init.js')
    expect(script).toContain(lightBackground)
    expect(script).toContain(darkBackground)
  })

  it('carries the light background in the shell and the manifest', () => {
    expect(read('index.html')).toContain(lightBackground)
    expect(read('public/manifest.webmanifest')).toContain(lightBackground)
  })
})
