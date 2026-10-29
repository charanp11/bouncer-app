# Bouncer: project brief

Last updated: v0.12.0 (draft), 2026-10-29

This is the short, plain-English picture of Bouncer as it is now. The
detailed logs are `docs/PLAN.md` (every phase, audit and hand check) and
`docs/TESTS.md` (every tested behaviour). When this brief and those disagree,
fix this brief.

## 1. What Bouncer is

Bouncer is a free, open-source desktop companion for coding agents (Claude
Code, the Codex CLI, GitHub Copilot CLI and Cursor CLI today) on Windows. A small "island" at the top of the
screen shows every agent session live. When an agent asks for permission
(run a command, edit a file), Bouncer shows a card with the whole request
and Allow / Deny buttons; rules you write can answer safe requests for you;
risky requests are flagged with a plain reason and never auto-allowed. It
can also answer Claude Code's questions and plans, explain a command, chat
read-only, and check a dropped file before you open it. If Bouncer is
closed, slow or confused, the agent simply asks in its own terminal prompt:
Bouncer never blocks the agent and never allows anything by default.

## 2. How it works

- **Hooks.** Claude Code and the other agents can run a small program at set moments
  ("a session started", "I want to run this command"). Bouncer installs
  entries in their settings files that run its **relay**.
- **Relay** (`bouncer-hook.exe`). A tiny program the agent starts for each
  event. It reads the event, passes it to the app, and, for a permission
  request, waits for the answer and prints it back to the agent. Hard time
  limits: 2 seconds for ordinary events, 110 seconds for a decision. Any
  error, timeout or odd input: it prints nothing and exits, so the agent
  asks as usual.
- **Pipe.** The relay and the app talk over a Windows named pipe that only
  your Windows user can open (checked on both ends). Debug and release
  builds use different pipes.
- **App** (Tauri: a Rust backend and a web page in WebView2). The backend
  holds the sessions, checks requests against your rules and the risk
  checks, issues a one-time random ID for each request, and writes the
  redacted log. The page draws the island, the Bouncer character, cards and
  Settings.
- **Cards.** A permission request becomes a card: the full command (hidden
  characters made visible), edits as a diff, a red reason if risky. Allow
  only works 600 ms after the card appears, the keyboard starts on Deny, and
  Enter never approves. A card waits 100 seconds, then goes, and the agent
  asks in its own prompt.

## 3. Features by agent

| | Claude Code | Codex CLI (v0.10.0) | GitHub Copilot CLI (v0.11.0) | Cursor CLI (v0.12.0) |
| --- | --- | --- | --- | --- |
| Sessions in the island | Yes | Yes, labelled Codex | Yes, labelled Copilot | Yes, labelled Cursor |
| Allow / Deny cards | Yes, when Claude Code asks | Yes, when Codex asks | No Allow ever; Deny on risky commands (guard on) | No Allow ever; Deny on risky shell commands (guard on) |
| Rules auto-allow | Yes (auto mode) | Yes | No: Copilot's own rules decide | No: Cursor's own rules decide |
| Risky flags | Yes | Yes (PowerShell rules on Windows) | Yes, on the guard's card | Yes, on the guard's card (shell commands only) |
| "Asking in the terminal" | Yes | Yes | Yes (watch-only) | No (Cursor's hooks don't say) |
| Questions and plans in the island | Yes | No | No (watch-only "question in the terminal") | No |
| Guard every mode | Yes (opt-in) | No | Yes (opt-in), Deny / "Let Copilot decide" only | Yes (opt-in), Deny / "Let Cursor decide" only |
| Explain this command | Yes | Yes | Yes, on the guard's card | Yes, on the guard's card |

Why the differences:

- **Claude Code** has the richest hooks: an "ask" answer that brings back its
  prompt in any mode (the guard) and question / plan events Bouncer can
  answer.
- **Codex** asks through a hook that takes only allow or deny; anything else
  "fails closed" (would block), so Bouncer sends nothing else. It has no
  hook for questions and its "ask" isn't supported. Needs Codex 0.145+.
  Codex strips the hook's environment and trusts each hook once in
  `/hooks`. The card shows Codex's own reason as "Codex says …", never as
  Bouncer's verdict.
