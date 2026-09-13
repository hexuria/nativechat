# Vocabulary

Use OpenGrok words in NativeChat v2. NativeChat’s current names collide.

| Term | Meaning | NativeChat today | After |
|------|---------|------------------|--------|
| **Account** | Signed-in person (email, name, avatar, org, session) | Fake local Account Settings | Server `/account` |
| **Session (auth)** | Access + refresh pair. Exists only when **both** are held | Missing | Cookie jar or Bearer from `/auth/login` |
| **Coworker / agent** | Hired being: name, standing **role**, **model pin**, computer, visibility, optional group members | Missing. Closest is `profiles` (credential + models) | Roster row. One being. |
| **Profile (person)** | Account name/avatar. Email does not change | Account Settings → Profile | Keep as **account**, not agent |
| **Profile (LLM)** | Does not exist in OpenGrok | `profiles` + `credentials` tables | **Delete.** It is a coworker |
| **Model pin** | Gateway route the coworker thinks through | `text_model_id` + which API key | `coworker.model` |
| **Role** | Standing paragraph (≤1000 chars) composed into **one** system message every turn | Missing | `PATCH /coworkers/{id}` `{ role }` |
| **Computer** | Box/Docker assigned by the server | Missing | Later UI. Client never sends a box id |
| **Routine** | Cron + prompt on a coworker. Server fires even if the laptop is off | Missing | Later. `Schedule` aggregate |
| **Avatar** | Two kinds: **person** (`account.avatarUrl` data URL) and **coworker** (`avatarShape` / `avatarColor` / `GET /avatars/{id}`) | Local profile image-ish | Person first; coworker mark later |
| **Auto-review** | Server policy: what tools may this coworker use, judged by a model. Global, overridable per coworker | Missing | Later. Not a client LLM |
| **Transcript** | Server event log for **this person × this coworker** | Local `chat_sessions` + `chat_messages` | Server is truth; sqlite is cache |
| **Chat session (product wish)** | Many threads per agent | `chat_sessions` rows, not tied to a profile | **OpenGrok does not have this.** Phase 1: one chat = one coworker |
| **Group** | A coworker whose **members** think. No model, no computer of its own. Max 6 members | Missing | Later. `createGroup` / `setGroupMembers` |
| **Channel / users in a room** | Slack-like room with people + agents | Missing | **Not on the server as that.** See 06 |

If a sentence uses “profile” without saying person vs LLM, rewrite it.
