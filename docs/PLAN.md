# Bouncer — MVP plan

Bouncer is a free, open-source desktop companion for Claude Code. It shows every
session live, auto-approves safe actions under rules you control, flags risky ones
with a plain reason, and never blocks the agent. Seven phases, about eight weeks, $0.

**Current phase: Phase 0 — Repo and guardrails**

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

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
      pass locally on Windows and in CI on `windows-latest` and `macos-latest`
- [ ] Crates `bouncer-relay`, `bouncer-core`, `bouncer-app`; `relay` depends only on
      std + `serde_json`
- [ ] `npm run tauri dev` opens an empty Bouncer window on Windows
- [ ] `tauri.conf.json` has the CSP above and `freezePrototype: true`; no plugins; no
      commands; one capability file for `main` with the fewest core permissions that
      work; no `remote`
- [ ] `cargo deny check`, `npm audit` and gitleaks pass in CI; the weekly
      advisories job exists
- [ ] Every Action pinned to a SHA; workflow `permissions: contents: read`
- [ ] Dependabot (cargo, npm, github-actions) and private vulnerability reporting
      are on; branch protection on `main` as listed under Verify
- [ ] `LICENSE` (MIT), `SECURITY.md`, `CREDITS.md`, `README.md`, `.gitignore`,
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
- Lowest supported Claude Code version: decided in Phase 1 audit.

## Phase summaries

(Added as each phase passes Verify.)

### Setup summary (2026-10-01)

Toolchain already present: git 2.51.2, Node 24.11.0, rustc/cargo 1.99.0
(stable-x86_64-pc-windows-msvc) with clippy and rustfmt, winget 1.29, WebView2
154.0. Nothing needed installing. `git init -b main`; repo-local identity
`Charan <177669994+charanp11@users.noreply.github.com>`.
