# Bouncer and Coucou on Windows

Charan's goal (2026-10-12): on Windows, Bouncer must be clearly better than
Coucou where it matters: safety, CPU and memory, reliability.

**Method.** Coucou's public source, read through GitHub at
`Louis-CFM/coucou@45e7e6b0246b` (2026-10-12), Windows app in `windows/`
(release `windows-v0.4.0`, 2026-10-11). Nothing was installed, built or run, so
none of Coucou's numbers below are measured: they are what its code and README
say. Bouncer's numbers are measured (`docs/PLAN.md`). Coucou is MIT; nothing
was copied.

## At a glance

| Area | Coucou (Windows) | Bouncer | Who's ahead |
| --- | --- | --- | --- |
| Agents with Allow / Deny | Claude Code, Codex, Copilot CLI, Muse Code; eight more watched (asked in their own terminal) | Claude Code | Coucou |
| Auto-allow by rules | None ("Always" removed until a rules list exists; `views.ts`) | Rules file, observe / auto modes, exact "Always allow" rule preview, risky requests flagged with a reason and never auto-allowed, compound / unparseable commands always asked, 150+ case table and fuzzing | **Bouncer** |
| Accidental approval | Clicks ignored for 600 ms after a card appears, in the page only | 600 ms arm enforced by the backend too (a too-early Allow is refused), visible fill, focus never on Allow, Enter never approves, a card arriving while you type takes no keys | **Bouncer** |
| Decisions bound to the request | The app's process ID and a counter (`pipe.rs`): guessable | Random 128-bit single-use IDs | **Bouncer** |
| Pipe security | Pipe name carries the SID; the hook checks the server is the same user; the server sets no access list of its own and doesn't check the client on Windows (`pipe.rs`; Windows' default pipe access gives other accounts read only) | User-only access list set by the server, same-user check on both ends, a second server refused | **Bouncer** |
| Never blocking the agent | 300 ms connect, 2 s fire-and-forget, 110 s decision; no answer means the terminal asks | Same budgets (2 s, 100 / 110 / 120 s nested), tested against a hung app and a crash mid-card | Even |
| Web page lockdown | CSP allows inline styles and `data:` / `blob:` images; window gets `core:default` and autostart | Only our script and style, no inline, no `data:`; only our own commands; both tested | **Bouncer** |
| Agent text is never HTML | Built with text nodes, Markdown from text nodes (no `innerHTML` in the files read) | `textContent` only, enforced by a test over every frontend file | Even (we test it) |
| Network | GitHub, Spotify, chat providers, an optional phone link (TLS server, mDNS, a relay); "no telemetry" | One daily GET of the latest release's `latest.json` (opt-out; nothing about the user sent); installs only on a click, signature checked; chat goes through your own Claude Code | **Bouncer** (less to trust) |
| Chat | API keys (Anthropic, OpenAI, Gemini, OpenRouter, local models), kept in Windows Credential Manager; Markdown answers | Your own Claude Code login (no key, counts against your plan), locked to read-only in an empty folder, nothing on disk; Markdown is roadmap a | Mixed: we're safer and need no key; they have more models and Markdown |
| Activity history | Weekly recap | Local, redacted activity log (secrets removed before writing), 30-day retention, one wipe path; away summary | **Bouncer** (redaction, wipe) |
| Settings file writes | Diff, dated backup, consent, only its entries | Same, plus byte-identical uninstall and atomic write (tested) | Even |
| CPU | Cursor polled from Rust at ~60 Hz while the island is open, twice a second when folded (`island.rs`); not measured here | Event-driven, no polling; motion in short bursts: idle 0.5–1.7%, working 4%, a card 4.7%, chat streaming 15% (one core, 60 s) | **Bouncer** by design; theirs unmeasured |
| Memory | Larger dependency tree (tokio, reqwest, rustls, ring, keyring, WebSocket, mDNS); not measured | Measured (release build, Task Manager's private memory, app + WebView2, 60 s): idle 93 MB, a card up 103 MB; our own process 5 MB, the rest is WebView2 | Unknown: Coucou not measured (needs installing on a test machine) |
| Tests | Unit tests (66 in the ten Rust files read), JS tests | 196 Rust + 71 frontend tests; a regression gate (every scenario listed, CI fails on a missing test); Windows and macOS CI, macOS smoke test | **Bouncer** |
| Installer and releases | NSIS (current user, no admin) and MSI, GitHub releases, unsigned with a SmartScreen walkthrough, Defender false-positive note | None yet (roadmap c) | Coucou |
| Extras | File drop, live diffs, plan usage from the status line, GitHub, Spotify, global shortcuts, phone approvals, outfits, desktop mascot, two languages, Linux | Live diffs, away summary, sounds, settings, chat | Coucou |

## Where Bouncer is better

1. **It can decide for you, safely.** Coucou only forwards Allow / Deny.
   Bouncer's rules engine auto-allows what you've allowed and flags risky
   requests with a plain reason. It never auto-allows a compound or
   unparseable command. This is the core of the product and Coucou has no
   equivalent.
2. **Harder to approve by accident.** Our arm delay holds in the backend as
   well as the page, focus never lands on Allow, and Enter never approves. A
   card that arrives while you type takes no keys (found and fixed by a
   real-click check).
3. **Tighter local security.** The pipe has a user-only access list and
   same-user checks on both ends, decisions use random single-use IDs, the
   CSP is strict, and only our own commands are exposed. Coucou leaves the
   Windows pipe at default security, allows inline styles and `data:`
   images, and grants `core:default`.
4. **Almost nothing on the network.** Coucou talks to GitHub, Spotify, chat
   providers and, optionally, a phone or relay. Bouncer's only request of
   its own is the daily update check (one small file from GitHub, nothing
   about you; off in Settings).
5. **Chat without a key.** Ours runs your own Claude Code login: no API key
   to store and no second bill. It's locked down (Read / Grep / Glob, empty
   folder, no settings, hooks or MCP) and keeps nothing.