- **Copilot** (v0.11.0, roadmap h2): its
  hook runs for every request before Copilot's own rules, can't say whether
  Copilot would ask, and any `allow` or `ask` from a hook overrides the deny
  rules you set in Copilot. Its hooks also block the tool if they crash or
  print rubbish. So Bouncer only ever says Deny or nothing: watch-only, plus
  a Deny / "Let Copilot decide" card for risky requests when Guard every
  mode is on (unanswered, it goes after 100 s and Copilot decides). Every
  hook command ends in "exit 0", so a missing Bouncer never blocks Copilot.
  Bouncer installs its own file, `hooks/bouncer.json` in Copilot's folder.
  Needs Copilot CLI 1.0.67 or newer (1.0.66 treats a slow hook as a
  refusal). Nothing is visible inside Copilot's sandbox (`--sandbox`): its
  hooks can't reach Bouncer there.
- **Cursor** (v0.12.0, roadmap h3): its shell hook runs before every
  shell command, whether or not Cursor would ask, so Bouncer gets what
  Copilot gets: Deny or nothing, with a Deny / "Let Cursor decide" card for
  risky shell commands when Guard every mode is on. A hook `allow` doesn't
  beat Cursor's own deny list, but would skip its prompt, so Bouncer never
  sends one. On Windows Cursor reads a hook's answer only when the hook's
  shell exits, and blocks a command on output that isn't valid JSON: every
  hook command ends in "exit 0" and the relay prints one valid deny or
  nothing, so a closed, missing, crashed or slow Bouncer never blocks Cursor
  (all proven with the real Cursor CLI). Bouncer adds its entries to your
  own `~/.cursor/hooks.json` and leaves your hooks alone. Each shell command
  costs about 0.75 s more on Windows. Tested with Cursor CLI 2026.10.22 on a
  free account in a test profile, Privacy Mode on.

Where Claude Code runs: the terminal, the VS Code extension and the Claude
desktop app's Code tab all work; Claude Code on the web can't (its hooks run
in the cloud).

## 4. Safety model

**How risk is decided.** Bouncer parses shell commands (Bash, and PowerShell
for Codex, Copilot and Cursor on Windows) with its own parser. A request is flagged risky when
it: downloads a script and runs it; builds part of the command while it
runs; hides a command inside a string; deletes outside the project; touches
secrets (.env, keys, credentials); force-pushes; runs as administrator;
touches a shell startup file, Claude Code's settings, Bouncer's own rules
or git's config / hooks; uses look-alike letters; or hides part of the
command. A rule can allow a request only if Bouncer understands all of it:
every part parsed and matched, no redirect to a file, every path inside the
project, no risk. Compound or unparseable commands always ask.

