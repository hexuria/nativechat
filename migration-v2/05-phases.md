# Phased breakdown

Each phase leaves the app runnable. Do not start N+1 until N’s checklist is true.

**Verification is gpui-agent** against the running NativeChat window. See [07-verification.md](07-verification.md). `cargo test` is necessary and not sufficient.

## Phase 0 — Hygiene (no OpenGrok)

Already on `main`: GPUI kit, virtualized feed, TTS cleanup. No further schema work here.

## Phase 1 — Login + account  **first**

- Base URL setting
- Cookie-jar HTTP
- Login / refresh / logout
- Account Settings bound to `/account`
- Signed-out vs signed-in shells
- Remove local account password as source of truth

**Done when:** gpui-agent snapshot shows signed-in name after `click` on login, wrong password stays signed-out with an error, sign-out returns the signed-out shell. Server is `:1447`.

## Phase 2 — Roster + pin  **inference setup**

- `GET /coworkers` as the rail (not session list)
- `GET /models` + hire `POST /coworkers`
- `PATCH` model and role
- Empty roster = hire, not error
- Credentials modal unused on this path
- Profile Select in the chat header goes away (header = coworker name)

**Done when:** gpui-agent hire + snapshot shows the coworker in the rail; after quit/relaunch the same `coworker-{id}` is still there (server). Pin is a catalogue route, not a local key.

## Phase 3 — Turn  **inference**

- Select coworker → load transcript cache
- Composer → `POST /ag-ui`
- Stream assistant text; follow tail
- sqlite cache only
- Delete Gemini from the send path

**Done when:** gpui-agent send from `composer` streams assistant text in the snapshot; quit/relaunch still shows that transcript from the server.

## Phase 4 — Look/feel

- Dark-first, avatar rail, “Message {name}”
- Drop “Default” profile chrome
- Keep TTS buttons on the last chunk of a message

## Phase 5 — Computer + routines (right pane)

- Show assigned computer when the server has one (no box id on the wire from us)
- List/create/pause routines for this coworker (`Schedule`)
- Do not implement VNC until the pane is honest

## Phase 6 — Auto-review + cards

- Render pending approval cards
- `resolveAutoReviewApproval` / AG-UI answer
- Per-coworker override of global auto-review (server already stores it)

## Phase 7 — Groups (agent-to-agent)

- Hire a group coworker with members
- Group transcript as the room
- Member-attributed messages
- No nested groups, max 6

## Phase 8 — Product extensions OpenGrok does not have

Only after 1–7, and likely **server work first**:

- Many sessions/threads per coworker
- Humans + agents in one “channel”
- Invite a NativeChat user into a coworker’s room

Do not implement Phase 8 as local sqlite.

## Dependency graph

```
1 login  →  2 roster/pin  →  3 turn  →  4 look
                 ↓
            5 computer/routines
                 ↓
            6 auto-review
                 ↓
            7 groups
                 ↓
            8 new server aggregates
```
