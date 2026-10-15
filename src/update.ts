// The update line (v0.8, prototype v0.29): what its button says and whether
// it can be pressed. The backend refuses the same cases on its own.

export type Update = { version: string; installing: boolean };

/** Installing: "Updating…"; a card waiting or a chat answering: "Update
 * after you answer" (Bouncer never quits mid-decision); else "Update". */
export function updateButton(update: Update, busy: boolean): { label: string; enabled: boolean } {
  if (update.installing) return { label: "Updating…", enabled: false };
  if (busy) return { label: "Update after you answer", enabled: false };
  return { label: "Update", enabled: true };
}
