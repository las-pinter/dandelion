# 0008. Route-level code splitting

- **Status:** accepted
- **Date:** 2026-11-27

## Context

The page registry discovers `pages/*/page.tsx` with an eager `import.meta.glob`,
so every page and everything it imports lands in the first bundle. ADR-0006
noted that `react-markdown` and `remark-gfm` added roughly 180 kB to that
bundle and deferred splitting until the budget mattered. The Documents page
made `react-markdown` a second consumer, and the build reported a 700 kB entry
chunk, past Vite's 500 kB warning.

The registry needs each page's metadata — path, title, icon, order — before
anything renders, so the metadata must stay in the eagerly imported module.

## Decision

- `pages/<screen>/page.tsx` holds only the metadata and a `lazy()` wrapper that
  imports `pages/<screen>/screen.tsx`, which holds the screen itself.
- The router wraps every route element in one `Suspense` boundary with a short
  "Loading the page…" fallback, so a page module loads on first visit.
- Page tests import the screen from `./screen`, so they keep testing the real
  component rather than the lazy wrapper.

## Consequences

- The entry chunk fell from 700 kB (217 kB gzip) to 399 kB (126 kB gzip); the
  agent, documents, and terminal pages load their heavy dependencies —
  `react-markdown`, xterm — on first visit, and Vite's size warning is gone.
- A first visit to a page shows one loading line while its chunk arrives; later
  visits are instant because the module stays cached.
- A new page follows the same two-file shape. The registry, navigation, and
  router still need no manual edit.
