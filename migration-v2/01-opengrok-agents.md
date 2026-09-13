# How OpenGrok models an agent

Extracted from `opengrok-core` + `opengrok-server` + the web console APIs. Not Electron renderer code.

## Hire

A coworker is an event-sourced aggregate (`CoworkerEvent`). Hire records **name + model route**. The server assigns a computer. The client does not pick a box id (identity rule: payloads are overwritten, not trusted).

Hire paths:

| Door | Who | Model stored |
|------|-----|----------------|
| `POST /coworkers` `{ name, model?, templateId? }` | Console / NativeChat | Request pin, else template pin, else deployment `OG_MODEL` |
| Electron `createAgent` | Grok Bot | Historically always `OG_MODEL` (desktop ignores a model field) |
| Seam B mint | Cursor BYOS | Same default |

**Empty roster is valid.** Do not treat `[]` as failure.

Templates (`GET /templates`, admin-written): copy model, tools, approval set, points, **role** onto the hire in the same append so a half-applied template cannot exist.

## Model pin

- Stored on the coworker. A **run reads `coworker.model`**, never the process env.
- Changing `OG_MODEL` and restarting only affects **new** hires.
- `PATCH /coworkers/{id}` `{ model }` is `Repinned` — own event so a rename cannot silently change the brain.
- Catalogue: `GET /models` — server asks the gateway with the **deployment** key, returns **ids** (and a note if empty). Empty without a reason is a bug.
- `POST /models/probe` is a real billed ping, rate-limited, secrets redacted. A listed route can still fail.
- Pin grammar (OAG): `provider/model[@api|@sub]` or `oag/auto|cheap|frontier`. Not a bare upstream name, not a key.

## Role

- `RoleSet { role: Option<String> }` on the aggregate. Max **1000** characters.
- Own event (not a field on rename).
- Every turn, `persona.rs` builds **one** system message: identity, then standing role, then machine discipline. Two competing prompts is a known class of bug.
- `PATCH /coworkers/{id}` `{ role }` — absent = leave; `null` or blank = clear.

## Computer

- `ComputerAssigned { box_id, mode: dedicated | shared }`.
- Dedicated: this coworker owns the machine; stopping it is safe.
- Shared: several coworkers; one coworker must not destroy it.
- Client UI may show “Buwiz’s screen”; it must not send a box id on hire or turn.
- Phase 1 NativeChat can **omit** the computer pane. Turns still run; the server still assigns.

## Avatar

Two different things:

1. **Person** — `POST /account/profile` `{ avatarUrl }` as `data:image/…`. Email never changes.
2. **Coworker** — generative mark: `avatarShape`, `avatarColor`, optional image. Slim clients send `x-sand-slim-avatars: 1` and load `GET /avatars/{id}?v=`. Version must be set whenever an avatar exists or the route 404s.

NativeChat phase 1: account avatar. Coworker rail can use shape/color or initials until we fetch `/avatars`.

## Routine

- Separate aggregate (`Schedule`): cron + prompt + name, tied to **one coworker**.
- Server fires the turn even if NativeChat is closed. Finished run is posted **into that coworker’s transcript** as a message from the coworker.
- Pause/resume/edit keep the schedule id and history (not delete-and-recreate).
- Listed per **signed-in person**, not a global pool (privacy fix).
- Phase 2+ UI (right inspector). Do not fake cron in sqlite.

## Auto-review

- **Not** a client feature. Server policy: enabled + allow/block instruction texts.
- Two tiers: **global** (account) overridden **per coworker**. Null field = inherit.
- A model judge (`OG_AUTO_REVIEW_MODEL`) answers allow / block / ask across **all** tools.
- At most one consent **card** per tool call (remote-control gate and auto-review do not double-ask).
- `resolveAutoReviewApproval` answers the card from any device.
- Default **off**. NativeChat phase 1: ignore. Later: render the card, do not invent a local judge.

## Group (agent-to-agent)

- A group **is a coworker** with `members: Vec<CoworkerId>` (non-empty). Max **6**. Nested groups forbidden. Same member set is idempotent.
- **No model, no computer.** Members think. Orchestrator runs on the **group’s** thread; each member’s turn uses that member’s pin, key, tools, policy, spend.
- The group’s transcript is the room. Member messages are attributed to the speaking coworker (`activeRemoteMemberId` while they speak).
- Implemented on the server (`gateway/group.rs`, `tests/against_groups.rs`). Electron shared-rooms / multiplayer is a different, mostly-stubbed path — **do not build that in NativeChat**.

## One transcript, not many sessions

OpenGrok: **one transcript per (person, coworker)**. That is the chat. There is no “new chat with the same agent” aggregate.

NativeChat’s `chat_sessions` table is a list of untitled threads with no owner-agent. Mapping:

- Phase 1: **session list → coworker roster**. Opening a coworker opens that transcript.
- “Many sessions per agent” is a **new product** on the server (or a local lie). Do not add `agent_id` to sqlite and pretend it is OpenGrok.
