// Bouncer, drawn in code: the SVG from design/prototype ("Meet Bouncer"),
// built element by element (never parsed from a string). Its state is a
// class on the wrapper; the animations live in styles.css.

export type BallState = "idle" | "working" | "needs" | "risky" | "paused";

const NS = "http://www.w3.org/2000/svg";

type Part = [tag: string, attrs: Record<string, string>, text?: string];

const JUMPER: Part[] = [
  ["circle", { class: "body-c", cx: "16", cy: "17", r: "12" }],
  ["path", { class: "deep", d: "M5.2 20.5a12 12 0 0 0 21.6 0a14 14 0 0 1-21.6 0z" }],
  ["ellipse", { class: "shine", cx: "10.5", cy: "10.8", rx: "3.2", ry: "1.9", transform: "rotate(-35 10.5 10.8)" }],
  ["rect", { class: "shades", x: "7.2", y: "13", width: "17.6", height: "5.2", rx: "2.6" }],
  ["rect", { class: "glint", x: "9.4", y: "14.2", width: "3.4", height: "1.1", rx: ".55" }],
  ["path", { class: "ink smile", d: "M12.6 22.2q3.4 2.6 6.8 0" }],
  ["path", { class: "ink flat", d: "M12.8 23h6.4" }],
  ["path", { class: "ink sleep", d: "M9.5 16.2q2.4 1.6 4.8 0M17.7 16.2q2.4 1.6 4.8 0" }],
];

function part([tag, attrs, text]: Part): SVGElement {
  const node = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  if (text !== undefined) node.textContent = text;
  return node;
}

function group(cls: string, parts: Part[]): SVGElement {
  const g = part(["g", { class: cls }]);
  g.append(...parts.map(part));
  return g;
}

/** The character in a state; `big` is the 72px detail-rail size. */
export function ball(state: BallState, big = false): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = `ball ${state}${big ? " big" : ""}`;
  const svg = part(["svg", { viewBox: "0 0 32 32", "aria-hidden": "true" }]);
  svg.append(
    part(["ellipse", { class: "shadow", cx: "16", cy: "30", rx: "8", ry: "1.6" }]),
    group("jumper", JUMPER),
    part(["text", { class: "zz", x: "25", y: "7" }, "z"]),
    group("alert", [
      ["circle", { cx: "27", cy: "5", r: "4.2" }],
      ["text", { x: "25.9", y: "7.6" }, "!"],
    ]),
  );
  wrap.append(svg);
  return wrap;
}
