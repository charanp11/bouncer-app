#!/bin/bash
# What a finished Mac build must be, from the release job and the installer test:
#
#   check-mac-bundle.sh target/universal-apple-darwin/release/bundle X.Y.Z
#
# Both architectures in all three programs, one sealed ad-hoc signature, a
# sound .dmg, and an updater archive that unpacks to the same sealed app.
set -euo pipefail

bundle=$1
version=$2
app="$bundle/macos/Bouncer.app"
archive="$bundle/macos/Bouncer_${version}_universal.app.tar.gz"
fail() { echo "::error::$1"; exit 1; }

for name in bouncer-app bouncer-hook bouncer; do
  archs=$(lipo -archs "$app/Contents/MacOS/$name")
  case "$archs" in
    "x86_64 arm64" | "arm64 x86_64") ;;
    *) fail "$name has [$archs], not both architectures" ;;
  esac
done

codesign --verify --deep --strict "$app" || fail "the app's signature doesn't verify"
# Held in a variable: grep -q stopping early would fail a pipe under pipefail.
signature=$(codesign --display --verbose=2 "$app" 2>&1)
grep -q '^Signature=adhoc' <<< "$signature" || fail "the app isn't ad-hoc signed"

hdiutil verify "$bundle/dmg/Bouncer_${version}_universal.dmg" || fail "the .dmg doesn't verify"

# The updater skips each entry's first folder, so anything else at the top
# would land inside the bundle.
tops=$(tar -tzf "$archive" | cut -d/ -f1 | sort -u)
[ "$tops" = Bouncer.app ] || fail "the update archive's top level is [$tops], not Bouncer.app"
unpacked=$(mktemp -d)
tar -xzf "$archive" -C "$unpacked"
codesign --verify --deep --strict "$unpacked/Bouncer.app" || fail "the unpacked update's signature doesn't verify"
rm -rf "$unpacked"

echo "bundle $version: universal, ad-hoc signed, .dmg and update archive sound"
