# Bouncer

A free, open-source desktop companion for Claude Code on Windows and macOS.

Bouncer shows your Claude Code sessions live, auto-approves safe actions under
rules you control, flags risky ones with a plain reason, and never blocks the
agent: if Bouncer is closed, slow or confused, Claude Code just asks you in the
terminal as usual.

**Status:** early development. Nothing to install yet.

## Building from source

Needs Rust (stable) and Node.js 24.

```sh
npm ci
npm run tauri dev
```

## Privacy

No telemetry and no network calls. Everything stays on your machine.

## License

[MIT](LICENSE). Security issues: see [SECURITY.md](SECURITY.md).
