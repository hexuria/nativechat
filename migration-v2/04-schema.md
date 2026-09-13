# Schema remodel

Sqlite stays a **cache and app prefs**, not a second OpenGrok.

## Delete or stop writing (phase 1)

| Table | Action |
|-------|--------|
| `credentials` | Stop using for turns. Migration: drop after login works, or leave orphaned and unused |
| `profiles` | Stop using. Coworker lives on the server |
| `models` | Stop seeding. Catalogue is `GET /models` (memory + optional disk cache of **ids only**) |

Do not migrate API keys onto the server from the client. Org keys are an admin job on OAG.

## Replace `chat_sessions`

Today a session is an orphan thread. OpenGrok’s unit is **coworker**.

**Phase 1 mapping**

```
sidebar row  = coworker (id, name, model, avatar…)
open chat    = that coworker’s transcript for this account
```

Local tables (optional cache):

```sql
-- prefs, not product
CREATE TABLE connection (
  key TEXT PRIMARY KEY,  -- 'base_url'
  value TEXT NOT NULL
);

-- cookie jar may live in reqwest; if we persist, encrypt at rest
CREATE TABLE auth_meta (
  account_id TEXT,
  email TEXT,
  updated_at TEXT
);

CREATE TABLE coworker_cache (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  model TEXT NOT NULL,
  role TEXT,
  visibility TEXT,
  updated_at TEXT
);

CREATE TABLE transcript_cache (
  coworker_id TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (coworker_id, entry_id)
);
```

Unknown transcript kinds **round-trip**. Do not drop columns we do not render yet.

`chat_sessions` / `chat_messages`: freeze, then drop once AG-UI cache is proven. Offer no “import old jokes into a coworker” unless we explicitly want it (out of scope).

## Do not add

- `agent_id` on `chat_sessions` to fake many chats per agent
- Local cron for routines
- Local auto-review instruction tables
- Local box ids

Those belong on the server when we take those phases.

## Auth tokens

Prefer **cookie jar** matching the console. If we later persist cookies, they are credentials: OS keychain, not plaintext sqlite.

## Settings keys to add

- `opengrok_base_url`
- last selected `coworker_id`
- theme (already)
