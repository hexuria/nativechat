# NativeChat

GPUI desktop client for [OpenGrok](https://github.com/hexuria/opengrok-server). The app is a window onto the server: login, coworkers (agents), and chat turns. It does **not** call model providers and does **not** hold API keys.

```
NativeChat (this repo)
    →  opengrok-server  (:1447)
         →  open-ai-gateway  (:29080)
              →  models
```

## Required siblings

Check these out next to each other (this repo already expects them at `/Volumes/goldcoders/OSS` in our setup):

| Repo | Role | Typical listen |
|------|------|----------------|
| [hexuria/opengrok-server](https://github.com/hexuria/opengrok-server) | Accounts, coworkers, transcripts, `POST /ag-ui` | `http://127.0.0.1:1447` |
| [hexuria/open-ai-gateway](https://github.com/hexuria/open-ai-gateway) | Model door (routes, org keys, spend) | `http://127.0.0.1:29080` |

NativeChat never talks to the gateway. The server holds `oag_live_` and asks OAG on the person’s behalf.

See `migration-v2/` for the phased plan (login → roster/pin → turn → look).

## Run

1. Start **open-ai-gateway** (see that repo’s README).
2. Start **opengrok-server** (`scripts/serve.sh` in that repo). `GET http://127.0.0.1:1447/health` should return `{"ok":true,…}`.
3. First account is CLI-only (invite-gated signup). Against the **live** database:

   ```sh
   # OG_DATABASE_URL must be the running server’s, not a leftover .env
   opengrok admin org create --name NativeChat \
     --admin-email you@nativechat.local --domain nativechat.local \
     --password 'choose-8+'
   ```

4. NativeChat:

   ```sh
   export OPENGROK_BASE_URL=http://127.0.0.1:1447   # default
   cargo run -p nativechat
   ```

   Sign in with the admin email/password. Session cookies stay in memory (sign in again after quit).

Agent-driven verification (optional):

```sh
GPUI_AGENT=1 GPUI_AGENT_TOKEN=dev-secret \
  GPUI_AGENT_SCREENSHOT_DIR=/tmp/nativechat-agent \
  cargo run -p nativechat --features agent
```

## What this app is not

- Not a Gemini/OpenAI SDK wrapper. Chat goes `POST /ag-ui` on OpenGrok.
- Not the Electron Grok Bot reconstruction. No 123-command wire.
- Local sqlite is cache/prefs, not the product. Coworkers and transcripts live on the server.
