# Bouncer

A free, open-source desktop companion for Claude Code on Windows and macOS.

Bouncer shows your Claude Code sessions live, auto-approves safe actions under
rules you control, flags risky ones with a plain reason, and never blocks the
agent: if Bouncer is closed, slow or confused, Claude Code just asks you in the
terminal as usual.

**Status:** early development. Nothing to install yet.

## Chat

Ask Claude a question from the island: the chat button next to the gear in
the session list opens it.

- **It runs your own Claude Code** (`claude -p`, signed in as you). Each
  message counts against your Claude plan's usage, like any Claude Code
  message; the panel shows your plan's 5-hour and 7-day use. Bouncer never
  reads, stores or passes an API key, and a run that isn't on your Claude
  login is stopped.
- **It's read-only.** A chat can only Read, Grep and Glob inside its own
  empty folder. It can't edit or write files, run commands, use the web,
  your MCP servers or connectors, or your Claude Code settings and hooks.
- **Nothing is kept.** Chats live in memory only: no history, no transcript.
  Cancel, a 5-minute limit per message, or quitting Bouncer stops Claude Code
  and everything it started.
- **First, confirm which `claude` it runs.** The first time, the panel shows
  the full path of the Claude Code it found (a native `claude.exe`, or
  `claude` on macOS; npm's `.cmd` / `.ps1` shims are never run). Open
  Settings → **Claude Code for chat**, check the path, and click **Confirm
  this path**. If that path ever changes, Bouncer asks again before any chat
  runs.

## Building from source

Needs Rust (stable) and Node.js 24.

```sh
npm ci
npm run tauri dev
```

## Privacy

No telemetry, and Bouncer itself makes no network calls. Everything stays
on your machine, except what you send in a chat: that goes to Claude through
your own Claude Code, as any Claude Code message does.

## License

[MIT](LICENSE). Security issues: see [SECURITY.md](SECURITY.md).
