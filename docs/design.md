# Design: flat surfaces, pressable controls

Dandelion looks like a well-made instrument: plain surfaces, hairline borders,
one accent that marks what is alive, and controls that plainly look pressable.
The tokens live in `web/src/styles/tokens.css`; components use only their
semantic names (`bg-card`, `text-muted-foreground`, `text-status-waiting`).

## Principles

1. **Flat by default.** No glass, gradients, or glow. Surfaces are solid; the
   only elevation is the shadow on an overlay (dialog, command palette) over a
   scrim.
2. **One border weight, one radius.** Every surface is a 1 px border at a
   6–8 px control radius or a ~12 px panel radius. Pill shapes belong to
   status badges only.
3. **Buttons look like buttons.** An action carries a border or a fill, a
   hover, and a press that moves one pixel with an inset shadow. Bare text is
   never an action. `quiet` stays for dense rows; a page-level action uses
   `outline` or `default`.
4. **One accent, earned.** Lime marks live activity and the current selection
   in the dark theme; the ink primary carries the main action in light. One
   primary action per view.
5. **The OS picks the theme.** A saved choice wins; otherwise the page follows
   `prefers-color-scheme` and keeps following it while open. Both themes are
   complete and meet WCAG AA for text (`src/styles/tokens.test.ts`).
6. **State beats data.** Each screen leads with what needs attention (an
   agent waiting for permission, a failed unit), then the numbers.
7. **Data in mono, words in sans.** Space Grotesk for interface text;
   JetBrains Mono for values, paths, commands, ids, and terminal output.
   Numbers use tabular figures so live values do not jitter.
8. **Keyboard everywhere.** Every action is reachable from the command
   palette (`⌘K` / `Ctrl+K`); lists move with `j`/`k` and open with `Enter`.
9. **Phone-solid.** The sidebar becomes a tab bar on narrow screens, tables
   scroll in their own box, and every touch target is at least 44 px.

## Controls

| Intent | Variant | Where |
|--------|---------|-------|
| The one main action | `default` (ink on light, lime on dark) | Send, Approve while blocked, a page's primary command |
| A normal action | `outline` (surface + border) | Refresh, Full screen, pane toggles, Deny, Interrupt |
| A densely repeated action | `secondary` or `quiet` | Segmented ranges, row-level commands |
| A destructive action | `destructive` (red text, border; red fill on hover) | Kill, stop, disable |
| Navigation | `link` | Rare; a real link is preferred |
| A dialog's corner close | `ghost` with `size="icon-sm"` | Only inside an overlay; never for a page action |

Sizes: 28 px (`xs`, `sm`), 32 px (default, icons), 36 px (`lg`). The press is
`translateY(1px)` plus a 1 px inset shadow over 80 ms; hover and focus
transitions take 120 ms. Focus is a 2 px outline with a 2 px offset, never a
glow.

An input's or an outline button's border is its whole affordance, so it uses
`--input`, which holds 3:1 against the surface behind it (WCAG 1.4.11,
checked in `src/styles/tokens.test.ts`). `--border` is for panel separators,
which need no contrast.

## Status language

One meaning per color, used the same way in every screen:

| State | Token | Meaning | Motion |
|---|---|---|---|
| working | `status-working` | Running right now | Soft dot pulse |
| waiting | `status-waiting` | Needs the owner (permission, question, auth) | Slow dot blink; panel border tinted |
| idle | `status-idle` | Alive, nothing to do | None |
| done | `status-done` | Finished, result not yet seen | None |
| error | `status-error` | Failed or lost | None |

Color is never the only signal: each state also has a text label.

## Surfaces

- **Background** `background`: the page ground, plain in both themes.
- **Panels** `card` with a 1 px `border` and `rounded-xl`: lists, metrics,
  details. No shadow.
- **Rows** inside a panel: a muted header band and a hover fill; the selected
  row is filled and, in the sidebar, carries a 2 px edge in `primary`. The
  brand mark uses `primary` too; status colours are for state only.
- **Overlays** `popover` with a 1 px border and the one allowed shadow.
- **Terminal and screen output**: inset with mono type, never wrapped in
  decoration.

## Motion

Short and functional: 120 ms for hover, focus, and selection; the press takes
80 ms; the pulse and blink belong only to live states.
`prefers-reduced-motion` turns all animation off (see `web/src/index.css`).

## Components

Build from shadcn primitives in `web/src/components/ui/`, styled only through
tokens. `Button` has a story with every variant and state
(`src/components/ui/Button.stories.tsx`); new visual patterns get a story in
Storybook before a page uses them.
