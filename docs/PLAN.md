# Bouncer — MVP plan

Bouncer is a free, open-source desktop companion for Claude Code. It shows every
session live, auto-approves safe actions under rules you control, flags risky ones
with a plain reason, and never blocks the agent. Seven phases, about eight weeks, $0.

**Current phase: Phase 2 — Island window and manual approvals**

## MVP scope

| In the MVP | After the MVP |
| --- | --- |
| Live view of every Claude Code session (several at once) | A second agent (Codex CLI, Gemini CLI) |
| Approve or deny permission requests from the island | Approving from your phone |
| Rules engine: auto-allow safe actions, flag risky ones with a reason | Auto-update |
| Local activity log and a "while you were away" summary | Sitting exactly inside the MacBook notch |
| Bouncing-ball character and sounds, made in code | Chat, file drop, other integrations |

Events are normalized into one internal `Event` format from Phase 1, so a second
agent later is a new adapter, not a rewrite. The adapter abstraction is added only
when that second agent arrives.

## Architecture

```
Claude Code ──runs──▶ relay (bouncer-hook) ──user-only socket / pipe──▶ app core (Rust)
     ▲                                                                  ├─ rules engine (allow / ask + reason)
     │                                                                  ├─ activity log (local, redacted)
     │                                                                  ▼
     └──── allow / deny / nothing ◀──── relay ◀──── your click ──── island window
Fails safe: app missing or slow → relay prints nothing → Claude Code asks in the terminal.
```

## How every phase runs

1. **Audit** — read the docs and code the phase touches; write the threat list and
   "done when" criteria here before any code.
2. **Implement** — small commits, one concern each, in the order listed.
3. **Test** — unit tests, integration tests on recorded real hook payloads, a short
   manual script on a real machine.
4. **Verify** — security checklist, reread the whole diff, update docs, tag `v0.N.0`.

---

## Setup — toolchain on Windows

