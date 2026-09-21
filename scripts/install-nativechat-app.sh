#!/usr/bin/env bash
# Copy a built NativeChat.app, ad-hoc codesign, verify, then remasure-relaunch.
#
# Mac remasure: sync this file over ~/bin/install-nativechat-app.sh (that
# path is not in the repo). Always `--identifier dev.goldcoders.nativechat`.
#
# Usage:
#   scripts/install-nativechat-app.sh [SRC.app] [DEST.app]
# Defaults:
#   SRC  = <repo>/target/release/bundle/osx/NativeChat.app
#   DEST = /Applications/NativeChat.app

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/stop-nativechat-for-codesign.sh
source "$ROOT/scripts/stop-nativechat-for-codesign.sh"

BUNDLE_ID="dev.goldcoders.nativechat"
# The keychain records an item's permission against a signature, so an ad-hoc one — a fresh
# hash on every build — makes the app a stranger to its own saved logins each time it is
# rebuilt. The team's Developer ID keeps that permission from build to build, and it is the
# same identity the dev binary carries (scripts/sign-dev.sh). NATIVECHAT_SIGN_ID overrides;
# "-" goes back to ad-hoc.
SIGN_ID="${NATIVECHAT_SIGN_ID:-Developer ID Application: Goldcoders Corp (5KZ8MD34QW)}"
if [ "$SIGN_ID" != "-" ] && ! security find-identity -v -p codesigning | grep -qF "$SIGN_ID"; then
  echo "warning: no identity called \"$SIGN_ID\"; signing ad-hoc, the keychain will ask again after every build" >&2
  SIGN_ID="-"
fi
SRC="${1:-$ROOT/target/release/bundle/osx/NativeChat.app}"
DEST="${2:-/Applications/NativeChat.app}"
ENTITLEMENTS="$ROOT/nativechat.entitlements"

if [[ ! -d "$SRC" ]]; then
  echo "error: no app at $SRC (build with ./build.sh first)" >&2
  exit 1
fi

stop_nativechat_for_codesign

mkdir -p "$(dirname "$DEST")"
rm -rf "$DEST"
ditto "$SRC" "$DEST"

codesign --force --deep --sign "$SIGN_ID" --identifier "$BUNDLE_ID" \
  --entitlements "$ENTITLEMENTS" \
  "$DEST"

if ! codesign --verify --deep --strict "$DEST"; then
  echo "error: codesign --verify failed for $DEST" >&2
  exit 1
fi

SIGNED_ID="$(codesign -dv --verbose=4 "$DEST" 2>&1 | awk -F= '/^Identifier=/{print $2; exit}')"
if [[ "$SIGNED_ID" != "$BUNDLE_ID" ]]; then
  echo "error: codesign identifier is '${SIGNED_ID:-<empty>}', expected $BUNDLE_ID" >&2
  echo "adhoc linker-hash ids break Screen Recording TCC continuity" >&2
  exit 1
fi
echo "Signed identifier: $SIGNED_ID"

maybe_start_nativechat_remasure "$ROOT"
