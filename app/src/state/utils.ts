/**
 * Minimal `cn` helper for conditional className composition, used by
 * shadcn/ui-generated components. No additional dependency (clsx/
 * tailwind-merge) is introduced for this milestone -- this plain
 * implementation is sufficient until real components are added.
 */
export function cn(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}