- [x] Check versions: `git --version`, `node -v`, `rustc --version`, `cargo --version`, `winget --version`
- [x] Install what's missing (Charan approves each Windows admin prompt):
  - `winget install --id Rustlang.Rustup -e`
  - `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
  - `winget install --id OpenJS.NodeJS.LTS -e`
  - `winget install --id Git.Git -e` (if git is missing)
- [x] Close and reopen the terminal / Claude Code so PATH updates, then re-check versions
- [x] `rustup default stable` and `rustup component add clippy rustfmt`
- [x] WebView2 present (it ships with Windows 10/11)
- [x] Git identity set in the repo: `git config user.name` / `git config user.email`
- [x] Smart App Control off (Windows Security → App & browser control). In enforce
  mode it blocks freshly compiled Rust build scripts, so Tauri can't build.

## Phase 0 — Repo and guardrails (~2 days)

- **Audit:** check "bouncer" isn't used by another developer tool on GitHub,
  crates.io and npm (if taken, keep "Bouncer" as the app name and use `bouncer-app`
  for package names); read Tauri 2's security guide (capabilities, CSP).
- **Implement:** cargo workspace (`relay`, `core`, `app`), Tauri 2 scaffold with
  plain TypeScript, MIT `LICENSE`, `SECURITY.md`, `CREDITS.md`, `.gitignore`, README stub.
- **Test:** CI on Windows + macOS runs `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test`, `cargo deny check`, `npm audit`, gitleaks; a weekly scheduled job
  runs `cargo deny check advisories`.
- **Verify:** Dependabot on; Actions pinned to commit SHAs; workflow
  `permissions: contents: read`; private vulnerability reporting on; branch
  protection on `main` (set after CI first runs): PR required (0 approvals), every
  CI job on both OSes a required check, branch up to date before merge, linear
  history, no force pushes or deletion, applies to admins. Signed commits: later.
- Commits: `chore: init workspace` → `chore: add tauri app shell` →
  `ci: add lint, test and audit workflow` → `docs: add license, security policy and readme stub`

### Phase 0 audit (2026-10-01)

Name: settled as `bouncer-app` (see Decisions). Versions today: `tauri` 2.12.1,
`tauri-build` 2.7.1, `@tauri-apps/cli` and `@tauri-apps/api` 2.12.1, Vite 8.3,
TypeScript 7.0, Rust 1.99.

What Tauri 2's security guide means for us:

- **Commands are open by default.** Every registered command can be called from
  every window unless `build.rs` lists them with
  `tauri_build::AppManifest::new().commands(&[...])` and a capability grants each
  one. Phase 0 registers no commands; Phase 2 must use the manifest from its first
  command.
- **Capabilities:** every file in `src-tauri/capabilities/` is enabled automatically.
  We keep exactly one file, bound to the `main` window, with no `remote` URLs.
- **The template is not secure as shipped.** `create-tauri-app` gives `"csp": null`,
  the `opener` plugin with `opener:default`, a sample `greet` command, a Vite dev
  server that listens on `TAURI_DEV_HOST`, and mobile-only crate types. All of that
  is removed.
- **CSP** goes in `app.security.csp`; Tauri adds hashes/nonces for bundled assets at
  build time. Ours: `default-src 'self'; script-src 'self'; style-src 'self';
  img-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none';
  base-uri 'none'; form-action 'none'; frame-ancestors 'none'`.
- Also set: `freezePrototype: true`. Leave as default: `withGlobalTauri: false`,
  `assetProtocol.enable: false`, no isolation pattern (we load no third-party
  frontend code). Devtools stay out of release builds (no `devtools` feature).
- Lifecycle: the dev server binds to localhost only; pin Actions to SHAs; audit both
  Rust and npm dependencies.

Phase 0 threats:

| Threat | Fix |
| --- | --- |
| Insecure template defaults ship (no CSP, opener plugin, sample command) | Strip them; CSP set; one minimal capability; reviewed in Verify |
| Any window can call any command | No commands in Phase 0; `AppManifest` allow-list from Phase 2 |
| Dev server reachable from the LAN | Vite `host` fixed to `localhost`, `strictPort` |
| Compromised or retagged GitHub Action | Every `uses:` pinned to a full commit SHA with a version comment; Dependabot updates them |
| CI token abused | Workflow-level `permissions: contents: read`; no secrets used |
| Vulnerable / unlicensed / non-crates.io dependency | `cargo deny check` (advisories, licenses, bans, sources: crates.io only); `npm audit`; lockfiles committed; `npm ci` in CI |
| Secret committed | gitleaks in CI on full history; GitHub secret scanning and push protection (already on) |
| Direct push or force push breaks `main` | Branch protection: PR + required checks, linear history, no force push/delete, admins included |
| New advisory lands between pushes | Weekly scheduled `cargo deny check advisories` |
| Vulnerability reported in public | `SECURITY.md` + private vulnerability reporting |
| Line-ending drift between Windows and macOS (fmt check, diffs) | `.gitattributes` with `* text=auto eol=lf` (Git for Windows sets `core.autocrlf=true` system-wide) |

Phase 0 is done when:

- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
      pass locally on Windows and in CI on `windows-latest` and `macos-latest`
- [x] Crates `bouncer-relay`, `bouncer-core`, `bouncer-app`; `relay` depends only on
      std + `serde_json`
- [x] `npm run tauri dev` opens an empty Bouncer window on Windows
- [x] `tauri.conf.json` has the CSP above and `freezePrototype: true`; no plugins; no
      commands; one capability file for `main` with the fewest core permissions that
      work; no `remote`
- [x] `cargo deny check`, `npm audit` and gitleaks pass in CI; the weekly
      advisories job exists
- [x] Every Action pinned to a SHA; workflow `permissions: contents: read`
- [x] Dependabot (cargo, npm, github-actions) and private vulnerability reporting
      are on; branch protection on `main` as listed under Verify
- [x] `LICENSE` (MIT), `SECURITY.md`, `CREDITS.md`, `README.md`, `.gitignore`,
      `.gitattributes` exist

Plan corrections found in this audit:

1. `cargo deny check` already checks the RustSec advisory database, which is what
   `cargo audit` does. Decision: drop `cargo audit`; add a weekly scheduled
   `cargo deny check advisories` job.
2. The plan didn't cover npm dependencies. Added `npm audit` and npm in Dependabot.
3. CI must build the frontend (`npm ci && npm run build`) before clippy and tests,
   because `tauri::generate_context!` fails to compile when `frontendDist` is missing.
4. Branch protection that requires status checks also blocks direct pushes to
   `main`. Decision: once protection is on, every change goes through a PR.
5. `.gitattributes` was missing from the file list.

## Phase 1 — Hook relay and local socket (~1 week)

- **Audit:**
  - Read https://code.claude.com/docs/en/hooks and record the exact stdin fields and
    exact allow/deny output for `PermissionRequest` and `PreToolUse`. Save real
    payloads from a playground session as test fixtures.
  - Confirm which events fire for actions Claude Code already allows by its own
    permission rules.
  - Pick the lowest supported Claude Code version (needs every hook we use) from the
    changelog; Bouncer tells users on older versions to update.
  - Study Coucou's relay deadlines and peer checks (github.com/Louis-CFM/coucou,
    `windows/hook/src/main.rs`).
- **Implement:**
  - `relay` binary: stdin capped at 1 MB, drop large fields (`tool_response`),
    connect within 300 ms, fire-and-forget within 2 s, permission requests wait up to 110 s.
  - Unix socket in a per-user `0700` folder (macOS); named pipe restricted to the
    user's SID (Windows); both ends verify same user.
  - Normalized `Event` struct (agent, session, project, tool, input, time).
  - `bouncer install-hooks` / `uninstall-hooks`: dated backup, merge only our
    entries, show diff, confirm, atomic write.
- **Test:** fixtures parse; app closed → exit < 50 ms with empty output; app hung →
  gives up at deadline; malformed or 5 MB input → clean exit; two sessions stream;
  install + uninstall leaves `settings.json` byte-identical.
- **Verify:** no default "allow"; another user cannot connect; no unconfirmed writes.
- Commits: `feat(relay): read and forward hook events` → `feat(core): normalized event model` →
  `feat(app): local socket server with peer check` → `feat(relay): wait for permission decisions with deadline` →
  `feat(cli): safe hook install and uninstall` → `test(relay): fixtures, timeouts and malformed input`

### Phase 1 audit (2026-10-01)

Sources: code.claude.com/docs/en/hooks, the Claude Code `CHANGELOG.md`, Coucou's
`windows/hook/src/{main,win}.rs`, and payloads recorded in `../bouncer-playground`
(its own `.claude/settings.local.json`; user settings untouched).

What Claude Code sends and accepts:

- Every hook gets JSON on stdin with `session_id`, `transcript_path`, `cwd`,
  `hook_event_name`, usually `permission_mode`, plus per-event fields.
  `PreToolUse` / `PermissionRequest` add `tool_name`, `tool_input`, `tool_use_id`
  (`PermissionRequest` may add `permission_suggestions`); `PostToolUse` adds
  `tool_response` (can be a whole file); `UserPromptSubmit` adds `prompt`.
- Exit 0 + empty stdout = "no decision", Claude Code carries on normally. Exit 2
  blocks on `PreToolUse` (we never use it); other non-zero codes are logged errors.
- `PermissionRequest` allow:
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`;
  deny: same with `{"behavior":"deny","message":"..."}`. We send no `updatedInput`
  and no permission updates.
