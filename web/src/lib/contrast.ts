/** WCAG contrast maths for the design tokens. */

export interface Rgb {
  r: number
  g: number
  b: number
}

/** Parses `#rgb` or `#rrggbb` into channels. */
export function parseHex(value: string): Rgb | null {
  const hex = value.trim().replace(/^#/, '')
  if (hex.length === 3) {
    const [r, g, b] = [...hex].map((digit) => Number.parseInt(digit + digit, 16))
    return r === undefined || g === undefined || b === undefined ? null : { r, g, b }
  }
  if (hex.length !== 6) return null
  const number = Number.parseInt(hex, 16)
  if (Number.isNaN(number)) return null
  return { r: (number >> 16) & 0xff, g: (number >> 8) & 0xff, b: number & 0xff }
}

/** Relative luminance per WCAG 2.1. */
export function relativeLuminance({ r, g, b }: Rgb): number {
  const channel = (value: number) => {
    const ratio = value / 255
    return ratio <= 0.03928 ? ratio / 12.92 : ((ratio + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/** The contrast ratio between two hex colors, or `null` when a color is bad. */
export function contrastRatio(foreground: string, background: string): number | null {
  const near = parseHex(foreground)
  const far = parseHex(background)
  if (!near || !far) return null
  const a = relativeLuminance(near)
  const b = relativeLuminance(far)
  const [light, dark] = a > b ? [a, b] : [b, a]
  return (light + 0.05) / (dark + 0.05)
}
