# NativeChat PR #67 review — `origin/main...origin/gol/user-form-transcript-chrome-4df8` (HEAD `9fd434e`)

All line numbers are against the PR-67 tree (`9fd434e`), read from a detached worktree at
`/private/tmp/claude-501/-Volumes-goldcoders-code-nativechat/a08ee87f-995c-4f9d-9b2e-ed8fed6ea266/scratchpad/nc67`.
The checkout at `/Volumes/goldcoders/code/nativechat` is at `246a33f`, which already contains the
PR-68 commits; findings below were verified against `9fd434e` only.

---

## [SEVERITY: high] [CONFIDENCE: high] `restore_user_form` can never undo an optimistic Dismiss on a card that was idle

**Where:** `src/state.rs:6754-6775` (`paint_user_form_resolution`) and `src/state.rs:6829-6846` (`restore_user_form`)

**What's wrong:**
`user_form_restore` is a `HashMap<String, FormResolution>` — it can only remember a *previous
settled* resolution. `paint_user_form_resolution` computes

```rust
let prior = (spec.resolution != Some(FormResolution::Sending))
    .then_some(spec.effective_resolution())
    .flatten();
if let Some(prior) = prior { self.user_form_restore.insert(card_key.to_string(), prior); }
```

For an **idle** card `effective_resolution()` is `None`, so `prior` is `None` and nothing is stored.
`restore_user_form` then does `user_form_restore.remove(card_key)` → `None` → the `if let` body is
skipped, so `spec.resolution` stays `Some(Dismissed)` and `user_form_resolutions[card_key]` stays
`Dismissed`. Compare `user_form_handoff_restore: HashMap<String, Option<ComputerHandoffStatus>>`
(`state.rs:1484`), which *does* store the "was absent" case correctly — the asymmetry is the bug.

**Why it matters / how it fails:**
`settle_user_form_http(Dismiss, MissingRoute | MissingEntry | MissingEntryId)` returns
`UserFormHttpSettle::Restore` (`src/opengrok/user_form.rs:466-470`), and the `Err(_)` arm for
`UserFormVerb::Dismiss` (`state.rs:7369`) also calls `restore_user_form`. Concretely: an open
`e_form` card, user clicks **Dismiss**; the card paints `Dismissed`; `POST /ag-ui/user-form/dismiss`
404s on a server without the route (or the network drops). Intended behaviour per the module doc
("A 404 that is not that sentence restores that card") is to reopen the card. Actual behaviour: the
card stays `Dismissed` forever, `has_open_user_form` now returns false, `sync_waiting_chrome` clears
"Waiting for you", and the agent is still parked on a form the person believes they answered. The
same no-op applies to the `self.opengrok.is_none()` branch at `state.rs:7250-7254`.

**Suggested fix:** make `user_form_restore` a `HashMap<String, Option<FormResolution>>`, always
insert (even for `None`), and have `restore_user_form` write back `spec.resolution = prior` and
`user_form_resolutions.remove(card_key)` when `prior` is `None`.

---

## [SEVERITY: high] [CONFIDENCE: high] An answered `credential.request` card comes back live after `follow_run` / reconcile

**Where:** `src/state.rs:7085-7141` (`answer_credential_request`), `src/state.rs:6337-6384` (`graft_user_forms`), `src/state.rs:5923-5948` (`follow_run`), `src/state.rs:7576` (`has_open_user_form`)

**What's wrong:**
In PR 67, `CredentialRequestSpec` has **no** `resolution` field and `AppState` has no
`credential_request_resolutions` map. `answer_credential_request` simply deletes the part
(`remove_credential_request_part`, `state.rs:6999-7008`) and then calls
`state.follow_run(spec.run_id.clone(), …)`. `follow_run` rebuilds parts from the run's whole event
list (`reply_from_replay` → `TurnAssembler` → `push_credential_request`,
`src/opengrok/gen_ui.rs:651-663`) and assigns `last.parts = parts.clone()` (`state.rs:5948`).
`graft_user_forms` has no `ChatPart::CredentialRequest` arm, so nothing removes it again. The same
happens on `overlay_replay_cards` → `overlay_server_cards` (`state.rs:235-247`), which re-pushes a
`CredentialRequest` whenever it is not already on the message.

**Why it matters / how it fails:**
Server emits CUSTOM `credential.request {requestId: "req-9", origin: "github.com", runId: "run-1"}`.
User clicks **Not now**. Card disappears, `post_credential_result(Denied, "req-9", …)` goes out,
`follow_run("run-1")` starts polling — and within ~300 ms the poll re-paints the same idle
"Use a saved login for github.com?" card with live buttons. The user can answer again, producing a
second `POST /ag-ui/credential/result`. Worse, `has_open_user_form` returns `true` for **any**
`ChatPart::CredentialRequest(_)` regardless of state (`state.rs:7576`), so `sync_waiting_chrome`
pins "Waiting for you" on the thread permanently and `composer_send_posts_turn` /
`has_hitl_to_interrupt` keep treating the thread as HITL-parked.

**Suggested fix:** keep a `credential_request_resolutions: HashMap<String, …>` (or at minimum a
`HashSet<String>` of answered request ids), apply it in `graft_user_forms` and
`overlay_server_cards`, and make `has_open_user_form` test `spec.is_unresolved()` rather than
matching the variant.

---

## [SEVERITY: high] [CONFIDENCE: high] `credential.request` looks the vault up by the raw wire origin, but rows are stored as eTLD+1 → a saved login always reports `missing`

