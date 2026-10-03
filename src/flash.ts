// When the pill shows a flash ("Auto-allowed", "Rules changed"): for
// FLASH_MS from the first moment the pill can show it. A flash raised while a
// card or a "Rule added" message is up starts when that's gone, unless it's
// stale by then.

/** How long the pill shows a flash. */
export const FLASH_MS = 2500;
/** A flash first seen later than this after it was raised is never shown. */
export const FLASH_LATE_MS = 2500;

export type Seen = { n: number; from: number };

/** Milliseconds the flash `n` (raised at `atMs`) still shows at `now`, and
 * the updated record of when the pill first showed it. */
export function flashLeft(n: number, atMs: number, now: number, seen: Seen): { left: number; seen: Seen } {
  if (seen.n !== n) {
    if (now - atMs > FLASH_LATE_MS) return { left: 0, seen };
    seen = { n, from: now };
  }
  return { left: seen.from + FLASH_MS - now, seen };
}
