/**
 * The control that selects a row (an agent, a container, a unit): a small
 * secondary button, so it reads as a control at rest, presses like one, and
 * still shows selection through `aria-pressed`.
 */
export const SELECT_ROW_CLASS =
  'rounded-md border border-input bg-secondary px-1.5 py-0.5 text-left font-medium transition-[background-color,border-color,box-shadow,transform] duration-[120ms] ease-out hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring active:translate-y-px active:shadow-[inset_0_1px_2px_rgb(0_0_0/0.14)] active:duration-[80ms] aria-pressed:bg-accent aria-pressed:shadow-[inset_0_1px_2px_rgb(0_0_0/0.14)]'
