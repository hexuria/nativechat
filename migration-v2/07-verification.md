# Verification: gpui-agent, not a browser

NativeChat is a GPUI desktop app. **Do not verify a phase in Chrome, Playwright-against-localhost-HTML, or a single render screenshot of a web page.** Every phase checklist is closed with **gpui-agent against the running window**.

Product builds leave the agent off. Verification builds turn it on.

## Launch

```sh
export GPUI_AGENT=1
export GPUI_AGENT_TOKEN=dev-secret
export GPUI_AGENT_SCREENSHOT_DIR=/tmp/nativechat-agent
cargo run -p nativechat --features agent
```

Wait for `gpui-agent listening` on the run log. Token must match the CLI.

CLI (release binary from the gpui-agent checkout):

```sh
export GPUI_AGENT_TOKEN=dev-secret
AGENT=/Volumes/goldcoders/.rust/cargo/git/checkouts/gpui-agent-c9dd041b93a1784f/a587716/target/release/gpui-agent
$AGENT hello
$AGENT snapshot
$AGENT click <stable-id>
$AGENT screenshot --out <name.png>   # relative to GPUI_AGENT_SCREENSHOT_DIR
```

In-process `screencapture` from the NativeChat binary needs Screen Recording TCC and often returns `screenshot_unavailable`. **Shell** `screencapture -l <CGWindowID>` works; use that if the agent screenshot is empty.

## Stable ids (today)

`app-window`, `sidebar`, `sidebar-chat-list`, `nav-new-chat`, `nav-toggle-sidebar`, `session-{id}`, `footer-theme`, `footer-account`, `footer-credentials`, `footer-profile`, `composer`.

New OpenGrok surfaces **must add ids** as they land, e.g. `footer-sign-in`, `login-email`, `login-submit`, `coworker-{id}`, `hire-name`, `hire-model`, `header-coworker`. A phase that cannot be clicked from a snapshot is not done.

## Per-phase agent loop

1. Launch with `--features agent` as above. opengrok `:1447` (and oag) already up for phases 1+.
2. `hello` then `snapshot` — tree matches the phase (signed-out shell, roster, hire, composer).
3. Drive the happy path with `click` / invoke (sign in, hire, send).
4. Screenshot after settle (~2–4s). HUD if FPS is in scope (`⌘⇧F`).
5. Hunt the other states: wrong password, empty roster, signed-out after sign-in.
6. Do not declare done on compile + one screenshot.

## Phase 1 (login) minimum

- Snapshot shows sign-in, not the old session list as the only rail.
- Click login → snapshot `/account` name in the footer.
- Wrong password → visible error, still signed out.
- Sign out → snapshot back to signed-out shell.

## Phase 2 (roster / pin)

- `GET /coworkers` rows are clickable (`coworker-{id}`).
- Hire from the UI; snapshot shows the new row after restart (server truth).
- Header is the coworker name, not a profile Select.

## Phase 3 (turn)

- Click a coworker, type in `composer`, send.
- Snapshot after stream: assistant text in the transcript.
- Quit and relaunch: same transcript still there (server), not only sqlite.

`cargo test` / `cargo check --features agent` stay required. They do **not** replace the agent loop.