- `PreToolUse` fires before every tool call, including ones Claude Code's own rules
  already allow; `PermissionRequest` fires only when Claude Code would show a
  dialog. So: Bouncer observes everything via `PreToolUse` (prints nothing) and
  answers only `PermissionRequest`.
- Hooks for one event run in parallel; the default command timeout is 600 s
  (since 2.1.3). We set `timeout` explicitly anyway: 120 s for
  `PermissionRequest` (above our 110 s wait), 5 s for the rest.
- Exec form (`"command": <path>, "args": []`, since 2.1.139) spawns the relay
  without a shell, so paths with spaces ("Full Time") and shell differences
  (Git Bash vs PowerShell on Windows) don't matter.
- Hooked events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, `PermissionRequest`, `Notification`, `Stop`.

Lowest supported Claude Code version: **2.1.139** — `PermissionRequest` exists
since 2.0.45, the fix stopping hook "allow" from bypassing `deny` rules landed in
2.1.77, and exec-form `args` arrived in 2.1.139.

From Coucou (studied, not copied): 300 ms connect / 2 s fire-and-forget / 110 s
decision budgets enforced by the main thread with `recv_timeout` (a worker does the
blocking I/O and is abandoned on overrun); SID in the pipe name; relay checks the
pipe server's SID via `GetNamedPipeServerProcessId`; only exact, known answers
print anything. Differences: Coucou truncates long strings, which could show a
user a cut-off command to approve — Bouncer forwards inputs whole and drops the
event instead if stdin exceeds 1 MB; Bouncer also restricts the pipe's DACL and
checks the client on the server side.

Transport:

- Windows: `\\.\pipe\bouncer-<SID>`, created with DACL `D:P(A;;GA;;;<SID>)`,
  `FILE_FLAG_FIRST_PIPE_INSTANCE` (fails if someone squatted the name) and
  `PIPE_REJECT_REMOTE_CLIENTS`. Server checks the client process's SID, relay checks
  the server's. Win32 via `windows-sys` (decision below).
- macOS: `~/Library/Application Support/Bouncer/bouncer.sock` (was
  `$TMPDIR/bouncer-<uid>/`, changed after review: `$TMPDIR` can differ between
  the GUI app and a terminal); the app creates the folder `0700`
  and refuses to use one that is a symlink, not ours, or open to others. Both ends
  compare `getpeereid` with `getuid` (two hand-declared libc functions).
- Wire: the relay sends one JSON line; for `PermissionRequest` the app answers one
  line, exactly `allow` or `deny`. Anything else, EOF or timeout → nothing printed.
- The server lives in `bouncer-core` (`ipc` module) so relay integration tests can
  run a real server; the app only starts it. `BOUNCER_ENDPOINT` overrides the
  path in debug/test builds only (peer checks still apply).
- `bouncer install-hooks` / `uninstall-hooks` is a small `bouncer` binary in
  `bouncer-core` (the app is a GUI-subsystem exe on Windows and can't prompt in a
  console). It targets `~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR`) unless
  `--settings <file>` is given. Our entries = hook commands whose file name is
  `bouncer-hook[.exe]`.

Phase 1 threats:

