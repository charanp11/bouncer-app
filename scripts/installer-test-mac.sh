#!/bin/bash
# The .dmg, end to end, on a disposable Mac (CI's fresh macos-latest runner).
# Never run this on a real profile: it writes /Applications and Bouncer's data.
#
#   installer-test-mac.sh <Bouncer_X_universal.dmg> <fixtures dir>
#
# Mount, copy to /Applications, hooks with the installed CLI in a scratch
# folder, then for each architecture the installed app takes a request through
# the installed hook and prints nothing once it quits; uninstall-hooks puts
# settings.json back byte for byte. Clicks stay out: a runner has no
# Accessibility permission (the hand list covers them).
set -euo pipefail

dmg=$1
fixtures=$2
app=/Applications/Bouncer.app
bin="$app/Contents/MacOS"
sock="$HOME/Library/Application Support/Bouncer/bouncer.sock"
work=$(mktemp -d)
mount="$work/mount"
export CLAUDE_CONFIG_DIR="$work/claude"
settings="$CLAUDE_CONFIG_DIR/settings.json"

pass() {
  echo "PASS  $1  $2"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then echo "- $1: $2" >> "$GITHUB_STEP_SUMMARY"; fi
}
fail() { echo "::error::$1"; exit 1; }

# 1. Mount and copy, as a person would.
hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$dmg" > /dev/null
[ -L "$mount/Applications" ] || fail "the .dmg has no Applications link"
ditto "$mount/Bouncer.app" "$app"
hdiutil detach "$mount" > /dev/null
codesign --verify --deep --strict "$app" || fail "the installed app's signature doesn't verify"
pass "Install" "copied from the .dmg, signature intact"

# 2. Hooks with the installed CLI, in a scratch folder.
mkdir -p "$CLAUDE_CONFIG_DIR"
printf '{\n  "model": "opus"\n}\n' > "$settings"
cp "$settings" "$work/original.json"
echo y | "$bin/bouncer" install-hooks > /dev/null
hook=$(jq -r '.hooks.PermissionRequest[].hooks[].command | select(endswith("bouncer-hook"))' "$settings")
[ "$hook" = "$bin/bouncer-hook" ] || fail "the hook command is [$hook]"
args=$(jq -r '.hooks.PermissionRequest[].hooks[] | select(.command | endswith("bouncer-hook")) | .args | join(" ")' "$settings")
pass "Hooks" "$hook $args"

event() {
  jq -c --arg cwd "$PWD" '.cwd = $cwd | .session_id = "installer-test"' "$fixtures/$1"
}

# 3. A request through the installed hook, on each architecture. $1 names the
# run, the rest is what starts a program on that architecture.
use() {
  name=$1
  shift
  "$@" "$bin/bouncer-app" > "app-$name.log" 2>&1 &
  pid=$!
  for _ in $(seq 1 120); do
    [ -S "$sock" ] && break
    kill -0 "$pid" 2> /dev/null || fail "$name: the app exited before opening its socket"
    sleep 0.5
  done
  [ -S "$sock" ] || fail "$name: no socket within 60 s"

  # shellcheck disable=SC2086 # args are separate words on purpose
  event claude-code-2.1.287-SessionStart.json | "$@" "$hook" $args
  # shellcheck disable=SC2086
  event claude-code-2.1.288-PermissionRequest-Bash.json | "$@" "$hook" $args > "$work/answer-$name.txt" &
  relay=$!
  sleep 10
  kill -0 "$relay" 2> /dev/null || fail "$name: the request didn't wait for a card"
  kill -0 "$pid" 2> /dev/null || fail "$name: the app crashed"
  screencapture -x "island-$name.png"

  kill "$pid"
  wait "$pid" || true
  wait "$relay" || fail "$name: the relay exited with an error after the app quit"
  [ ! -s "$work/answer-$name.txt" ] || fail "$name: the relay answered: $(cat "$work/answer-$name.txt")"
  ! grep -q panicked "app-$name.log" || fail "$name: the app panicked"
  rm -f "$sock"
  pass "A card ($name)" "a request waited on the installed app; nothing printed after it quit"
}

use native env
softwareupdate --install-rosetta --agree-to-license > /dev/null 2>&1 || true
use x86_64 arch -x86_64

# 4. Hooks out again: the settings file as it was.
echo y | "$bin/bouncer" uninstall-hooks > /dev/null
cmp "$settings" "$work/original.json" || fail "settings.json wasn't restored"
pass "Uninstall hooks" "settings.json back byte for byte"
