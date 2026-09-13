# Out of scope (and traps)

## Do not copy

- The 123 Electron `POST /api` commands (`opengrok-wire`). NativeChat is a new client.
- Embedding open-ai-gateway in the GPUI process.
- Cursor / Codex / Claude BYOS “Router” (Seam B). This app is OpenGrok Server only.
- Vendor renderer / Grok Bot assets (`LEGAL.md` on the server repo).

## OpenGrok does not have (do not fake in sqlite)

| Wish | Reality |
|------|---------|
| Many chat sessions per agent | One transcript per (account, coworker) |
| Slack-style channels with users | Groups are **coworkers with member agents**, not people rooms |
| Add a human user to a channel | Org **visibility** + sharing “may talk”; not a channel membership table in the client |
| Client-named computer | Server assigns |
| Client-held provider keys | Org/gateway only |

If we want those wishes, they are **opengrok-server** features, then NativeChat UI.

## Later, not never

- Server-side TTS
- Computer streaming / VNC
- Spend meters in the rail
- Bot keys (`POST /coworkers/{id}/keys`) for MCP/automation
- Slim avatars HTTP

## TTS

Keep current local native/AI TTS until Phase 3 is boring. Do not couple inference migration to speechify.

## Tests that must not survive the inference cut

`tests/profile_selection_tests.rs` reimplements `AppState` profile logic. When `profiles` die, delete or replace that file. Do not keep a double that stays green while send() talks to OpenGrok.
