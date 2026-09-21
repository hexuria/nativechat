#!/usr/bin/env bash
# Sign the development binary with a stable identity.
#
# The keychain does not trust a program by its path: it trusts the signature. An unsigned
# build is a different program to it every time it is compiled, so the first read of a
# saved password after every `cargo build` puts the "enter the login keychain password"
# sheet up, whatever Touch ID already said. Signing each dev build with one identity keeps
# the designated requirement the same from build to build, so the permission the person
# gives once carries over.
#
#   scripts/sign-dev.sh [path-to-binary]
#
# The identity defaults to the team's Developer ID, the same one the shipped app uses, so
# the permission carries over to it as well. Override with NATIVECHAT_SIGN_ID, or set it to
# "-" for an ad-hoc signature (back to a sheet after every build).
set -euo pipefail

BIN="${1:-target/debug/nativechat}"
IDENTITY="${NATIVECHAT_SIGN_ID:-Developer ID Application: Goldcoders Corp (5KZ8MD34QW)}"
BUNDLE_ID="${NATIVECHAT_BUNDLE_ID:-dev.hexuria.nativechat}"

if [[ "$(uname -s)" != Darwin ]]; then
  exit 0
fi

if [[ ! -f "$BIN" ]]; then
  echo "sign-dev: no binary at $BIN" >&2
  exit 1
fi

if [[ "$IDENTITY" != "-" ]] && ! security find-identity -v -p codesigning | grep -qF "$IDENTITY"; then
  echo "sign-dev: no code-signing identity called \"$IDENTITY\"; signing ad-hoc instead" >&2
  echo "sign-dev: the keychain will ask again after each build until a real identity is used" >&2
  IDENTITY="-"
fi

# A fixed identifier, so a rename of the binary does not change what the keychain trusts.
codesign --force --sign "$IDENTITY" --identifier "$BUNDLE_ID" "$BIN"
codesign --verify --verbose=2 "$BIN" 2>&1 | tail -1
