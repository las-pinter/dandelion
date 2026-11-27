import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"
import { Slot } from "radix-ui"

/**
 * Flat controls: a 1px border, a surface fill, and a press that moves the
 * control one pixel with an inset shadow. The primary action is the only
 * filled button on a screen; `quiet` stays for dense rows, never a page action.
 */
const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center gap-1.5 rounded-lg border text-sm font-medium whitespace-nowrap transition-[background-color,border-color,color,box-shadow,transform] duration-[120ms] ease-out select-none active:not-aria-[haspopup]:translate-y-px active:not-aria-[haspopup]:shadow-[inset_0_1px_2px_rgb(0_0_0/0.14)] active:not-aria-[haspopup]:duration-[80ms] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-45 disabled:border-border [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default:
          "border-primary bg-primary text-primary-foreground hover:border-primary/90 hover:bg-primary/90",
        outline:
          "border-input bg-card text-foreground hover:bg-muted aria-expanded:bg-muted aria-pressed:bg-muted",
        secondary:
          "border-input bg-secondary text-secondary-foreground hover:bg-accent aria-expanded:bg-accent",
        quiet:
          "border-transparent bg-transparent text-muted-foreground hover:border-border hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground",
        ghost:
          "border-transparent bg-transparent text-foreground hover:border-border hover:bg-muted aria-expanded:border-border aria-expanded:bg-muted",
        destructive:
          "border-input bg-card text-destructive hover:border-destructive hover:bg-destructive/10 focus-visible:outline-destructive aria-invalid:border-destructive",
        link: "border-transparent bg-transparent text-primary underline-offset-4 hover:underline",
      },
      size: {
        default:
          "h-8 px-3 has-data-[icon=inline-end]:pr-2.5 has-data-[icon=inline-start]:pl-2.5",
        xs: "h-7 gap-1 rounded-[min(var(--radius),8px)] px-2 text-xs has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        sm: "h-7 rounded-[min(var(--radius),8px)] px-2.5 text-[0.8rem] has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 px-3.5 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
        icon: "size-8",
        "icon-xs": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
        "icon-sm": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