| Threat | Fix |
| --- | --- |
| Relay prints "allow" by mistake | Only an exact `allow` line from a verified app, only for `PermissionRequest`; every other path prints nothing; tests assert empty stdout |
| App missing / slow / hung blocks Claude Code | 300 ms connect, 2 s fire-and-forget, 110 s decision; main thread never blocks on I/O; exit 0 always |
| Another user answers or reads events | User-only DACL / `0700` folder; SID / uid check on both ends |
| Pipe-name squatting (Windows) | SID in name, `FIRST_PIPE_INSTANCE`, relay checks server SID |
| Socket folder swapped for a symlink (macOS) | `symlink_metadata`, owner and mode checked before bind |
| Huge or malformed stdin | 1 MB cap → drop event; parse failure → exit 0; `tool_response` dropped |
| User approves a truncated command | No per-field truncation; whole event or nothing |
| Settings file damaged or clobbered | Dated backup, merge only our entries, diff shown, `y` required, atomic rename; key order kept (`serde_json` `preserve_order`) |
| Uninstall removes user hooks | Only entries pointing at `bouncer-hook` removed; containers removed only if we emptied them |
| Shell injection via install path | Exec form, no shell |
| Hooks fire inside the Claude Code session building Bouncer | Recording and manual tests only in `../bouncer-playground` |

Phase 1 is done when:

- [x] Relay: stdin > 1 MB, malformed JSON, missing fields, app closed → empty
      stdout, exit 0; app closed exits fast (measured: ~25 ms median on Windows,
      including process start)
- [x] Hung app: fire-and-forget gives up at 2 s; decision budget is 110 s (tested:
      still waiting after 3 s; the 110 s value itself is a constant, not run)
- [x] Real recorded payloads (playground) are fixtures and parse into `Event`
- [x] `allow`/`deny` from the app print the documented JSON; anything else prints
      nothing; non-`PermissionRequest` events never print
- [x] Two sessions stream concurrently to one server
- [x] Pipe DACL / socket folder mode set; peer check on both ends (code + test of
      the own-user path; another-user path reviewed by hand). Live pipe read back
      as `D:P(A;;FA;;;<own SID>)`, protected, one entry
- [x] install + uninstall leaves a settings file byte-identical; backup written;
      no write without `y`; user hooks untouched
- [x] Manual: real `PermissionRequest` allowed end to end, and unanswered → normal
      terminal prompt (playground, Claude Code 2.1.143 — first written down as
      2.1.287, corrected after review)
- [x] Manual: real `PermissionRequest` denied end to end (playground, default
      permission mode: Claude Code showed "Denied in Bouncer", file not created)
- [x] fmt, clippy, tests green locally and in CI (Windows + macOS)

