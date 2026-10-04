// How big the island is drawn. Every size in styles.css is at 100% (the
// prototype's Build spec); the whole island is scaled by one value so its
// proportions never change. The value is the Size preference.

export type Size = "small" | "medium" | "large";

export const SCALES: Record<Size, number> = { small: 1, medium: 1.25, large: 1.5 };
/** Medium, the default (Charan). */
export const SCALE = SCALES.medium;
/** The island never takes more than this share of the screen's width. */
const MAX_SHARE = 0.4;
/** Open island max height at 100%, its gap from the screen top and the room
 * kept below it for the shadow (styles.css). */
const OPEN_MAX_H = 520;
const TOP_GAP = 8;
const SHADOW_B = 32;

/** Width of each island shape at 100%, from styles.css. */
export const WIDTH = { hidden: 180, pill: 288, open: 400, wide: 720 } as const;

/** The scale for a shape `width` px wide (at 100%) on a screen `screen` px
 * wide: `scale`, smaller if the island would pass MAX_SHARE of the screen,
 * but never below 100%. */
export function islandScale(width: number, screen: number, scale = SCALE): number {
  if (!(screen > 0)) return scale;
  return Math.max(1, Math.min(scale, (MAX_SHARE * screen) / width));
}

/** The open island's max height at 100%, so that at `scale` it never passes
 * the work area (`avail` px tall); then its body scrolls. */
export function openMaxHeight(avail: number, scale: number): number {
  if (!(avail > 0)) return OPEN_MAX_H;
  return Math.max(120, Math.min(OPEN_MAX_H, avail / scale - TOP_GAP - SHADOW_B));
}

/** A saved size name, or the default. */
export function scaleOf(size: string | undefined): number {
  return SCALES[size as Size] ?? SCALE;
}
