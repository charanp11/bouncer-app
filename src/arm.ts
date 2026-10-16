// The arm delay on Allow-style buttons (Allow, Send answer, Approve plan, Add
// rule and allow, Delete history). It protects against a card appearing
// under the pointer, so it counts from when the card became fully visible
// (`since`), once: picks and other re-renders never restart it. A button
// that can't be pressed yet always looks it (Charan, 2026-10-16: no click is
// ever swallowed by a control that looks ready).

export const ARM_MS = 600;

export type Arm =
  /** Pressable. */
  | { look: "ready" }
  /** Not fully visible yet: the arm hasn't started (no fill). */
  | { look: "waiting" }
  /** Arming: the fill runs `left` ms more, from `waited` ms in. */
  | { look: "arming"; waited: number; left: number }
  /** Can't be pressed for another reason (a question without a pick, an
   * update that waits): disabled, no fill. */
  | { look: "blocked" };

/** The button's state at `now` for an arm counted from `since` (0: not
 * fully visible yet); `enabled` false means it can't be pressed yet anyway. */
export function armState(since: number, now: number, enabled = true): Arm {
  if (!enabled) return { look: "blocked" };
  if (since === 0) return { look: "waiting" };
  const waited = now - since;
  if (waited < ARM_MS) return { look: "arming", waited, left: ARM_MS - waited };
  return { look: "ready" };
}
