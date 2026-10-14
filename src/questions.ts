// Picking options on a question card (roadmap c2). Picks are option
// numbers per question; only numbers ever leave the page.

/** One question as the backend shows it (text already made visible-safe). */
export type Question = {
  question: string;
  header: string | null;
  multi: boolean;
  options: { label: string; description: string | null }[];
};

/** A press on option `n`: a single-select question picks just it (a second
 * press clears it); a multi-select one adds or removes it. Always sorted. */
export function toggle(chosen: number[], n: number, multi: boolean): number[] {
  if (chosen.includes(n)) return chosen.filter((c) => c !== n);
  return multi ? [...chosen, n].sort((a, b) => a - b) : [n];
}

/** Every question has a pick: only then can the answer be sent (Claude Code
 * would accept a partial answer without a word). */
export function ready(chosen: number[][], count: number): boolean {
  return chosen.length === count && chosen.every((c) => c.length > 0);
}
