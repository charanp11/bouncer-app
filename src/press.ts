// One click on the island always counts, the first time (Charan, 2026-10-16).
// A press in progress means the island is never rebuilt under the pointer:
// a rebuild between pointerdown and pointerup makes the browser drop the
// click (seen on the test instance: pointerup came, click never did). While
// a press lasts, the newest view waits; it is drawn right after the release,
// once the click has run. A press held past MAX_HOLD_MS (a drag, a release
// the page never hears) stops holding, so the island can't stall.

export const MAX_HOLD_MS = 1500;

export class PressGate<V> {
  private down = false;
  private since = 0;
  private held: V | null = null;
  private readonly draw: (view: V) => void;
  /** Runs `f` after the current input event (the click) is done. */
  private readonly after: (f: () => void) => void;
  private readonly now: () => number;

  constructor(draw: (view: V) => void, after: (f: () => void) => void, now: () => number) {
    this.draw = draw;
    this.after = after;
    this.now = now;
  }

  /** A press began on the island. */
  press() {
    this.down = true;
    this.since = this.now();
  }

  /** The press ended (released, cancelled, or the window lost focus): the
   * view that waited, if any, is drawn after the click. */
  release() {
    if (!this.down) return;
    this.down = false;
    const view = this.held;
    this.held = null;
    if (view !== null) this.after(() => this.draw(view));
  }

  /** True if `view` must wait for the release (it's kept, the newest wins);
   * false if it can be drawn now. */
  hold(view: V): boolean {
    if (this.down && this.now() - this.since < MAX_HOLD_MS) {
      this.held = view;
      return true;
    }
    this.down = false;
    this.held = null;
    return false;
  }
}
