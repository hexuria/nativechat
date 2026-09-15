# Rebuild and relaunch NativeChat (default OpenGrok :1447).
run port="1447":
    #!/usr/bin/env bash
    set -euo pipefail
    pkill -x nativechat 2>/dev/null || true
    export OPENGROK_BASE_URL="${OPENGROK_BASE_URL:-http://127.0.0.1:{{port}}}"
    export GPUI_AGENT="${GPUI_AGENT:-1}"
    export GPUI_AGENT_TOKEN="${GPUI_AGENT_TOKEN:-dev-secret}"
    exec cargo run -p nativechat --features agent