6. **Cheaper to keep open.** Bouncer has no polling loop, and its motion
   comes in short bursts. Coucou polls the cursor at 60 Hz whenever its
   island is open.
7. **Proven by tests.** Every scenario sits in `docs/TESTS.md`, CI fails if
   a named test is missing, and it runs on Windows and macOS.

## Where Bouncer is behind

1. **Releases.** No installer or GitHub release yet. Coucou ships NSIS + MSI
   with a SmartScreen walkthrough.
2. **Other agents.** Coucou approves for Codex, Copilot CLI and Muse Code,
   and watches eight more. We support Claude Code only (roadmap h).
3. **Questions.** Coucou answers Claude Code's questions in the island. We
   send them to the terminal.
4. **Chat polish.** Coucou renders Markdown (roadmap a), offers many models,
   and takes dropped files (roadmap d).
5. **Convenience.** Global shortcuts (jump to the waiting card, open chat),
   "Open terminal", plan usage in the island at all times, autostart, a
   second language.
6. **Breadth.** Linux, phone approvals and integrations. Not goals for us.

## What to fix before the Windows release

In order, as part of roadmap c unless noted:

1. **Measure memory** (working set, app + WebView2, idle and with a card)
   and put it next to the CPU numbers. Done 2026-10-12 (`docs/PLAN.md`,
   roadmap c): idle 93 MB, a card 103 MB (Task Manager's private memory),
   5 MB of it ours. Coucou can be measured the same way on a test machine
   (install it there, not on the dev PC).
2. **Installer:** per-user, no admin, and an uninstall that removes only our
   hook entries (Coucou leaves `settings.json` alone on uninstall; we should
   offer to remove our entries, with the diff). Add an autostart option,
   off by default.
3. **SmartScreen and Defender:** the same honest walkthrough, plus checksums
   and attestations. Signing is audited in c (cost: $0 if possible).
4. **Hooks health:** Settings warns when installed hooks are missing events,
   and one click re-installs (already planned for Phase 9).
5. **Markdown in chat** (roadmap a), before the release, so chat doesn't look
   unfinished next to Coucou's.
6. **A shortcut to the waiting card** (one key combo, never one that
   approves), so the keyboard-first flow is as quick as Coucou's. Small.
7. Keep the lead: every new feature keeps the strict CSP, user-only pipe,
   no network and the regression gate.
