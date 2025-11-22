#!/bin/bash
# Combined build and fix script
# This ensures permissions are ALWAYS applied after bundling

set -e

echo "🚀 Building NativeChat with microphone permissions..."
echo ""

# Step 1: Build the bundle
echo "📦 Step 1/3: Building bundle..."
cargo bundle --release

# Step 2: Inject NSMicrophoneUsageDescription
echo "📝 Step 2/3: Injecting microphone permission..."
plutil -insert NSMicrophoneUsageDescription -string "NativeChat needs access to your microphone for voice input." \
  target/release/bundle/osx/NativeChat.app/Contents/Info.plist 2>/dev/null || \
plutil -replace NSMicrophoneUsageDescription -string "NativeChat needs access to your microphone for voice input." \
  target/release/bundle/osx/NativeChat.app/Contents/Info.plist

# Step 3: Re-sign with entitlements
echo "✍️  Step 3/3: Re-signing with entitlements..."
codesign --force --deep --sign - --entitlements ./nativechat.entitlements \
  target/release/bundle/osx/NativeChat.app

echo ""
echo "✅ Build complete with microphone permissions!"
echo ""
echo "To reset permissions and test:"
echo "  tccutil reset Microphone com.example.nativechat"
echo "  open target/release/bundle/osx/NativeChat.app"
