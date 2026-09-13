# Priority: login and inference

This is the first slice. Everything else is decoration until NativeChat is a window onto a session and a turn.

## Stack

```
NativeChat (GPUI)
    │  HTTPS JSON + SSE
    ▼
opengrok-server :1447
    │  account, coworker, transcript, policy, harness
    │  oag_live_ key stays here
    ▼
open-ai-gateway :29080
    │  route, credential pool, spend
    ▼
providers
```

NativeChat **must not** call Gemini/OpenAI/Anthropic, **must not** store `api_key` rows, **must not** send `OG_OAG_API_KEY`.

## 1. Connection

- Setting: OpenGrok **base URL** (dev default `http://127.0.0.1:1447`).
- Health: `GET /health` → `{ ok: true, … }`. Fail in a sentence, not a spinner forever.
- Electron forbids loopback except `127.0.0.1`; a native app may use LAN or loopback.

## 2. Login (account)

Console contract (already JSON):

| Call | Body / notes |
|------|----------------|
| `POST /auth/login` | `{ email, password }` → **httpOnly cookies** (`og_access`, refresh). JSON body is `{ email }` only |
| `POST /auth/refresh` | Cookie. Rotates the pair; old refresh dies |
| `POST /auth/logout` | Clears cookies |
| `GET /account` | Me. Bearer **or** cookie |
| `POST /account/profile` | name, avatar data URL. **Never email** |
| `POST /account/password` | `{ currentPassword, newPassword }` |

Signed-in means **both** access and refresh are held. Unknown plan strings → treat as Ultra on the desktop; NativeChat can ignore plan until spend UI.

**Native HTTP:** `reqwest` cookie store is enough. Do not invent a third login. Do not use Electron PKCE `/loginDeepControl` unless we need that door.

Disabled account: login refuses **distinguishably**. Unverified email is a state.

Account Settings in NativeChat: rebind to `/account`. Drop local name/email/password writes.

## 3. Catalogue and pin

| Call | Why |
|------|-----|
| `GET /models` | Signed-in. Ids only. Empty list **with a note** |
| `POST /models/probe` | Optional Test button. Billed, throttled |
| `GET /templates` | Hire picker |
| `POST /coworkers` | `{ name, model?, templateId? }` |
| `GET /coworkers` | Roster `may_use` |
| `PATCH /coworkers/{id}` | `{ model?, role?, visibility? }` |

Replace Profile Settings with: name, role, model datalist, template, Test. No credential picker.

## 4. A turn

**Door for NativeChat:** `POST /ag-ui` (MIT events, already the “other clients” door). Do **not** implement 123 `POST /api/<cmd>` verbs.

Flow the server already does:

1. Overwrite identity from the session (who is asking).
2. Load coworker → **that** pin is the model.
3. Policy + spend **before** the model call.
4. Durable harness; tool consent **suspends**, does not fail the run.
5. Every completion exits OAG.
6. Transcript appends; stream events back.

NativeChat today: `GeminiProvider` + env/credential key, local `chat_messages` insert. That path is deleted from the turn, not wrapped.

Streaming: consume AG-UI events; append to a **cache** of the server transcript; follow tail. Do not treat sqlite as source of truth.

## 5. What “attach a model to an agent” means

Not “pick Gemini on the profile.” It is:

1. Sign in.
2. `GET /models` (and optionally probe).
3. Hire with `model` or `PATCH` later (`Repinned`).
4. Next `POST /ag-ui` for that coworker uses the stored pin.

If the catalogue is empty, show the **note**, do not hire a silent default without saying so.

## 6. Suggested first PR sequence (inside this priority)

1. Config: `opengrok_base_url`. No keys.
2. HTTP client with cookie jar; login / refresh / logout; signed-out shell.
3. Account Settings → `/account`.
4. Roster `GET /coworkers` in the sidebar (replace session list visually).
5. Hire + pin + role (`POST` / `PATCH`). Drop credentials modal from the turn path.
6. `POST /ag-ui` one user message, stream assistant text, cache transcript.

Stop there. Computer, routines, auto-review, groups, TTS-on-server wait.

Close each of 1–6 with the **gpui-agent** loop in [07-verification.md](07-verification.md), not a browser.
