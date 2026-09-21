#!/bin/bash
# Combined build and fix script
# This ensures permissions are ALWAYS applied after bundling

set -e

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
# shellcheck source=scripts/stop-nativechat-for-codesign.sh
source "$ROOT/scripts/stop-nativechat-for-codesign.sh"

BUNDLE_ID="dev.goldcoders.nativechat"
APP="target/release/bundle/osx/NativeChat.app"

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

echo "🚀 Building NativeChat with microphone permissions..."
echo ""

# Remasure KeepAlive must not launch NativeChat while cargo bundle / codesign
# rewrites the .app (CODESIGNING Invalid Page → SIGKILL).
stop_nativechat_for_codesign

# Step 1: Build the bundle
echo "📦 Step 1/3: Building bundle..."
cargo bundle --release

# Step 2: Inject NSMicrophoneUsageDescription
echo "📝 Step 2/3: Injecting microphone permission..."
plutil -insert NSMicrophoneUsageDescription -string "NativeChat needs access to your microphone for voice input." \
  "$APP/Contents/Info.plist" 2>/dev/null || \
plutil -replace NSMicrophoneUsageDescription -string "NativeChat needs access to your microphone for voice input." \
  "$APP/Contents/Info.plist"

# Step 3: Re-sign with entitlements.
# Always pass --identifier matching Info.plist / cargo bundle id. Ad-hoc
# signing without it stamps the linker hash (`nativechat-f14f6ba…`) and
# breaks Screen Recording TCC continuity across remasures.
echo "✍️  Step 3/3: Re-signing with entitlements..."
stop_nativechat_for_codesign
codesign --force --deep --sign "$SIGN_ID" --identifier "$BUNDLE_ID" \
  --entitlements ./nativechat.entitlements \
  "$APP"

if ! codesign --verify --deep --strict "$APP"; then
  echo "error: codesign --verify failed for $APP"
  exit 1
fi

SIGNED_ID="$(codesign -dv --verbose=4 "$APP" 2>&1 | awk -F= '/^Identifier=/{print $2; exit}')"
if [ "$SIGNED_ID" != "$BUNDLE_ID" ]; then
  echo "error: codesign identifier is '${SIGNED_ID:-<empty>}', expected $BUNDLE_ID"
  echo "adhoc linker-hash ids break Screen Recording TCC continuity"
  exit 1
fi
echo "Signed identifier: $SIGNED_ID"

maybe_start_nativechat_remasure "$ROOT"

echo ""
echo "✅ Build complete with microphone permissions!"
echo ""
echo "To reset permissions and test:"
echo "  tccutil reset Microphone $BUNDLE_ID"
echo "  open $APP"