**Fails open (to the agent's own prompt), never to "allow".** Bouncer
closed, paused, slow, crashed, a timeout, unknown input: the relay prints
nothing and the agent asks in its own prompt. No code path allows by
default. For agents whose hooks block on a hook failure (Copilot), the hook
command is wrapped so a failure still means "nothing".

**Answers are bound.** Each card carries a random, single-use ID from the
backend; an answer for anything else is ignored. Only your click answers.

**Text is never HTML.** Commands, paths and file names from an agent are
shown as plain text; strict content security policy; the page can call only
Bouncer's own commands.

**What Bouncer never touches.**
- Agent settings are never written silently: a dated backup, only Bouncer's
  entries, the exact diff shown, your yes, an atomic write. Uninstall removes
  only Bouncer's entries.
- No telemetry. The only network use is the daily update check (off
  switch in Settings; development builds never check). Chat and Explain go
  through your own Claude Code login.
- No tool outputs or file contents are stored; the log is redacted before
  it is written, kept 30 days, on this computer only.
- The read-only chat can only read inside its own empty folder: no edits,
  commands, web, MCP servers or connectors.
- A dropped file is only read: never run, opened or unpacked; nothing is
  sent before you confirm.

Out of scope: malware already running as you (it could edit the agents'
settings itself).

## 5. Settings

In the order they appear:

- **Size**: 100 / 112 / 125%; applies at once.
- **Sound**: all sounds on or off; never while paused.
- **Sound style**: Soft or Playful; "needs you" and "risky" always stand
  out.
- **Volume**.
- **Sounds**: each sound on or off.
- **Auto-allow**: off = observe (cards say what rules would do); on = rules
  answer for you, risky still asks. Turning it on asks first.
- **Guard every mode**: off by default. On: Claude Code's risky requests in
  auto, accept-edits and bypass modes stop for a card (in a `claude -p` run
  they're refused); Copilot's risky requests get a Deny / "Let Copilot
  decide" card in any mode.
- **Activity history**: 30 days, local, redacted; "Delete history" (armed,
  with a confirm) is the only way to wipe it.
- **Claude Code for chat**: shows which `claude.exe` the chat and Explain
  run; you confirm the path once, and again if it changes.
- **Rules file**: where `rules.toml` lives (`%APPDATA%\Bouncer`).
- **Claude Code hooks**: status, and Install… / Remove… with the diff.
  Hooks written by a Bouncer before v0.11.1 show "Update needed": they
  work, and Re-install… adds `--agent claude-code` (shown in the diff first).
- **Codex hooks**: shown when a Codex folder exists; same as above; needs
  Codex 0.145+.
- **Copilot CLI hooks**: shown when a Copilot folder exists (`~/.copilot`
  or `$COPILOT_HOME`); writes Bouncer's own `hooks/bouncer.json` there with
  the diff; says what Copilot gets (watch-only plus Deny on risky commands,
  no Allow from Bouncer, nothing inside its sandbox); needs Copilot CLI
  1.0.67+.
- **Cursor CLI hooks**: shown when a `~/.cursor` folder exists; adds
  Bouncer's entries to Cursor's `~/.cursor/hooks.json` with the diff (your
  own hooks there stay); says what Cursor gets (watch-only plus Deny on
  risky shell commands, no Allow from Bouncer); tested with Cursor CLI
  2026.10.22.
- **Start with Windows**: off by default.
- **Shortcut Ctrl+Alt+B**: opens Bouncer from anywhere; never approves.
- **Check for updates**: on by default; off = no network at all.

## 6. Releases and updates

- **A tag is a release.** Pushing a tag like `v0.10.1` starts the release
  workflow: it runs the checks and the installer test, sets the version from
  the tag, builds the per-user installer, signs it for the updater, writes
  `SHA256SUMS.txt`, adds a build attestation, writes `latest.json` and makes
  a **draft** release. Charan reviews and presses Publish. Old `v0.5.x` /
  `v0.6.x` tags were internal and never trigger it.
- **The updater.** Once a day (first a minute after start) Bouncer reads the
  latest release's `latest.json`. If it's newer, the island shows "Bouncer
  X.Y.Z is ready" with **Update**; it installs only on your click, never
  while a card, question, chat or explanation is waiting. It checks the
  installer's signature against the public key built into Bouncer before
  running it. Anything wrong: nothing installs, it tries tomorrow.
- **The signing key.** The updater key pair: the private key is held by
  Charan and stored as the repository secrets `TAURI_SIGNING_PRIVATE_KEY`
  and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the public key is in the app's
  config (`tauri.conf.json`). The installer itself isn't
  code-signed yet (SmartScreen warns; SignPath Foundation planned).
- **Every release's hand checks:** the update from the previous version
  through the updater, the version in Settings, plus TESTS "Phase 9 hand
  checks".

## 7. Map of the code

- `crates/relay/`: the `bouncer-hook` relay (std + serde_json only).
  - `src/main.rs`: reads one event, talks to the app, prints the answer
    or nothing; the `--agent claude-code`, `--agent codex`, `--agent
    copilot --event <name>` and `--agent cursor --event <name>` flags
    (Copilot's and Cursor's input becomes the same line; deny only). No
    flag is an older Claude Code install.
  - `src/lib.rs`: shared pipe / socket naming and limits.
  - `src/win.rs`, `src/unix.rs`: the named pipe (Windows) and Unix socket
    (macOS), with same-user checks.
  - `src/questions.rs`: Claude Code question / plan answers.
  - `tests/`: fail-safe and end-to-end relay tests, recorded fixtures
    (Claude Code's, `codex-fixtures/`, `copilot-fixtures/`,
    `cursor-fixtures/`).
- `crates/core/`: everything that isn't UI.
  - `event.rs`: the events the app understands.
  - `ipc.rs`: the app's side of the pipe.
  - `approvals.rs`: waiting requests, single-use IDs, hidden characters.
  - `rules.rs`, `check.rs`, `shell.rs`: `rules.toml`, risk checks, the shell
    parser.
  - `activity.rs`, `redact.rs`, `away.rs`: the redacted log and the
    welcome-back summary.
  - `hooks.rs`: install / remove agent hooks (rule 6), the real-profile test
    guard.
  - `chat.rs`: the locked read-only chat and Explain.
  - `drop.rs`, `drop/`: file-drop checks.
  - `prefs.rs`, `autostart.rs`, `update.rs`, `code.rs`, `diff.rs`:
    settings, Start with Windows, update checks, code / diff display.
  - `src/bin/bouncer.rs`: the `bouncer` command line (install-hooks,
    uninstall-hooks).
  - `examples/fake_claude.rs`: a fake `claude` for tests.
- `crates/app/`: the Tauri app.
  - `src/main.rs`: windows, tray, commands the page may call.
  - `src/region.rs`: native window code (the only place for it).
  - `src/chat.rs`, `src/drop.rs`, `src/update.rs`: chat runs, file drops,
    the updater.
  - `installer-hooks.nsh`: the uninstaller's hooks step.
  - `tests/no_html_sinks.rs`: proves the page never builds HTML from text.
- `src/`: the page (plain TypeScript, no framework). `main.ts` draws the
  island, cards and Settings; `ball.ts`, `mood.ts`, `motion.ts`,
  `morph.ts` the character and motion; `sound.ts`, `pack.ts` sounds;
  `questions.ts`, `chat.ts`, `markdown.ts`, `drop.ts`, `update.ts` their
  panels; `arm.ts`, `press.ts` the click rules; `*.test.ts` its tests.
- `design/prototype/bouncer-island.html`: the visual source of truth.
- `scripts/`: `check-tests.mjs` (the regression gate), release helpers
  (`version.mjs`, `release-notes.mjs`, `latest-json.mjs`, `bundle.mjs`),
  `installer-test.ps1`.
- `.github/workflows/`: `ci.yml` (Windows + macOS checks, cargo deny,
  npm audit, gitleaks), `installer.yml` (install / upgrade / uninstall on a
  clean runner), `macos-smoke.yml`, `release.yml`, `advisories.yml`.
- `docs/`: this brief, `PLAN.md`, `TESTS.md`, `COMPARE.md` (vs Coucou),
  `releases/` (each release's notes).

## 8. Version history

Newest first. Before v0.7.0, tags marked finished phases and were never
published.

- **v0.12.0** (2026-10-29, draft): Cursor CLI support: watch-only plus Deny
  on risky shell commands (Guard every mode), no Allow from Bouncer, your
  own Cursor hooks kept; tested with Cursor CLI 2026.10.22. Privacy fix:
  Cursor's input sent through Claude Code's hooks is ignored (it carried
  your email).
- **v0.11.1** (2026-10-28, published 2026-10-28): hardening: Claude Code's hooks name
  themselves (`--agent claude-code`); older hooks keep working and
  Settings shows "Update needed" with Re-install.
- **v0.11.0** (2026-10-28, published 2026-10-28): GitHub Copilot CLI support: watch-only plus
  Deny on risky commands (Guard every mode), no Allow from Bouncer, nothing
  inside Copilot's sandbox; needs Copilot CLI 1.0.67+.
- **v0.10.1** (2026-10-21, published 2026-10-22): every Settings hint
  wraps (the Codex hint was cut).
- **v0.10.0** (2026-10-21): Codex CLI support (cards, rules, risk checks,
  hooks install with diff); needs Codex 0.145+.
- **v0.9.0** (2026-10-17): drop a file on the island to check it: real type,
  name tricks, risky script lines, zip contents, signature, SHA-256.
- **v0.8.3** (2026-10-16): every click counts; version shown in Settings.
- **v0.8.2** (2026-10-16): first clicks always count.
- **v0.8.1** (2026-10-15): questions and plans in the island, Guard every
  mode, the updater. (v0.8.0 was never published: a Settings freeze.)
- **v0.7.0** (2026-10-14): first public Windows release: per-user
  installer, clean uninstall, checksums and attestations, missing-hooks
  warning, Ctrl+Alt+B, Start with Windows.
- **v0.6.4**: Explain this command. **v0.6.3**: Markdown in chat answers.
  **v0.6.0–0.6.2**: the read-only chat.
- **v0.5.x**: the Bouncer character, island motion and sound pack.
- **v0.4.0**: the redacted activity log and welcome-back summary.
- **v0.3.0**: rules and risk checks, "Always allow…".
- **v0.2.0**: the island and approval cards.
- **v0.1.0**: the relay, the pipe and the fail-safe.

## 9. Decisions

- **Rust + Tauri, plain TypeScript, no UI framework** (set at the start;
  changes need Charan's OK): a small, fast app with few dependencies; a
  framework would add supply chain for one small page. No new dependency
  without asking why the standard library isn't enough.
- **The relay uses only the standard library and serde_json**: it runs on
  every agent event, so it must start fast and have almost nothing to go
  wrong.
- **Fail to the agent's own prompt, never to allow**: being wrong must cost
  a prompt, never an unwanted action.
- **Risky is flagged, never auto-denied**: Bouncer informs, you decide.
- **Rules in TOML, log as JSON Lines day files**: readable by hand; SQLite
  was dropped in Phase 4 (a dependency for no gain).
- **No telemetry; one opt-out update check**: privacy first; manual
  installs only (0.7.0) left users behind, so the updater came in 0.8.1,
  click-to-install only.
- **One feature per release**, each audited first, its own PR, Charan's OK
  to merge; a tag means a release.
- **Agents one per release, told apart by an explicit flag**
  (`--agent codex`, `--agent copilot`, `--agent cursor`, and since v0.11.1
  `--agent claude-code`): the relay never guesses from the input's shape.
  Older Claude Code installs have no flag; for them only, input with a
  `timestamp` (Copilot's) or a `cursor_version`, `user_email` or
  `workspace_roots` (Cursor's) is refused, so a field Claude Code may add
  one day can't hide a flagged install.
  Why Cursor (Charan, 2026-10-29, a privacy fix): Cursor's CLI also runs
  the hooks in `~/.claude/settings.json` (when any of them has a matcher),
  drops their arguments so Bouncer's entry runs with no flag, and sends its
  own input with your email address. Bouncer now ignores that input: it
  never reaches the app, nothing is shown, Cursor goes on as usual.
  Codex went first: its hook answers like Claude Code's and fails open.
- **Copilot: Deny only** (Charan, 2026-10-28, after step 1): its hooks can
  override its own deny rules and block on failure; anything more would let
  Bouncer weaken Copilot's own safety. Rejected: Allow cards and rules
  (an allow beats Copilot's deny rules), the guard through `preToolUse`
  "ask" (a Yes beats them too, and autopilot turns it into a refusal), any
  `preToolUse` hook (it refuses the tool when the hook fails), and cards
  for every request (the hook can't tell whether Copilot would have asked).
- **Cursor: Deny only too** (Charan, 2026-10-29, after step 1): its shell
  hook runs for every command and can't say whether Cursor would ask; an
  `allow` doesn't beat Cursor's deny list but would skip its prompt. Picked
  over Antigravity CLI, whose hooks block the tool when they fail and have
  no "say nothing" answer. Shipping it needed proof first that empty
  output, a closed Bouncer, a missing or crashed relay and a hook timeout
  all let Cursor carry on (they do, with "exit 0").
- **Character and sounds drawn in code**: nothing copied from Coucou.
- **Repo name `bouncer-app`**: "bouncer" was taken on crates.io and npm.

## 10. Known limits and open questions

- Bouncer can only answer when the agent asks: in Claude Code's bypass and
  auto modes that's rarely, unless Guard every mode is on.
- Some questions stay in the terminal (typed answers, ambiguous options,
  while paused); approving a plan keeps the earlier permission mode.
- Codex: allow / deny only. Copilot: watch-only plus Deny; nothing inside
  its sandbox; rules never answer for it. Both tested with the real CLI and
  a local test model, never a live one (no subscriptions).
- Cursor: watch-only plus Deny, shell commands only (no file edits, reads
  or MCP tools as requests); rules never answer for it. Tested with the
  real Cursor CLI and a live model on a free account in a test profile.
  Antigravity CLI isn't supported: its hooks block the tool when they fail.
- Not code-signed: SmartScreen warns; Smart App Control blocks it.
- No Mac release (the code is checked on macOS in CI); no Linux.
- Explain and chat answers can be wrong; Explain costs CPU while it streams.
- Open: Dependabot #57 (the updater plugin leaves its pinned 2.13.3) waits
  for a release with a full updater test (installed 0.10.1 → new version →
  the version after).
- Open: the Narrator / VoiceOver checks (with the Mac release).

## 11. Roadmap

In order, one per release, each audited first:

1. **More agents (h)**, one per release, each audited for whether its hooks
   can block a tool call (yes: approvals; no: watch-and-flag, labelled).
   Codex, Copilot and Cursor are in; Antigravity was audited and left out
   (its hooks fail closed).
2. **f: Ask about this session**: a read-only question about a running
   session.
3. **g: Mac release** (needs a real Mac).
4. **e: Music**, within the motion and CPU budget.
5. **i: Linux** (optional).
