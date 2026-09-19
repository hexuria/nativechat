#!/usr/bin/env bash
# Serve fixtures/login-demo.html on 127.0.0.1:8765 for credential.request remasure.
#
# Prefer serving **in the box**: Mac localhost is often unreachable from Box
# (egress ERR_TUNNEL is Box/OG ownership — out of scope for NativeChat).
# Copy this HTML onto the box and run the same python server there, then open
# http://127.0.0.1:8765/login-demo.html in Box Chromium.
#
# Seed Settings→Logins for origin 127.0.0.1 so Use saved is real:
#   1. Fill the demo form via the in-chat user-form (Continue).
#   2. On "Save login for 127.0.0.1 as ada?", click Save
#      (gpui-agent: click save-login-save-{entry}).
#   3. Settings → Logins shows `ada · 127.0.0.1`.
# Empty vault: Settings → Logins empty → no "Use a saved login" card;
# NativeChat POSTs credential.result missing and does not fold Used/Filled.

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/fixtures"
echo "login demo: http://127.0.0.1:8765/login-demo.html"
exec python3 -m http.server 8765 --bind 127.0.0.1