Manual checks (playground `../bouncer-playground`; version read from each
session transcript's `"version"` field):

- 2026-10-02, Claude Code 2.1.143 (session `c419a738`): a real Write
  `PermissionRequest` allowed through Bouncer; unanswered requests fell back to
  the terminal prompt; `git status` (allowed by Claude Code) fired only
  `PreToolUse`. Fixtures: `claude-code-2.1.143-*` (12).
- 2026-10-02, Claude Code 2.1.287 (session `4936ec29`, `auto` mode): Bash tool
  events only; no `PermissionRequest` fired. Fixtures: `claude-code-2.1.287-*`.
- 2026-10-02, Claude Code 2.1.287 (session `c6c7d2ff`, `default` mode): a real
  Write of `deny-test.txt` denied through Bouncer; Claude Code showed "Denied in
  Bouncer" and the file wasn't created. Fixture:
  `claude-code-2.1.287-PermissionRequest-Write.json` (+ its `PreToolUse`,
  `Notification`). Automated equivalent: `permission_answers_print_the_documented_json`.

Plan corrections found in this audit:

1. Relay dependencies: std + `serde_json` + `windows-sys` (Windows only).
   Decision (Charan): `windows-sys` 0.61 — already in `Cargo.lock` via Tauri.
2. The socket server goes in `bouncer-core`, not `bouncer-app` (testability); the
   CLI is a `bouncer` binary in `bouncer-core`.
3. Hook timeouts must be set explicitly; exec form needs Claude Code ≥ 2.1.139.
4. "Drop large fields" means drop `tool_response` and refuse > 1 MB, not truncate.
5. (Verify) The relay first read stdin outside the deadline; a stdin left open
   would have hung it until Claude Code's hook timeout. Fixed and tested.

### Phase 1 review follow-ups (2026-10-02, on `phase-2`)

From Charan's review of PR #2, each a separate commit with tests:

1. `BOUNCER_ENDPOINT` is honored only in debug/test builds; CI also runs the
   relay's unit tests in release to prove release ignores it.
2. Settings backups (and the replacement file) get the original's permissions
   before any byte is written.
3. macOS socket moved to `~/Library/Application Support/Bouncer/` (fixed per
   user). Socket paths are limited to 104 bytes; that leaves room for user names
   up to ~48 characters, and a longer one makes `bind` fail safe.
4. `CLAUDE_CONFIG_DIR` is respected (it already was) and now tested end to end;
   an empty value no longer means `./settings.json`.
5. The 2.1.143 fixture names were right. The nine "2.1.287" fixtures were
   recorded by a session still running 2.1.143 (transcript `"version"` field);
   renamed, and ten real 2.1.287 recordings added. A test ties each fixture name
   to its recording session's version. The allow-path manual test also ran on
   2.1.143; the deny-path one ran on 2.1.287.

Also fixed: a race in two relay tests that counted events before the server's
handler thread had run.

## Phase 2 — Island window and manual approvals (~1.5 weeks)

- **Audit:** Tauri window options (transparent, always on top, no taskbar icon, no
  focus stealing); list every IPC command needed and nothing more.
- **Implement:** states hidden / peek / open; session list by session ID; approval
  card (agent, project, tool, full command in monospace, Allow once / Deny); queue
  with count, nothing dropped; decisions bound to single-use request IDs; tray menu
  (Pause, Settings, Quit).
- **Test:** approve and deny real requests; unanswered → terminal prompt after
  deadline; three sessions; `<script>`, ANSI escapes and right-to-left Unicode show
  as inert text.
- **Verify:** CSP `script-src 'self'`, no remote content; capabilities limited to our
  commands (no shell, no fs plugin); `textContent` only; idle CPU ~0%; Allow buttons
  arm after 600 ms, Allow is never default focus, Enter does not approve.
- Commits: `feat(app): island window shell` → `feat(ui): session list` →
  `feat(ui): approval card and queue` → `feat(app): request-bound decisions` →
  `feat(app): tray menu` → `test(ui): hostile command rendering`

### Phase 2 audit (2026-10-02)

Sources: Tauri 2.12 config schema and source (`tauri`, `tao` 0.37), Phase 0/1 notes.

Window (`main`, the island):

- `visible: false` at start, `decorations: false`, `resizable: false`,
  `alwaysOnTop: true`, `skipTaskbar: true`, `visibleOnAllWorkspaces: true`,
  `dragDropEnabled: false`, no maximize/minimize/close buttons.
- **No focus stealing:** `focus: false` covers only the first show. On Windows
  tao clears its "don't focus" marker after creation, so every later `show()`
  uses `SW_SHOW` and activates the window; on macOS `show()` is
  `makeKeyAndOrderFront`. So the window is `focusable: false`: it never takes
  keyboard focus, and clicks still work (`acceptFirstMouse` on macOS).
  Keystrokes stay in the terminal, so Enter can never approve. Trade-off: the
  island has no keyboard access; Phase 5 (accessibility) revisits it.
- macOS: activation policy `Accessory` (no Dock icon).
- **No transparency:** `transparent` needs `macOSPrivateApi` on macOS. The
  island is an opaque undecorated window instead.
- Placement: top centre of the primary monitor's work area, re-centred on
  every resize. Sizes: hidden (no sessions), peek (a pill), open (a panel that
  scrolls inside).

IPC (every command listed in `build.rs`'s `AppManifest` and granted in the one
capability; no core permissions, no plugins):

| Command | What | Why it's safe |
| --- | --- | --- |
| `subscribe(channel)` | Backend pushes the island view (sessions, queue, paused) on every change | Read-only; Tauri's channel fetch is exempt from the ACL by design |
| `decide(id, allow)` | Answers one queued request | Only a live, unused, backend-issued ID works; Allow refused < 600 ms after the card is issued |
| `expand(open)` | Peek / open when the pill is clicked | Only toggles our own window size |

The frontend gets no window, event, shell, fs or opener permissions; the
backend resizes and shows the window itself. The frontend uses `@tauri-apps/api`
(approved by Charan, 2026-10-02) for `invoke` and `Channel` only.

Decisions and queue (`bouncer-core`, `approvals` module, testable without Tauri):

- Each permission request gets a 128-bit ID: two `RandomState` (OS-seeded
  SipHash) hashes of a counter. std only; IDs never leave the process. Single
  use: removed on decide, timeout or pause.
- The connection thread waits on its own channel up to **100 s**, below the
  relay's 110 s budget (`bouncer_relay::DECISION_BUDGET`, now shared). On
  timeout the card is removed and nothing is answered, so the terminal asks.
- FIFO queue across sessions, unbounded, nothing dropped; the card shows "1 of N".
- Pause (tray): every queued request is released unanswered and new ones aren't
  queued, so all go to the terminal. The tray tooltip says "paused" and the
  tray icon becomes a greyed copy made in code. Sessions keep updating.
- Sessions keyed by session ID: project, last tool, state (working / needs
  you / idle); removed on `SessionEnd`.

Rendering (security rule 4):

- Every agent string is set with `textContent`. On top of that, the backend
  makes invisible characters visible before display: C0/C1 controls (ANSI
  escapes), bidi controls (U+202A–202E, U+2066–2069, U+200E/200F, U+061C) and
  zero-width characters become `\u{XXXX}`, so a command can't hide or reorder
  text. The command block is `direction: ltr; unicode-bidi: isolate`.
- Bash shows `tool_input.command`; other tools show `tool_input` as pretty
  JSON, in full (scrolls; never cut).
- A test fails if `src/` uses `innerHTML`, `outerHTML`, `insertAdjacentHTML` or
  `document.write`.
- Allow is disabled for 600 ms each time a new card reaches the front and is
  never focused; the window can't take keys at all.

Phase 2 threats:

| Threat | Fix |
| --- | --- |
| Script / markup injection from a command or path | `textContent` only; CSP `script-src 'self'`; test bans HTML sinks |
| ANSI / bidi / zero-width tricks hide what's approved | Escaped to visible `\u{XXXX}` before display; LTR isolate |
| Forged or replayed decision | Random single-use IDs from the backend; unknown or used ID → error, nothing answered |
| Click lands on the wrong card (queue shifts) | Decision names the ID shown; 600 ms re-arm when the front card changes |
| Accidental approval | 600 ms arm (UI and backend), no focus, no Enter |
| Window steals focus, keystrokes land in the island | `focusable: false` |
| Island blocks Claude Code | 100 s wait < 110 s relay budget; pause and quit release everything |
| Requests lost | Unbounded FIFO; timeout and pause fall back to the terminal, never "allow" |
| Frontend reaches more than it needs | Three commands in the manifest; no core/plugin permissions; no remote URLs |
| Command truncated | Full input shown, scrolls |

Phase 2 is done when:

- [ ] Island window: hidden / peek / open; top centre; on top; no taskbar
      entry; never takes focus
- [ ] Session list by session ID; three concurrent sessions shown
- [ ] Approval card (agent, project, tool, full command in monospace, Allow once /
      Deny), queue with count, nothing dropped
- [ ] Decisions bound to single-use IDs (unit tests: unknown, replayed, early
      Allow, timeout, pause)
- [ ] Tray: Pause/Resume (icon and tooltip show paused) and Quit
- [ ] Hostile strings (`<script>`, ANSI, RTL override, zero-width) render as
      inert visible text (core tests + manual check)
- [ ] End-to-end test: real relay → server → queue → decide → relay prints the
      documented JSON
- [ ] Manual: real request approved and denied from the island; unanswered →
      terminal prompt after the deadline; three playground sessions
- [ ] CSP unchanged; capability = our three commands; idle CPU ~0%
- [ ] fmt, clippy, tests green locally and in CI (Windows + macOS)

Manual checks:

- 2026-10-02, no Claude Code (debug relay fed by a script; three fake sessions):
  peek pill at top centre, always on top, never foreground. A queued `Bash`
  request with `<script>`, ANSI and U+202E showed as inert text with
  `\u{001B}` / `\u{202E}` markers. An unanswered request timed out and the relay
  printed nothing.
- 2026-10-02, same setup, Charan clicked Deny (window temporarily focusable):
  card cleared, island shrank to the pill, relay printed the documented deny JSON.
- 2026-10-02, same setup, final `focusable: false`: Charan typed in another
  app, clicked Deny once, kept typing. One click was enough, keyboard focus never
  left the other app, and the relay printed deny.

Plan corrections found in this audit:

1. Tray "Settings" moves to Phase 5 with the settings screen (Charan).
2. Transparent window dropped (needs the macOS private API).
3. "Enter does not approve" is guaranteed by `focusable: false`; keyboard access
   to the island becomes a Phase 5 accessibility item.
4. New dependency: `@tauri-apps/api` 2.12.1 (Charan approved). Tauri's
   `tray-icon` feature is turned on; it adds no crates to `Cargo.lock`.
5. Known limit: if a relay dies while its card is up (Claude Code killed), the
   card stays until the 100 s deadline; deciding it then does nothing.

## Phase 3 — Rules engine (~1.5 weeks)

Outcomes: auto-allow (logged), ask with a risk reason, or ask plainly. Nothing is
auto-denied in the MVP; anything not fully understood goes to the user.

- **Audit:** list Claude Code's own permission rules so ours complement them; collect
  evasion examples (`;`, `&&`, `|`, `$(...)`, backticks, `eval`, `bash -c`, aliases,
  `env` tricks, Unicode lookalikes) as test cases.
- **Implement:** `rules.toml` in the user config folder with shipped defaults;
  proper shell-word parser; compound commands auto-allowed only if every part is;
  unparseable → ask; paths fully resolved, outside the project → ask; risk flags
  with one-line reasons (deletes outside project, `.env`/keys, `curl | sh`,
  `git push --force`, `sudo`, shell profiles, `~/.claude`); observe mode by default;
  "Always allow" previews the exact rule.
- **Test:** 150+ case table incl. every evasion example; fuzz test: random input
  never crashes and never auto-allows.
- **Verify:** rules file refused if others can write it; rule changes shown in the
  island; observe mode default on fresh install.
- Commits: `feat(rules): rule file format and defaults` → `feat(rules): command parser and compound handling` →
  `feat(rules): path checks` → `feat(rules): risk flags with reasons` →
  `feat(ui): observe mode and always-allow preview` → `test(rules): case table and fuzzing`

## Phase 4 — Activity log and away summary (~1 week)

- **Audit:** exact stored fields; secret patterns (API keys, tokens, URL passwords,
  private keys, `.env` values).
- **Implement:** SQLite in app data folder, owner-only; redaction before write; never
  store outputs or file contents; 30-day retention + "Wipe history"; away summary
  after 10+ idle minutes (files changed, commands, auto-allowed vs asked, where it got
  stuck, time; token cost only if hook data has it).
- **Test:** redaction fixture of fake secrets; retention; summary of a recorded
  30-minute session.
- **Verify:** nothing leaves the machine; no fake secret anywhere on disk after tests.
- Commits: `feat(log): local event store` → `feat(log): secret redaction` →
  `feat(log): retention and wipe` → `feat(ui): away summary card` → `test(log): redaction and summary fixtures`

## Phase 5 — Character, sound and polish (~1 week)

- **Audit:** sketch five states (idle, working, needs you, risky, done); motion and
  sound budget.
- **Implement:** bouncing-ball character on a 2D canvas, no image files (bounces while
  working, blocks the door when risky, hops when done); Web Audio generated sounds;
  settings screen (sound, observe/auto-allow, rules file, hooks install/uninstall,
  wipe history); pause when hidden; honor OS reduced motion.
- **Test:** five states render; mute works; keyboard-only settings; screen reader order.
- **Verify:** contrast ≥ 4.5:1; idle CPU ~0%; `CREDITS.md` complete.
- Commits: `feat(ui): character renderer` → `feat(ui): character states` →
  `feat(ui): generated sounds` → `feat(ui): settings screen` → `feat(ui): reduced motion and accessibility`

## Phase 6 — Packaging and release (~1 week)

- **Audit:** document unsigned-app warnings per OS; Defender false-positive process;
  check eligibility for free SignPath Foundation signing.
- **Implement:** release workflow on `v*` tags: macOS universal `.dmg`, Windows
  `.msi`, SHA-256 checksums, GitHub build attestations; README with pitch, demo GIF,
  install / verify / uninstall steps, privacy statement.
- **Test:** install, use, uninstall on clean Windows (own PC) and macOS (friend's Mac,
  using the CI build); `settings.json` restored exactly.
- **Verify:** only the release job can write; no secrets in logs; README alone is enough.
- Commits: `ci: release workflow with checksums and attestations` →
  `docs: install, verify and uninstall guide` → `chore: v1.0.0`

---

## Threat model

Core rule: every failure means "ask in the terminal," never "allow."

| Threat | Fix | Phase |
| --- | --- | --- |
| App fails open | Errors/timeouts return empty output; tests assert no default allow | 1 |
| App blocks the agent | Hard relay deadlines; exit 0 if app missing | 1 |
| Another user answers approvals | User-only socket/pipe + same-user check both ends | 1 |
| Oversized / malformed input | 1 MB cap, field trimming, parse failure → exit 0 | 1 |
| Settings file damaged | Backup, merge, diff, confirm, atomic write | 1 |
| Script injection in island | `textContent` only, strict CSP, minimal capabilities | 2 |
| Forged / replayed approval | Single-use random request IDs | 2 |
| Accidental approval | 600 ms arm delay, no default focus, Enter doesn't approve | 2 |
| Disguised dangerous command | Real parser, compound needs all parts allowed, unparsed → ask, hidden chars flagged | 3 |
| Path escape | Resolve fully; outside project → ask | 3 |
| Rules file tampering | Refuse files others can write; show changes | 3 |
| Secrets in history | Redact, no outputs, local, owner-only, retention, wipe | 4 |
| Supply chain | Few deps, lockfiles, cargo deny (+ weekly advisories), npm audit, Dependabot, SHA-pinned Actions | 0 |
| Tampered download | CI-only builds, checksums, attestations | 6 |

Out of scope: malware already running as the user (it can edit Claude Code's
settings directly). The README says so.

## Cost

$0. macOS: no Apple Developer Program ($99/yr) — users click "Open Anyway" once in
Privacy & Security. Windows: unsigned unless SignPath Foundation accepts the project —
users click "More info → Run anyway" once. CI, scanning and attestations are free for
public repos. No paid APIs.

## Decisions

- Name: **Bouncer**, bouncing-ball character.
- Repo and package name: **`bouncer-app`** (github.com/charanp11/bouncer-app).
  `bouncer` is taken on crates.io ("CLI sandboxing toolkit", 2026) and npm, and
  several small Claude Code permission-hook projects on GitHub already use it
  (karanb192/bouncer, karlkfi/claude-bouncer, clownware/bouncer). Crates are
  prefixed `bouncer-` (`bouncer-relay`, `bouncer-core`, `bouncer-app`); the app
  name stays Bouncer.
- Test machines: Charan's Windows PC; a friend's Mac (CI builds) for manual tests.
- Risky actions are flagged, never auto-denied, in the MVP.
- Lowest supported Claude Code version: **2.1.139** (Phase 1 audit).

## Phase summaries

(Added as each phase passes Verify.)

### Setup summary (2026-10-01)

Toolchain already present: git 2.51.2, Node 24.11.0, rustc/cargo 1.99.0
(stable-x86_64-pc-windows-msvc) with clippy and rustfmt, winget 1.29, WebView2
154.0. Nothing needed installing. `git init -b main`; repo-local identity
`Charan <177669994+charanp11@users.noreply.github.com>`.

### Phase 0 summary (2026-10-01)

- Repo: github.com/charanp11/bouncer-app, public. Crates `bouncer-relay` (binary
  `bouncer-hook`, no dependencies yet), `bouncer-core` (empty), `bouncer-app` (Tauri
  2.12 shell). Frontend: Vite 8 + TypeScript 7, no UI framework.
- The Tauri shell has the CSP from the audit, a looser `devCsp` for dev only (Vite
  injects styles and uses an HMR websocket), `freezePrototype`, no plugins, no
  commands, an empty command allow-list in `build.rs`, and one capability for
  `main` with no permissions. Vite listens on `127.0.0.1:1420` only.
- CI (`ci.yml`): fmt, clippy `-D warnings` and tests on Windows and macOS (the
  frontend is built first); `cargo deny check`, `npm audit` and gitleaks on Linux.
  `advisories.yml` runs `cargo deny check advisories` every Monday. Actions pinned to
  SHAs; `permissions: contents: read`; checkout without persisted credentials.
- GitHub: private vulnerability reporting, Dependabot alerts and security fixes,
  read-only default Actions token, Actions can't approve PRs, rebase-merge only,
  branches deleted after merge. `main` protection: PR required (0 approvals), the
  five CI checks required (bound to GitHub Actions) and up to date, linear history,
  no force push or deletion, admins included.
- Tests: 1 (the relay prints nothing and exits 0), passing locally and in CI on
  both OSes.
- Notes: the repo was recreated once to drop a line from early history. Dependabot
  version updates and the weekly advisories job only start once their files are on
  `main`; run `advisories.yml` once by hand after the merge. cargo-deny warns about
  duplicate crate versions from Tauri's tree (allowed as warnings).

### Phase 1 summary (2026-10-02)

- `bouncer-hook` (relay): reads one hook event (≤ 1 MB, else dropped whole),
  drops `tool_response` / `transcript_path`, forwards it as one JSON line. All
  blocking work, stdin included, runs on a worker under a 2 s budget, extended to
  110 s only for `PermissionRequest`. Prints only for an exact `allow` / `deny`
  from a same-user app. Dependencies: `serde_json`, plus `windows-sys` on Windows.
- Transport: Windows named pipe `\.\pipe\bouncer-<SID>` with a protected
  user-only DACL, `FIRST_PIPE_INSTANCE`, remote clients rejected, SID checked on
  both ends. macOS: socket in `$TMPDIR/bouncer-<uid>/` (`0700`, owner and mode
  checked), `getpeereid` on both ends.
- `bouncer-core`: `Event` (agent, session, project, kind, tool, input, time); the
  `ipc` server; `hooks` install/uninstall logic and the `bouncer` CLI. The app
  starts the server and answers nothing yet (every request falls back to the
  terminal). `examples/answer.rs` is a console stand-in used for manual checks.
- `bouncer install-hooks` / `uninstall-hooks`: exec-form hooks for eight events,
  diff, `y` required, dated backup, atomic rename, key order kept.
- Lowest supported Claude Code: 2.1.139.
- Tests: 35 (relay 17, core 18), green locally and in CI on Windows and macOS.
  Fixtures: 12 events from Claude Code 2.1.143 (incl. two real
  `PermissionRequest`s; nine were first mislabeled 2.1.287, fixed after review)
  and 10 from 2.1.287 (incl. one `PermissionRequest`), added after review.
- Checked by hand: playground session allowed a Write through Bouncer; unanswered
  requests fell back to the terminal prompt; `git status` (already allowed by
  Claude Code) fired only `PreToolUse`; the live pipe's DACL read back user-only.
- Denied by hand: a real Write denied through Bouncer showed "Denied in Bouncer /
  Denied by PermissionRequest hook" and the file wasn't created.
- Not done by hand: a second Windows/macOS account trying to connect (no second
  account; DACL and peer checks reviewed instead); the macOS socket was exercised
  only by CI.
- Note for Phase 3: in Claude Code's `auto` permission mode, Claude Code approves
  many actions itself and no `PermissionRequest` fires; Bouncer only sees them
  through `PreToolUse`.
- Dependabot alert #1 (glib < 0.20, GHSA-wrw7-89jp-8q8g): reaches us only through
  Tauri's Linux GTK stack; not compiled for Windows or macOS. Dismissed as
  "vulnerable code is not used".