**Where:** `src/state.rs:7104` (`vault.find(&spec.origin, spec.username.as_deref())`) and `src/site_login/store.rs:54-78` (`find` does `WHERE origin = ?`)

**What's wrong:**
`SiteLoginVault::save` stores `origin` exactly as handed to it, and the only producer is
`save_candidate` (`src/site_login/extract.rs:34-38`) which normalises through `registrable_origin`,
i.e. `"https://accounts.google.com"` → `"google.com"`. `answer_credential_request` passes
`spec.origin` **verbatim** from the CUSTOM frame into `find`, and `find` uses exact SQL string
equality — no `registrable_origin` and no `origins_match` (`origins_match` /
`login_matches_request` do not exist in PR 67; they arrive in PR 68).

**Why it matters / how it fails:**
User saves a Google login (row `origin = "google.com"`). Agent emits
`credential.request {origin: "https://accounts.google.com/signin", username: "ada@example.com"}`.
User clicks **Use saved login**. `find("https://accounts.google.com/signin", Some("ada@…"))` returns
`None` → `result_without_broker(true, false, false)` = `Missing` → the app POSTs `missing` and the
agent is told there is no saved login, even though there is. Any request whose `origin` is a URL,
has a `www.`/`accounts.` subdomain, or carries a port will miss.

**Suggested fix:** normalise with `registrable_origin(&spec.origin)` before `find` (as PR 68 does),
and/or make `SiteLoginVault::find` compare on a normalised column.

---

## [SEVERITY: high] [CONFIDENCE: high] "Try again" after `fill_failed` re-POSTs the form **without** the password

**Where:** `src/components/chat.rs:829-871` (`sync_user_form_fields` retention) and `src/components/user_form.rs:636-678` (`fill_failed_actions`)

**What's wrong:**
`sync_user_form_fields` only builds `needed` entries for cards where `spec.is_unresolved()`
(`chat.rs:836-838`), then `retain`s `user_form_inputs` / `user_form_textareas` to exactly those keys
(`chat.rs:868-871`). A `FillFailed` card is *settled*, so all of its `InputState` entities are
dropped the frame it collapses. `fill_failed_actions` then calls
`collect_submit_values(&spec, &inputs, &textareas, &picks, cx)` with `picks = values.clone()`, and
`values` for a user-form row (`chat.rs`, the `row.user_form` branch) deliberately **skips masked
fields** ("Secrets stay in InputState… a non-empty stub would look like a filled password").
`fill_failed_actions` also does not overlay `state.user_form_typed` (only `render_idle` does), and
the "Try again" button is gated on `can_post` only — `required_fields_filled` is not checked.

**Why it matters / how it fails:**
User types `ada@example.com` + `hunter2` into an `e_form` card and clicks Continue. Server returns
`formResolution: "fill_failed"`. Card collapses to "Not filled" with **Try again / I'll do it on the
computer / Stop for now**. Clicking **Try again** POSTs
`{entryId: "e_form", agentId: …, values: {"email": "ada@example.com"}}` — no `password` key — which
the server will fail again (or, worse, accept as a partial fill). The user has no way to retype
because the collapsed card renders no fields. Agent-typed values (`user_form_typed`) are lost the
same way.

**Suggested fix:** keep `InputState` entities alive while `effective_resolution() == Some(FillFailed)`
(or re-expand the card into `render_idle` on "Try again"), overlay `user_form_typed` in
`fill_failed_actions`, and gate "Try again" on `spec.continue_enabled(&live, server_fill)` like
Continue.

---

## [SEVERITY: medium] [CONFIDENCE: high] `same_card()` treats an equal `callId` as identity even when both cards carry *different* gateway `entryId`s — and `merge()` then clobbers the id

**Where:** `src/opengrok/user_form.rs:706-717` (`same_card`), `:767-790` (`merge`), `src/state.rs:210-226` (`overlay_server_cards`), `src/opengrok/gen_ui.rs:599-606` (`push_user_form`), `src/state.rs:6780-6799` (`paint_user_form_call_peers`)

**What's wrong:**
```rust
pub fn same_card(&self, other: &Self) -> bool {
    if self.has_gateway_entry_id() && other.has_gateway_entry_id() && self.entry_id == other.entry_id { return true; }
    if !self.call_id.is_empty() && self.call_id == other.call_id { return true; }   // ← no entry-id guard
    false
}
```
The second clause fires for `A{entry: "e_1", call: "c"}` vs `B{entry: "e_2", call: "c"}`. This
directly contradicts `completes_with` (`:727-738`), whose whole point is that two stamped cards are
siblings, not twins. `merge()` then does `if incoming.has_gateway_entry_id() { self.entry_id = incoming.entry_id; }`
— the surviving card's `entry_id` is silently replaced by the other card's.

