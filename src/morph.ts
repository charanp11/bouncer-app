// How the island moves from one view to the next (prototype "Motion"):
// growing, the window gets the bigger size first and the island animates
// inside it; closing, the island animates first and the window shrinks
// after. A card is never animated (in, out or under), and nothing moves
// with reduced motion. No DOM here, so it can be tested.

export type Size = { w: number; h: number };
export type Plan = { kind: "still" } | { kind: "grow" | "close"; hold: Size };

/** Below this (px) a size change isn't worth animating (a re-render). */
const SAME = 0.5;
/** The viewport may be this much (px) smaller and still hold the island
 * (rounding between the page's and the window's pixels). */
const SLACK = 2;

/** What to do going from the island box `from` to `to`, whose page (with
 * the shadow room) is `page`, in a window now `window`. `still`: a card is
 * or was in front, reduced motion, or a window that can't be transparent. */
export function plan(from: Size, to: Size, page: Size, window: Size, still: boolean): Plan {
  if (still) return { kind: "still" };
  if (Math.abs(from.w - to.w) < SAME && Math.abs(from.h - to.h) < SAME) return { kind: "still" };
  // The window holds both the old island (still showing) and the new page.
  const hold = { w: Math.ceil(Math.max(window.w, page.w)), h: Math.ceil(Math.max(window.h, page.h)) };
  return { kind: to.w * to.h >= from.w * from.h ? "grow" : "close", hold };
}

/** The viewport `view` shows all of `page`: what "fully visible" means. */
export function holds(view: Size, page: Size): boolean {
  return view.w >= page.w - SLACK && view.h >= page.h - SLACK;
}
