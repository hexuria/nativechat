#!/usr/bin/env bash
# Stop remasure/e2e KeepAlive before copy or `codesign --force`.
#
# A LaunchAgent that starts NativeChat while pages are being rewritten
# SIGKILLs with CODESIGNING Invalid Page. Ignore missing labels/processes.
#
# Labels match Master Tester remasure KeepAlive (gui/$UID/…).

stop_nativechat_for_codesign() {
  if [[ "$(uname -s)" != Darwin ]]; then
    return 0
  fi
  local uid
  uid="$(id -u)"
  local labels=(
    ai.nativechat.remasure
    dev.hexuria.nativechat-remasure
    dev.hexuria.nativechat-e2e
  )
  local label
  for label in "${labels[@]}"; do
    launchctl bootout "gui/${uid}/${label}" 2>/dev/null || true
  done
  local plist
  for plist in \
    "${HOME}/Library/LaunchAgents/ai.nativechat.remasure.plist" \
    "${HOME}/Library/LaunchAgents/dev.hexuria.nativechat-remasure.plist" \
    "${HOME}/Library/LaunchAgents/dev.hexuria.nativechat-e2e.plist"; do
    if [[ -f "$plist" ]]; then
      launchctl bootout "gui/${uid}" "$plist" 2>/dev/null || true
    fi
  done
  pkill -x NativeChat 2>/dev/null || true
  pkill -x nativechat 2>/dev/null || true
}

maybe_start_nativechat_remasure() {
  local root="$1"
  local starter="${root}/scripts/start-nativechat-remasure.sh"
  if [[ -x "$starter" ]]; then
    "$starter"
  fi
}
