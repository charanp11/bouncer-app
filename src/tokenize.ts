// Syntax colors for the code pane: a small per-line tokenizer that returns
// plain {cls, text} pieces, which the page turns into text nodes. It never
// produces HTML. Unknown languages come back as one plain piece.

export type Token = { cls: "" | "k" | "s" | "num" | "c" | "ty"; text: string };

const KEYWORDS: Record<string, string> = {
  rust: "as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait type unsafe use where while true false",
  ts: "as async await break case catch class const continue default delete do else enum export extends finally for from function if import in instanceof interface let new of return static switch this throw try type typeof var void while yield true false null undefined",
  python: "and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield True False None",
  go: "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var true false nil",
  json: "true false null",
  shell: "if then else elif fi for while do done case esac in function return export local",
};

const COMMENT: Record<string, string> = {
  rust: "//.*$",
  ts: "//.*$",
  go: "//.*$",
  python: "#.*$",
  shell: "(?:^|(?<=\\s))#.*$",
  json: "",
};

const STRING: Record<string, string> = {
  // Rust: "..." and char literals only; a lone ' is a lifetime.
  rust: `"(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)'`,
  ts: `"(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)*'|\`(?:[^\`\\\\]|\\\\.)*\``,
  python: `"(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)*'`,
  go: `"(?:[^"\\\\]|\\\\.)*"|\`[^\`]*\``,
  json: `"(?:[^"\\\\]|\\\\.)*"`,
  shell: `"(?:[^"\\\\]|\\\\.)*"|'[^']*'`,
};

const PATTERNS = new Map<string, RegExp>();

function pattern(syntax: string): RegExp | undefined {
  if (!(syntax in KEYWORDS)) return undefined;
  let re = PATTERNS.get(syntax);
  if (!re) {
    const parts = [
      COMMENT[syntax] && `(?<c>${COMMENT[syntax]})`,
      `(?<s>${STRING[syntax]})`,
      `\\b(?<k>${KEYWORDS[syntax].split(" ").join("|")})\\b`,
      syntax === "python" || syntax === "shell" || syntax === "json" ? "" : `\\b(?<ty>[A-Z][A-Za-z0-9_]*)\\b`,
      `\\b(?<num>\\d[\\d_]*(?:\\.\\d+)?)\\b`,
    ].filter(Boolean);
    re = new RegExp(parts.join("|"), "g");
    PATTERNS.set(syntax, re);
  }
  return re;
}

export function tokenize(line: string, syntax: string): Token[] {
  const re = pattern(syntax);
  if (!re) return line ? [{ cls: "", text: line }] : [];
  const out: Token[] = [];
  let last = 0;
  re.lastIndex = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m[0] === "") {
      re.lastIndex++;
      continue;
    }
    if (m.index > last) out.push({ cls: "", text: line.slice(last, m.index) });
    const g = m.groups ?? {};
    const cls: Token["cls"] = g.c ? "c" : g.s ? "s" : g.k ? "k" : g.ty ? "ty" : "num";
    out.push({ cls, text: m[0] });
    last = m.index + m[0].length;
  }
  if (last < line.length) out.push({ cls: "", text: line.slice(last) });
  return out;
}
