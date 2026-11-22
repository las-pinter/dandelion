# Design: Mission Control

Dandelion looks like a calm mission-control room at night: a deep dark
canvas, glass panels, and one electric lime accent that marks what is alive.
The tokens live in `web/src/styles/tokens.css`; components use only their
semantic names (`bg-card`, `text-muted-foreground`, `text-status-waiting`).

## Principles

1. **Dark first.** Dark is the primary theme and the default; light is a
   full alternate, not an afterthought. Every color pair meets WCAG AA
   (4.5:1 for text) in both themes.
2. **One accent, earned.** Lime (`brand`, `primary`) marks live activity,
   the current selection, and the primary action, never decoration. One
   primary action per view.
3. **Glow means alive.** `shadow-glow` and the pulse animation belong only
   to things that are running right now: a working agent, a live chart.
   Static content never glows.
4. **State beats data.** Each screen leads with what needs attention (an
   agent waiting for permission, a failed unit), then the numbers.
5. **Data in mono, words in sans.** Space Grotesk for interface text;
   JetBrains Mono for values, paths, commands, ids, and terminal output.
   Numbers use tabular figures so live values do not jitter.
6. **Keyboard everywhere.** Every action is reachable from the command
   palette (`⌘K` / `Ctrl+K`); lists move with `j`/`k` and open with `Enter`.
7. **Phone-solid.** The sidebar stacks on narrow screens, tables scroll in
   their own box, and every touch target is at least 44 px.

## Status language

One meaning per color, used the same way in every screen:

| State | Token | Meaning | Motion |
|---|---|---|---|
| working | `status-working` | Running right now | Soft pulse |
| waiting | `status-waiting` | Needs the owner (permission, question, auth) | Slow blink; panel border tinted |
| idle | `status-idle` | Alive, nothing to do | None |
| done | `status-done` | Finished, result not yet seen | None |
| error | `status-error` | Failed or lost | None |

Color is never the only signal: each state also has a text label.

## Surfaces

- **Background** `background`: the deepest layer; may carry one faint lime
  radial highlight at the top of the page.
- **Panels** `card` with a 1 px `border` and `rounded-2xl`: the glass panels
  that hold metrics, lists, and details.
- **Overlays** `popover`: menus, the command palette, dialogs.
- **Terminal and screen output**: near-black inset, mono, never wrapped in
  decoration.

## Motion

Short and functional: 150–200 ms for hover and selection, the pulse and
blink only for live states. `prefers-reduced-motion` turns all animation off
(see `web/src/index.css`).

## Components

Build from shadcn primitives in `web/src/components/ui/`, styled only
through tokens. New visual patterns get a story in Storybook before a page
uses them.
