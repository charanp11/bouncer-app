// Markdown in chat answers, read into plain blocks and pieces of text that
// main.ts turns into elements filled with textContent. Nothing here is ever
// HTML: a tag in an answer is just text, and a link is its words with the
// address after them, never something to click.
//
// ponytail: the common subset only (paragraphs, headings, lists, quotes,
// fenced code, rules; bold, italic, inline code, links as text). Nested lists
// come out flat, tables as their text; add them if answers need them.

export type Piece = { kind: "text" | "bold" | "italic" | "code"; text: string };

export type Block =
  | { kind: "p" | "quote"; pieces: Piece[] }
  | { kind: "heading"; level: 1 | 2 | 3; pieces: Piece[] }
  | { kind: "list"; ordered: boolean; start: number; items: Piece[][] }
  | { kind: "code"; syntax: string; text: string }
  | { kind: "rule" };

/** Fence languages → the code pane's tokenizer (`tokenize.ts`). */
const SYNTAX: Record<string, string> = {
  rs: "rust", rust: "rust",
  ts: "ts", tsx: "ts", typescript: "ts", js: "ts", jsx: "ts", javascript: "ts", mjs: "ts",
  py: "python", python: "python",
  go: "go", golang: "go",
  json: "json", jsonc: "json",
  sh: "shell", bash: "shell", shell: "shell", zsh: "shell", console: "shell", powershell: "shell", ps1: "shell", pwsh: "shell",
};

const FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([\w+-]*)/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;
const NUMBERED = /^\s*(\d{1,9})[.)]\s+(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;

/** The whole answer, read so far (an unclosed fence runs to the end, as
 * while it streams). */
export function markdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let para: string[] = [];
  let quote: string[] = [];
  let list: { ordered: boolean; start: number; items: string[] } | null = null;
  const flush = () => {
    if (para.length) blocks.push({ kind: "p", pieces: pieces(para.join("\n")) });
    if (quote.length) blocks.push({ kind: "quote", pieces: pieces(quote.join("\n")) });
    if (list) blocks.push({ kind: "list", ordered: list.ordered, start: list.start, items: list.items.map(pieces) });
    para = [];
    quote = [];
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = line.match(FENCE);
    if (fence) {
      flush();
      const close = new RegExp(`^\\s{0,3}${fence[1][0] === "`" ? "`" : "~"}{${fence[1].length},}\\s*$`);
      const code: string[] = [];
      while (++i < lines.length && !close.test(lines[i])) code.push(lines[i]);
      blocks.push({ kind: "code", syntax: SYNTAX[fence[2].toLowerCase()] ?? "", text: code.join("\n") });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = line.match(HEADING);
    if (heading) {
      flush();
      const level = Math.min(heading[1].length, 3) as 1 | 2 | 3;
      blocks.push({ kind: "heading", level, pieces: pieces(heading[2]) });
      continue;
    }
    if (RULE.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
      continue;
    }
    const q = line.match(QUOTE);
    if (q) {
      if (!quote.length) flush();
      quote.push(q[1]);
      continue;
    }
    const bullet = line.match(BULLET);
    const numbered = bullet ? null : line.match(NUMBERED);
    if (bullet || numbered) {
      const ordered = !!numbered;
      if (!list || list.ordered !== ordered) {
        flush();
        list = { ordered, start: numbered ? Number(numbered[1]) : 1, items: [] };
      }
      list.items.push(bullet ? bullet[1] : numbered![2]);
      continue;
    }
    if (list && /^\s{2,}\S/.test(line)) {
      // An indented line goes on with the item above.
      list.items[list.items.length - 1] += `\n${line.trim()}`;
      continue;
    }
    if (quote.length || list) flush();
    para.push(line);
  }
  flush();
  return blocks;
}

const INLINE = /(`+)([\s\S]*?[^`])\1(?!`)|\*\*(?=\S)([\s\S]*?\S)\*\*|__(?=\S)([\s\S]*?\S)__|\*(?=[^\s*])([\s\S]*?[^\s*])\*|(?<![\w])_(?=[^\s_])([\s\S]*?[^\s_])_(?![\w])|\[([^\]\n]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;

/** Bold, italic, inline code and links (as text) in one block's text. */
export function pieces(text: string): Piece[] {
  const out: Piece[] = [];
  const push = (kind: Piece["kind"], t: string) => {
    if (!t) return;
    const last = out.at(-1);
    if (last && last.kind === kind && kind === "text") last.text += t;
    else out.push({ kind, text: t });
  };
  let at = 0;
  for (const m of text.matchAll(INLINE)) {
    push("text", text.slice(at, m.index));
    if (m[1]) push("code", m[2].replace(/^ (?=\S)|(?<=\S) $/g, ""));
    else if (m[3] ?? m[4]) push("bold", m[3] ?? m[4]);
    else if (m[5] ?? m[6]) push("italic", m[5] ?? m[6]);
    else push("text", m[7] === m[8] ? m[7] : `${m[7]} (${m[8]})`);
    at = m.index + m[0].length;
  }
  push("text", text.slice(at));
  return out;
}