**Why it matters / how it fails:**
A run that mints a password card `e_1` and an OTP card `e_2` under the same `callId` (the "second OTP
card" case this PR adds) collapses into a single transcript card on `push_user_form`
(gen_ui.rs:600-606) or `overlay_server_cards` (state.rs:216). Continue on the merged card POSTs
`entryId: "e_2"` with the password values, or vice versa. `paint_user_form_call_peers` and
`set_computer_handoff` (`state.rs:6721-6733`) compound this: settling one stamped card settles every
sibling that shares the `callId`, which is exactly the "resolve one → others affected" failure the
PR set out to fix (it only fixed the different-`callId` case).

**Suggested fix:** add `&& !(self.has_gateway_entry_id() && other.has_gateway_entry_id())` to the
`call_id` clause of `same_card` (i.e. call-id identity only when at least one side is call-keyed),
and make `merge` refuse to overwrite a non-empty `entry_id` with a different one.

---

## [SEVERITY: medium] [CONFIDENCE: medium] `push_user_form`'s "one unresolved card" fold can bind a gateway `entryId` to the wrong open card

**Where:** `src/opengrok/gen_ui.rs:608-631` and `src/opengrok/user_form.rs:727-738` (`completes_with`)

**What's wrong:**
`completes_with` returns `true` whenever the ids don't *both* exist on both sides:
`A{entry: "", call: "call-1"}` vs `B{entry: "e_b", call: ""}` passes both guards and returns `true`.
`push_user_form` then merges them whenever exactly one unresolved card is committed.

**Why it matters / how it fails:**
A #140-style call-only card (`call-1`, no `entryId`, Continue correctly gated) is on screen. The
server later journals a `send-message` envelope for a *different* form that carries only
`id: "e_b"` and no `callId`. `unresolved_count == 1`, so `existing.merge(spec)` runs and `call-1`'s
card acquires `entry_id = "e_b"`. Continue is now enabled, and pressing it POSTs the `call-1` form's
values under `e_b`. This is the "POST a `call-*` card under someone else's entryId" hazard the PR
explicitly guards elsewhere.

**Suggested fix:** require a positive id overlap before folding (share a `callId`, or the incoming
envelope's `entryId` already matches a stored `handoff/entry` mapping for that card), rather than
"there is exactly one unresolved card, so this must be it".

---

## [SEVERITY: medium] [CONFIDENCE: high] Blocking Keychain I/O runs on the GPUI main thread

**Where:** `src/state.rs:7036-7052` (`save_offered_login`), `:7066-7080` (`delete_site_login`), `:7099-7120` (`answer_credential_request`); `src/site_login/store.rs:93` and `:119`; `src/site_login/secrets.rs:148-179`

**What's wrong:**
All three use `cx.spawn(async move …)`, which in GPUI schedules on the **foreground** executor. The
awaited work is only partly async: `SiteLoginVault::save` calls `self.secrets.set(&id, password)?`
(a synchronous `SecFramework` write) inline, `secret_present` → `contains` → `get` is a fully
synchronous Keychain read, and `SiteLoginVault::delete` calls `self.secrets.delete(id)?`
synchronously. None of these `await`.

**Why it matters / how it fails:**
macOS Keychain calls can block for hundreds of milliseconds and can raise a modal
("NativeChat wants to use your confidential information stored in …"), e.g. after a re-sign or on a
locked keychain. While that modal is up the whole GPUI main loop is stalled — no repaint, no input,
including the transcript that just showed the save prompt. This is also the one place the perf
commit's goals are most visibly undone.

**Suggested fix:** move the `SecretStore` calls to `cx.background_executor().spawn(...)` (or make
`SecretStore` async and run the Security-framework calls on a blocking pool), then hop back with
`this.update`.

---

## [SEVERITY: medium] [CONFIDENCE: high] `SecretStore::contains` fully decrypts the password, and it is called even when the user presses "Not now"

**Where:** `src/site_login/secrets.rs:139-141` (`FileVault::contains`), `:177-179` (`KeychainSecrets::contains`), `src/site_login/store.rs:123-125` (`secret_present`), `src/state.rs:7100-7120`

**What's wrong:**
The module header says "UI never calls `SecretStore::get` to display. A.0 `credential.request` uses
`SecretStore::contains` only" — but `contains` is implemented as `self.get(id)…is_some()`
/ `matches!(self.get(id), Ok(Some(_)))`, i.e. it performs the full Keychain read and materialises the
password in a `Vec<u8>` → `String`. Separately, `answer_credential_request` runs the whole
`vault.find(...)` + `vault.secret_present(...)` block **before** it branches on `allow`
(`result_without_broker(allow, …)` is what consumes `allow`), so pressing **Not now** still reads the
secret out of the Keychain.

**Why it matters / how it fails:**
A user who declines a credential request still triggers a Keychain decrypt (and potentially the ACL
prompt), and the plaintext password briefly exists on the heap in a `String` that is dropped without
zeroization. Same for every "Use saved login" — the value is read and discarded purely to compute a
boolean. It never reaches the Box, the transcript, or a REST body (I traced every call site; the
"no password→Box" invariant holds), but the exposure window is unnecessary.

**Suggested fix:** implement `contains` with `SecItemCopyMatching` + `kSecReturnAttributes` (no
`kSecReturnData`); short-circuit `answer_credential_request` on `!allow` before touching the vault;
wrap `PendingSave::password` and any retrieved secret in a `Zeroizing<String>`.

---

## [SEVERITY: medium] [CONFIDENCE: high] `delete` removes the sqlite row before the Keychain item, so a Keychain failure orphans the secret and leaves a stale UI row

**Where:** `src/site_login/store.rs:114-121`, `src/state.rs:7062-7081`

**What's wrong:**
```rust
pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM site_logins WHERE id = ?").bind(id).execute(&self.pool).await?;
    self.secrets.delete(id)?;   // ← after the metadata is already gone
    Ok(())
}
```
`delete_site_login`'s `Err` arm only sets `site_login_error` — it does **not** call
`reload_site_logins`, so `state.site_logins` keeps the row that sqlite no longer has.

**Why it matters / how it fails:**
User clicks Delete on `ada@example.com / google.com`. sqlite row is gone; `secrets.delete` fails
(locked keychain, ACL prompt cancelled, `errSecAuthFailed`). The password stays in the Keychain under
service `ai.nativechat.site-login` / account `<uuid>` with nothing in the app that can ever name it
again, and Settings still lists a row whose Delete button will now do nothing but re-error. The
module doc already acknowledges the reinstall-orphan case; this makes it reachable in normal use.

**Suggested fix:** delete the secret first (idempotent — `KeychainSecrets::delete` already swallows
`errSecItemNotFound`), then the row; on error, `reload_site_logins` so the list matches sqlite.
`save()` has the mirror-image ordering problem (`store.rs:93` writes the secret before the row) —
a failed `INSERT` leaves an orphan under a fresh uuid.

---

## [SEVERITY: medium] [CONFIDENCE: high] A failed vault save silently destroys the password with no retry and no visible error

**Where:** `src/state.rs:7026-7054` (`save_offered_login`)

**What's wrong:**
`save_offered_login` does `self.pending_save.remove(&form_entry_id)` and
`self.remove_save_login_part(&form_entry_id)` *before* the write is attempted. On `Err(err)` it only
sets `state.site_login_error`, which is rendered **only** inside Settings → Logins
(`src/components/app_settings.rs:285-293`). There is no re-insert of the pending secret, no
re-push of the prompt, and no transcript feedback.

**Why it matters / how it fails:**
User fills a login form, Continue succeeds, "Save login for google.com as ada@example.com?" appears,
user clicks **Save**. The Keychain write fails (locked keychain / ACL denied / the vault was never
constructed because `database_service` or `config` was missing when `ensure_site_login_vault` ran).
The card vanishes as if it worked, the password is dropped, and nothing in the chat says otherwise.
The user believes the login is saved; the next `credential.request` reports `missing`.

**Suggested fix:** keep `pending_save` and the `ChatPart::SaveLogin` until the write returns `Ok`,
paint an inline error on the card, and surface `site_login_error` outside the Settings tab.

---

## [SEVERITY: medium] [CONFIDENCE: high] `box-handoff/resolve` failures are swallowed — "I'm done" paints Done while the server never resolved; and any non-200 counts as settled

**Where:** `src/state.rs:6585-6642` (`post_box_handoff_resolve`), `src/opengrok/user_form.rs:1183-1216` (`box_handoff_action_from_http`), `:1122-1127` (`box_handoff_settles_locally`)

**What's wrong:**
Three layers of optimism stack:
1. `dispatch_user_form` paints the form `Dismissed`/`Skipped`, sets the Computer card
   `Done`/`Skipped` and inserts `user_form_handoff_done` **before** the POST (`state.rs:7200-7215`).
2. `post_box_handoff_resolve`'s `Err(error)` arm does nothing except `note_signed_out` — a 500, a
   timeout, or a DNS failure leaves the optimistic paint in place with no message.
3. `box_handoff_action_from_http` maps every non-404, non-200 status to `Empty`, and
   `box_handoff_settles_locally(Empty)` is `true`. (The client happens to convert non-2xx into `Err`
   first, so this is only reachable on a 201/204, but it is wrong as written.) The
   `boxResolution` branch at `:1208-1215` is dead code — both arms return `Settled`, so **any** 200
   body is "settled".

**Why it matters / how it fails:**
Live handoff on `e_form`, sibling `e_hand`. User finishes on the Computer, clicks **I'm done**.
`POST /ag-ui/box-handoff/resolve` returns 502. The transcript shows form "Dismissed" + Computer
"Done", "Waiting for you" clears, and the agent is still blocked on an unresolved handoff with no
way for the user to retry (the buttons are gone).

**Suggested fix:** on `Err(_)` (and on `MissingRoute`), roll the card back to `ActionNeeded`, remove
it from `user_form_handoff_done`, and surface a retry; delete the dead `boxResolution` branch and
make non-2xx an explicit failure rather than `Empty`.

---

## [SEVERITY: medium] [CONFIDENCE: high] Queued box-handoff resolves can never land for a call-keyed card, are never retried, and are never bounded

**Where:** `src/state.rs:6492-6541` (`queue_pending_box_handoff` / `take_pending_box_handoff` / `flush_pending_box_handoff`), `:7219-7244` (the `ResolveHandoff` branch), `src/components/user_form.rs:251` (`let can_resolve = true;`)

**What's wrong:**
For a `call-*` card, `entry_id` is empty, so `box_handoff_post_id` always returns `None`
(`box_handoff_resolve_entry_id` needs a non-empty stored/spec `handoffEntryId`, which a locally-minted
`computer-handoff-call-*` never has). The `ResolveHandoff` branch therefore always takes the queue
path and inserts into `user_form_pending_resolves`. Nothing ever flushes it: the only flush sites are
the `Merge` arm of a submit/dismiss response and `collect_handoff_ids_and_flush`, both of which
require a `handoff_entry_id` to appear on a spec. There is no TTL, no cap, no user feedback, and no
retry on a later reconnect. Meanwhile the Skip / I'm done buttons are hard-coded live
(`can_resolve = true`).

**Why it matters / how it fails:**
On a `call-*` card the user clicks **Open the screen**, does the work, clicks **I'm done**. The card
paints "Dismissed" + Computer "Done" and a `PendingBoxHandoff` is stored forever under key
`call-9`. The agent never receives `handed_back`. This is a card that "looks actionable but silently
does nothing" server-side. Also note `queue_pending_box_handoff` inserts under *both* `card_key` and
`form_entry_id` while `take_pending_box_handoff` only removes the first one it finds — the second
copy leaks (today unreachable, because `card_key() == entry_id` whenever `entry_id` is non-empty,
but it is a latent double-POST).

**Suggested fix:** either don't queue when the card can never acquire a sibling id (and say so on the
card), or give the queue a TTL + a visible "couldn't tell the computer" state; make
`take_pending_box_handoff` remove both aliases.

---

## [SEVERITY: medium] [CONFIDENCE: high] Settings → Logins "Delete" is a one-click, unconfirmed, irreversible Keychain wipe

**Where:** `src/components/app_settings.rs:271-353` (`logins_page`, delete button at `:336-346`)

**What's wrong:**
The row's Delete button calls `state.delete_site_login(id, cx)` straight from `on_click`. Every other
destructive action in this codebase is two-click (recipe delete has `open_recipe_delete_confirm` /
`confirm_recipe_delete`; computer Update/Reset go through `open_computer_confirm`).

**Why it matters / how it fails:**
A mis-click on a 12-px-tall ghost button permanently removes the Keychain item — the password is not
recoverable from anywhere in the app, and nothing tells the user it happened beyond the row
disappearing.

**Suggested fix:** two-step confirm (reuse `recipe_delete_overlay`'s pattern) or an undo toast that
keeps the secret for a few seconds.

---

## [SEVERITY: medium] [CONFIDENCE: high] Linux/dev fallback vault: the temp file is created with umask permissions and the chmod result is discarded

**Where:** `src/site_login/secrets.rs:92-106` (`FileVault::write_map`)

**What's wrong:**
```rust
let tmp = self.path.with_extension("vault.tmp");
fs::write(&tmp, bytes).map_err(...)?;          // created 0666 & !umask → typically 0644
#[cfg(unix)] { let _ = fs::set_permissions(&tmp, Permissions::from_mode(0o600)); }   // result ignored
fs::rename(&tmp, &self.path).map_err(...)?;
```
The plaintext JSON `{id: password}` exists on disk at 0644 between `fs::write` and
`set_permissions`. If `set_permissions` fails it is silently ignored and the renamed vault is left
world-readable. The final `self.path` also never has its mode asserted (it inherits the temp file's).

**Why it matters / how it fails:**
On a shared Linux dev box, any local user can `cat ~/.local/share/nativechat/site-login.vault.tmp`
during the window, or `site-login.vault` permanently if the chmod failed (e.g. the file is on a mount
that rejects `chmod`, or the process lost the ability to change it). The module doc claims
"mode 0600" unconditionally.

**Suggested fix:** create with `OpenOptions::new().write(true).create_new(true).mode(0o600)` so the
file is never visible at a wider mode, propagate the permission error instead of `let _ =`, and use a
`.tmp` name that is not adjacent to a readable extension.

---

## [SEVERITY: medium] [CONFIDENCE: medium] Unbounded, never-pruned user-form state maps; `pending_save` (plaintext passwords) survives logout

**Where:** `src/state.rs:1469-1490` (the eight new maps) and `src/state.rs:2459+` (`logout`)

**What's wrong:**
`user_form_resolutions`, `user_form_restore`, `user_form_handoffs`, `user_form_pending_resolves`,
`user_form_handoff_done`, `user_form_computer_handoffs`, `user_form_handoff_restore`,
`user_form_typed`, `user_form_picks` and `pending_save` are all keyed by entry/call ids and are never
pruned — not on conversation switch, not on `delete_session`, not on `logout`. `remember_user_form_resolution`
writes up to three keys per spec (`entry_id`, `card_key`, `call_id`), and
`overlay_replay_cards` calls it for every replayed card on every thread visit. `logout`
(`state.rs:2459-2500`) clears `thread_activity`, `approval_decisions`, `computers`, etc. but touches
none of these — `pending_save` keeps plaintext passwords across a sign-out.

**Why it matters / how it fails:**
A long-lived session with many forms grows these maps monotonically. More importantly, a user who
fills a login form, does *not* answer the save prompt, and then signs out leaves
`hunter2` sitting in `AppState::pending_save` for the rest of the process's life — and
`inject_local_save_logins` will re-offer to save it when they sign back in as a different account.
`user_form_typed` (documented as "including secrets") has the same lifetime.

**Suggested fix:** clear the secret-bearing maps (`pending_save`, `user_form_typed`) in `logout` and
on `delete_session` / `delete_message`; bound or GC the id-keyed maps against the ids still present
in `conversations`.

---

## [SEVERITY: medium] [CONFIDENCE: medium] `answer_credential_request` leaves no transcript remnant, and posts `error` precisely when a login *is* saved

**Where:** `src/state.rs:7085-7141`, `src/opengrok/credential.rs:282-294` (`result_without_broker`)

**What's wrong:**
PR 67's flow is: fold nothing, delete the card, POST a status. `result_without_broker(true, true, true)`
returns `CredentialResultStatus::Error` by design ("broker is A.1"), which is what gets posted when
the user *does* have a matching saved login. The user sees the card vanish and nothing else.

**Why it matters / how it fails:**
User clicks "Use saved login" for a site they have saved. The card disappears, the server is told
`error`, and the agent most likely reports a failure a few seconds later. The user has no record of
what they clicked, no explanation of why it failed, and no distinction between "I declined",
"nothing saved", and "saved but the broker isn't built yet". (PR 68 adds
`CredentialRequestResolution` and the folded pill for exactly this; flagging it as a #67 gap.)

**Suggested fix:** fold the card to a settled state with copy that explains broker-off, the way
user-form Continue/Dismiss leave a remnant.

---

## [SEVERITY: low] [CONFIDENCE: high] `answer_credential_request` has no double-answer guard and resolves the conversation by "whatever is active"

**Where:** `src/state.rs:7085-7098`, `:7131-7136`

**What's wrong:**
There is no `if spec.is_settled() { return; }` guard (PR 68 adds one), and the follow-up uses
`state.active_conversation_id` rather than the conversation the request actually belongs to:
```rust
if !spec.run_id.is_empty() {
    let conversation_id = state.active_conversation_id.clone();
    state.begin_responding(conversation_id.as_deref(), "Working");
    state.follow_run(spec.run_id.clone(), conversation_id, cx);
}
```

**Why it matters / how it fails:**
Answer a credential request, switch to another bot's thread while the POST is in flight → "Working"
and the followed run's output land on the *wrong* thread's activity line. Combined with the card
reappearing (finding #2), the double-click path is trivially reachable.

**Suggested fix:** resolve the conversation by walking parts for `request_id` (the
`conversation_id_for_credential_request` helper PR 68 adds) and guard on an already-recorded answer.

---

## [SEVERITY: low] [CONFIDENCE: high] `box_handoff_action_from_http`'s `boxResolution` branch is dead code

**Where:** `src/opengrok/user_form.rs:1208-1215`

**What's wrong:**
```rust
if body.get("boxResolution").and_then(Value::as_str).is_some_and(|w| !w.is_empty()) {
    return BoxHandoffReply::Settled;
}
BoxHandoffReply::Settled
```
Both paths return the same value, so the check is inert and any 200 body — including one that
reports a refusal — is read as "settled".

**Suggested fix:** either remove the branch or make the else-arm something weaker (e.g. `Empty`) and
have the caller treat a 200 with no `boxResolution` as unconfirmed.

---

## [SEVERITY: low] [CONFIDENCE: high] `server_fill` / `user_form_verbs_available` is threaded through the whole Continue gate but ignored

**Where:** `src/opengrok/user_form.rs:744-750` (`can_post(&self, _server_fill: bool)`), `src/components/user_form.rs:83-87` and `:644-648` (two `app.read(cx)` calls that feed it)

**What's wrong:**
`can_post` ignores its argument, so `continue_enabled(spec, values, server_fill)` and
`crate::opengrok::continue_enabled` carry a parameter with no effect. The renderer still reads
`AppState` on every idle-card paint to compute it, and `ChatFeedRev` still fingerprints it
(`chat.rs`, `user_form_verbs: state.user_form_verbs_available`), causing feed rebuilds on a value
nothing consumes.

**Suggested fix:** delete the parameter and the field, or restore a real per-card meaning. Leaving a
dead flag named "verbs available" invites someone to re-add the global lock this PR removed.

---

## [SEVERITY: low] [CONFIDENCE: high] `collect_handoff_ids_and_flush` runs a full conversation walk on every SSE event

**Where:** `src/state.rs:5364` (inside the `run_turn` per-event callback) and `src/state.rs:6543-6583`

**What's wrong:**
Every SSE frame — including every text token — triggers
`state.collect_handoff_ids_and_flush(&conversation_id, cx)`, which clones all
`user_form_pending_resolves` keys, walks every message × every part of the active conversation, and
then calls `sync_waiting_chrome` (another full walk). `graft_user_forms(parts.clone())` also runs per
event.

**Why it matters / how it fails:**
This is O(messages × parts × tokens) per turn and lands in the same commit range as the perf work
(`30f14a6`) that reduced repaints to ~60 Hz. On a long thread the CPU saved by skipping `notify` is
spent here instead.

**Suggested fix:** only run it when `user_form_pending_resolves` is non-empty, or when the event's
part signature changed (the `stream_part_sig` value is already computed two lines above).

---

## [SEVERITY: low] [CONFIDENCE: medium] `stream_part_sig` cannot see an in-place part mutation, so a card update inside the 16 ms window is not painted

**Where:** `src/state.rs:674-696` (`stream_part_sig` / `stream_paint_due`), `:5339-5382`

**What's wrong:**
The signature is `(parts.len(), kind-bitflags)`. `push_user_form` / `push_credential_request` /
`push_save_login` *replace* an existing part in place when the id matches, leaving both the count and
the flags identical. If that event arrives less than `STREAM_PAINT_MIN` after the last paint,
`paint` is `false` and `cx.notify()` is skipped.

**Why it matters / how it fails:**
A `formResolution` arriving as a same-count update within 16 ms of the previous frame does not
repaint immediately. In practice this self-corrects: the post-loop `this.update(… cx.notify())` at
`state.rs:5393-5408` always fires when the stream ends, and any later event repaints. So this is a
latency wart, not a permanently stale UI — but the invariant ("a card flushes immediately") the
commit message claims is not actually enforced.

**Suggested fix:** include a cheap content hash (e.g. each card's `card_key` + resolution string) in
the signature.

---

## [SEVERITY: low] [CONFIDENCE: high] `already_saved_login` compares origins with raw `==`, inconsistent with the eTLD normalisation used everywhere else

**Where:** `src/state.rs:6914-6918`, used by `graft_user_forms:6377` and `inject_local_save_logins:6409` and `offer_save_login:6948`

**What's wrong:**
`row.origin == origin && row.username == username` — exact string equality, while the origin that
reaches it comes from `save_candidate`'s `registrable_origin` and the row's origin comes from the
same function. They agree today only because both paths normalise; any future producer that stores a
raw host makes the duplicate-suppression silently fail.

**Suggested fix:** normalise both sides (`registrable_origin`) inside `already_saved_login`.

---

## [SEVERITY: low] [CONFIDENCE: medium] `registrable_origin`'s hand-rolled suffix list collapses distinct credential scopes

**Where:** `src/site_login/origin.rs:8-35`

**What's wrong:**
`MULTI_PART_TLDS` is a 17-entry hard-coded list (no `com.cn`, `co.kr`, `com.tr`, `co.il`, `com.ar`,
`com.pl`, …) and the eTLD+1 reduction has no notion of private suffixes: `alice.github.io`,
`bob.github.io`, `x.vercel.app`, `y.herokuapp.com`, `z.web.app` all reduce to a single origin. Port
and scheme are also dropped, so `http://localhost:3000` and `https://localhost:8443` are one scope.

**Why it matters / how it fails:**
In PR 67 the only consumer is `save_candidate` (metadata) and `already_saved_login`, so the blast
radius is "a login saved for `alice.github.io` is treated as already-saved when a form on
`bob.github.io` appears" and `find()` can return the wrong row for a `credential.request` whose
origin happens to be pre-normalised. It becomes a genuine cross-site credential-scoping bug once
PR 68's `origins_match` / the A.1 broker start using it to decide *which secret* to hand over.

**Suggested fix:** use the `publicsuffix`/`psl` crate (with the PRIVATE section) rather than a hand
list, and keep the scheme+port in the stored origin.

---

## [SEVERITY: low] [CONFIDENCE: medium] Full-bleed Settings overlay paints over the macOS title-bar strip

**Where:** `src/components/layout.rs:485-500` (`app_settings_overlay`, `.absolute().inset_0().occlude().bg(background)`) and `src/components/app_settings.rs:175-198` ("← Back to app" at `py(px(16.))`)

**What's wrong:**
The overlay is now a child of the outer `v_flex().size_full().relative()`, i.e. `inset_0` starts at
`y = 0` and covers the whole `TITLE_BAR_H = 52` band. The settings nav's first row ("← Back to app")
sits at roughly `y = 16`, which is inside the traffic-light band.

**Why it matters / how it fails:**
With the window's transparent titlebar, the native traffic lights render above the GPUI layer, so
they stay clickable — but "← Back to app" is drawn underneath them and is partly obscured/unclickable
at the left edge. (PR 68's `fix(settings): traffic-light inset` is the corroborating change.)

**Suggested fix:** inset the settings nav by `TITLE_BAR_H` (or `TRAFFIC_LIGHT_INSET`) at the top, or
anchor the overlay below the title bar.

---

## [SEVERITY: low] [CONFIDENCE: medium] Voice waveform freezes during silence instead of scrolling flat

**Where:** `src/components/voice_wave.rs` (the `idle` branch added in `30f14a6`)

**What's wrong:**
When `current_amp`, `smoothed_amp` and `current_peak` are all `< 0.0005`, `scroll_phase` and
`animation_offset` stop advancing and no bar is pushed to `history`.

**Why it matters / how it fails:**
A user who pauses mid-dictation sees a completely static waveform rather than a flat scrolling line,
which reads as "the mic stopped". The same commit's `circular_voice_viz` change freezes the rotation
at `MIN_VELOCITY` for the same reason.

**Suggested fix:** keep pushing zero-amplitude bars (cheap) and gate only the `cx.notify()`, or make
the idle state visually explicit.

---

## [SEVERITY: low] [CONFIDENCE: high] `place_hitl_cards_in_document_order` hoists HITL cards above the first screenshot, reordering unrelated text

**Where:** `src/opengrok/gen_ui.rs:51-74`

**What's wrong:**
All HITL cards are extracted and re-inserted at the index of the *first* `ChatPart::Screenshot`.
Given `[Text("A"), Screenshot, Text("B"), UserForm]` the result is
`[Text("A"), UserForm, Screenshot, Text("B")]` — the form jumps above both the screenshot and
`Text("B")`.

**Why it matters / how it fails:**
This is deliberate (the `overlay_mounts_the_open_form_above_screenshots_not_at_the_bottom` test locks
it in) and it does give stable hide→reshow ordering, but the side effect is that narration written
*after* a form is displayed *before* it. On a turn where the bot says "I've opened the page" →
screenshot → "now enter your password" → form, the user reads the form before the sentence
introducing it.

**Suggested fix:** anchor each HITL card to its own position in the event stream rather than to a
single global insertion point.

---

## [SEVERITY: low] [CONFIDENCE: high] `agent/host.rs` defines the new commands but registers no `invoke` names for them

**Where:** `src/agent/host.rs:139-176` (the `Command` variants) and `:1982-2081` (`invoke`)

**What's wrong:**
`UserFormContinue`, `UserFormDismiss`, `UserFormOpenScreen`, `UserFormSetField`,
`ComputerHandoffDone/Skip/TakeOver`, `SaveLogin`, `SkipSaveLogin`, `DeleteSiteLogin` and
`AnswerCredentialRequest` are all reachable only via `click`/`key` target matching
(`:1582-1660`); the `invoke` table stops at `approval.answer` and returns
`unknown invoke '<name>'` for everything new.

**Why it matters / how it fails:**
`gpui-agent invoke user-form.continue …` (the documented dev-loop driver in the project memory)
fails; a driver must first snapshot the tree and synthesise a click on `user-form-continue-<key>`.
PR 68 fixes this for `AnswerCredentialRequest` specifically.

**Suggested fix:** add the invoke names alongside the click targets, as the other commands have.

---

## [SEVERITY: low] [CONFIDENCE: high] `state.rs` is now 10.8k lines with ~1.2k lines of user-form / site-login logic inlined in `impl AppState`

**Where:** `src/state.rs:6337-7381` (the whole user-form + site-login block)

**What's wrong:**
Twenty-two private methods (`graft_user_forms`, `user_form_mut`, `user_form_context`,
`box_handoff_post_id`, `queue/take/flush_pending_box_handoff`, `collect_handoff_ids_and_flush`,
`post_box_handoff_resolve`, `remember_*`, `set/restore_computer_handoff`, `paint_*`,
`restore_user_form`, `dispatch_user_form`, plus the whole site-login group) and ten new `AppState`
fields live in one `impl` block next to sidebar toggles, TTS and the lightbox. `user_form_mut`,
`user_form_context`, `box_handoff_post_id`, `credential_request_spec`,
`remove_credential_request_part` and `remove_save_login_part` are six near-identical
"walk every conversation × message × part" loops.

**Suggested fix:** extract a `UserFormChrome` (the seven id-keyed maps + `dispatch_user_form` and its
paint/restore helpers) and a `SiteLoginState` (vault, `site_logins`, `pending_save`,
`site_login_error`) as their own types with their own unit tests, and give the part-walk one shared
iterator helper.

---

## [SEVERITY: low] [CONFIDENCE: medium] Missing test coverage for the id-kind rules that actually fail

**Where:** `src/opengrok/user_form.rs:1475+`, `src/state.rs:8380+`

**What's wrong:**
The suite covers the happy paths well (`open_handoff_does_not_post_form_entry_id`,
`bind_call_peers_settles_the_call_twin_not_a_later_otp`, `call_keyed_dismiss_settles_locally_and_survives_restore`,
`skip_without_sibling_id_queues_instead_of_posting_form_id`). It does **not** cover:
* Dismiss → `MissingRoute` on a card that was idle (finding #1 — `UserFormHttpSettle::Restore` is
  tested as a pure function but never end-to-end through `restore_user_form`).
* `same_card` with two different `entryId`s sharing a `callId` (finding #5).
* `completes_with` folding an entry-only envelope onto a call-only card (finding #6).
* Answer-then-`follow_run` for `credential.request` (finding #2) — `graft_keeps_folded_credential_request_across_sse`
  exists only in PR 68.
* Any `SecretStore` error path (`store::delete` mid-failure, `save` mid-failure); the only vault
  tests use `MemorySecrets`, which never fails.

**Suggested fix:** add a failing-secret-store test double and the four state-machine cases above.

---

## Categories that turned up nothing

* **Password reaching the Box / AG-UI content / a ChatPart / sqlite / the transcript.** I traced every
  consumer of `PendingSave.password`, `UserFormValues`, and `SecretStore::get`. The only writer of a
  password is `SiteLoginVault::save`; the only reader is `secret_present` (finding #8, an
  unnecessary decrypt, but the value is dropped). `saved_parts` (`state.rs:111-147`) reduces every
  HITL card to a paragraph break and `restored_parts` only reconstructs `Text`/`Screenshot`, so
  nothing card-shaped reaches sqlite. `plain_text` (`gen_ui.rs:973-989`) never renders card values.
  `submit_request_body` is the only place values leave the process, and `send_json`
  (`client.rs:260-300`) does not log bodies. `UserFormField`, `UserFormValues`, `PendingSave`,
  `SaveLoginSpec` and `CredentialRequestSpec` all have redacting or value-free `Debug`. There is no
  fill-into-Box function; `SESSION_BROKER_AVAILABLE` is `false` and `result_without_broker` is
  exhaustively tested never to return `Filled`.
* **Panics / `unwrap` / `expect` on user-reachable paths.** The only non-test `unreachable!`s are the
  two in `dispatch_user_form` (`state.rs:7259`, `:7272`), and both are genuinely unreachable — the
  `ResolveHandoff` arm `return`s at `:7243` before the spawn. No new production `unwrap`/`expect`/
  `panic!` in the diff.
* **The double-lease fix (`67f1ec3`).** I checked every path that can run during the Computer
  window's first paint: `ComputerScreen::new` takes `attention` by value, the `cx.observe` closure
  only does `cx.notify()` + a deferred `cx.spawn` with a `WeakEntity`, `Render` reads only
  `self.handoff_attention`, `computer_attention_banner` takes `cx: &App` and never reads `AppState`,
  and `start_teaching` / `toggle_teaching` touch no `AppState`. The remaining `self.app.read(cx)` at
  `computer_screen.rs:479` is inside `save_recipe`, an action handler. `push_computer_window_attention`
  snapshots `computer_window_attention()` before the `handle.update` loop. I found no second
  first-paint path that reads `AppState`.
* **Migration handling for `site_logins`.** `migrations/20260918010000_site_logins.sql` is additive,
  `CREATE TABLE IF NOT EXISTS` + `CREATE UNIQUE INDEX IF NOT EXISTS`, no password column, and the
  store test asserts the exact column list. The only latent issue is that `save()` reuses the id from
  `find()` and relies on `ON CONFLICT(id)`, so two concurrent saves of the same `(origin, username)`
  would hit the `(origin, username)` unique index instead — see the ordering note in the `delete`
  finding.
* **`Dismissed` vs `Skipped` vs `Done` legibility.** `FormResolution::pill/body` and
  `ComputerHandoffStatus::pill/body` give each state a distinct word and sentence, only `Submitted`
  gets a check icon, and `FillFailed` is the only one with a danger colour. That reads correctly.
