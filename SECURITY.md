# Security policy

Bouncer sits between Claude Code and your approval of its actions, so security
bugs matter more here than in most apps.

## Reporting a vulnerability

Please report privately through GitHub:
**Security → Report a vulnerability** on
[charanp11/bouncer-app](https://github.com/charanp11/bouncer-app/security/advisories/new).
Don't open a public issue.

Include what you found, how to reproduce it, and the impact you expect. You'll get
a reply within 7 days. Fixes are released before details are made public, and
you'll be credited unless you'd rather not be.

## Supported versions

Only the latest release gets security fixes. Bouncer is pre-1.0 today.

## What counts

Especially interested in:

- anything that makes Bouncer answer "allow" when it shouldn't (fail-open)
- anything that lets Bouncer block or hang Claude Code
- another local user, process or web page answering approvals
- agent-controlled text (commands, paths) executing as script in the app
- damage to Claude Code's user settings file
- secrets ending up in Bouncer's local history

Out of scope: malware already running as your user. It can change Claude Code's
settings directly, with or without Bouncer.
