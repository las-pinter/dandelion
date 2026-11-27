# 0009. A flat, pressable design system

- **Status:** accepted
- **Date:** 2026-11-27

## Context

The first direction, Mission Control, used glass panels, a dark-first theme,
and a lime glow for live things. In use, the surface treatments read as
decoration, and several real actions were painted as bare text: Full screen,
the pane toggles, Load earlier, and Kill. A control that is plain text does
not look pressable, and a page cannot be scanned for what it offers.

The owner asked for a flat, modern, professional look with controls that look
like controls, light and dark both, and the OS choosing the theme by default.

## Decision

- **Surfaces are flat.** No glass, gradient, or glow. One border weight, a
  6–8 px control radius and a ~12 px panel radius, and exactly one elevation:
  the shadow on an overlay over a scrim.
- **Controls are visible.** `Button` has `default` (the one primary action),
  `outline` (a normal action), `secondary` and `quiet` (dense rows),
  `destructive`, and `link`. A press moves the control one pixel with an
  inset shadow over 80 ms; hover and focus take 120 ms; focus is a 2 px
  outline with an offset. Bare text is never an action.
- **The accent is earned.** Lime marks live activity and selection in the
  dark theme and the one primary action; light uses an ink primary. Status
  colors keep their meaning and their text labels.
- **The theme follows the OS.** A saved choice wins; otherwise the page
  matches `prefers-color-scheme` and follows it live while open. Light is no
  longer the alternate of a dark-first product.
- **Contrast is a test.** `web/src/styles/tokens.test.ts` reads the token
  stylesheet and fails when a text pair falls below WCAG AA in either theme.

## Consequences

- Every page and component was swept in one pass, so the app has one visual
  language instead of a restyled shell and untouched pages.
- The layout tokens and the agent card's height floors are untouched; the
  e2e suite still measures the composer and the phone layout.
- Editing the shadcn primitives is part of the system now. Changes stay
  token-driven and small, so a later `shadcn add` is easy to reconcile.
- The direction in `docs/design.md` is rewritten; this record supersedes the
  Mission Control direction recorded in `docs/design.md`.
