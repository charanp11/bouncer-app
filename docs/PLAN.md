# Bouncer — MVP plan

Bouncer is a free, open-source desktop companion for Claude Code. It shows every
session live, auto-approves safe actions under rules you control, flags risky ones
with a plain reason, and never blocks the agent. A security guard for coding
agents, with personality. Ten phases (0–9), $0.

**Current phase: Phase 4 — Activity log and away summary**

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

- [ ] Day files written owner-only, refused if others can write; one event per
      line; bad lines skipped; 10 MB cap with a note in the island
- [ ] Redaction of all six families, before write (fake-secret fixture; no
      fake secret in any log file the tests write)
- [ ] 30-day retention and "Wipe history" (tray)
- [ ] Away summary matching the prototype's "Away summary" state, from the
      log, after 10+ idle minutes
- [ ] Summary of a 30-minute session fixture gives the expected numbers
- [ ] Idle sessions drop off (30 min; 4 h when waiting on the user)
- [ ] Manual: away summary in the real island after an idle span, next to the
      prototype; wipe; log file contents read by hand for secrets
- [ ] fmt, clippy, tests green locally and in CI (Windows + macOS)

Plan corrections found in this audit:

1. Storage is JSON Lines, not SQLite (Charan).
2. Token cost isn't in any hook payload; not shown.
3. "Auto-allowed" in the summary counts every tool call that didn't need the
   user (Claude Code's own rules or Bouncer's), as the prototype's numbers
   imply (26 auto-allowed of 31 commands, 3 asked).
4. Day files are per UTC day (std has no time zones); retention counts UTC days.

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
- **Implement:** bouncing ball on a 2D canvas, no image files: real physics bounce
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
| Clicks on the character land on Allow | Character and approval buttons never overlap; squish/dizzy clicks go to the canvas only |
| Idle CPU / battery drain | Animation loop stops when idle or hidden; cursor feed throttled or window-local |
| Motion sickness | OS reduced motion honoured; setting to turn motion off |

Phase 5 is done when:

- [ ] Every state has a pose; bounce, eyes, blink, squish, dizzy, greeting work
- [ ] Generated sounds with mute; reduced motion honoured
- [ ] Settings screen, keyboard access and screen reader order checked by hand
- [ ] Idle CPU ~0%; approval card never covered
- [ ] fmt, clippy, tests green locally and in CI

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
