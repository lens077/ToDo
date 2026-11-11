/**
 * Every top-level view, in the sidebar's reading order.
 *
 * Kept in `lib` so the preference layer can validate view ids without pulling
 * in React components (the sidebar owns labels and icons, not identity).
 */
export const MAIN_VIEW_IDS = [
  'overview',
  'reading',
  'workspaces',
  'goal',
  'quadrant',
  'capture',
  'ai',
  'review',
  'contract',
] as const;

export type MainView = (typeof MAIN_VIEW_IDS)[number];

export function isMainView(value: unknown): value is MainView {
  return typeof value === 'string' && (MAIN_VIEW_IDS as readonly string[]).includes(value);
}
