// How big the island is drawn. Every size in styles.css is at 100% (the
// prototype's Build spec); the whole island is scaled by one value so its
// proportions never change. 5b makes SCALE a setting (Small / Medium / Large).

/** Medium, the default (Charan). */
export const SCALE = 1.25;
/** The island never takes more than this share of the screen's width. */
const MAX_SHARE = 0.4;

/** Width of each island shape at 100%, from styles.css. */
export const WIDTH = { hidden: 180, pill: 288, open: 400, wide: 720 } as const;

/** The scale for a shape `width` px wide (at 100%) on a screen `screen` px
 * wide: SCALE, smaller if the island would pass MAX_SHARE of the screen,
 * but never below 100%. */
export function islandScale(width: number, screen: number, scale = SCALE): number {
  if (!(screen > 0)) return scale;
  return Math.max(1, Math.min(scale, (MAX_SHARE * screen) / width));
}
