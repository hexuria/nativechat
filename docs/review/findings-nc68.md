# NativeChat PR #68 review — findings

Range reviewed: `origin/gol/user-form-transcript-chrome-4df8...246a33f` (30 files, +3302/-491).
Everything below was checked against the source, not the commit messages. Where the claim is about
`registrable_origin` / `url::Url` behaviour I compiled the actual algorithm against the repo's
`url` rlib and ran it (probe output quoted inline).

Sorted most severe first.

---

## [SEVERITY: high] [CONFIDENCE: high] eTLD+1 matcher collapses multi-label public suffixes, so a saved login for one site matches a different site

**Where:** `/Volumes/goldcoders/code/nativechat/src/site_login/origin.rs:26-34` (`registrable_origin`), used by `origins_match:58` and `login_matches_request:67`

**What's wrong:**
`MULTI_PART_TLDS` is a 17-entry hand list (`co.uk`, `com.au`, …). Any public suffix not on that list
is treated as a plain two-label eTLD, so every host under it collapses to the same "registrable
origin". `origins_match` then reports a hit across unrelated sites.

I ran the exact function body:

```
        foo.com.cn -> Some("com.cn")
       evil.com.cn -> Some("com.cn")
   alice.github.io -> Some("github.io")
     bob.github.io -> Some("github.io")
example.s3.amazonaws.com -> Some("amazonaws.com")
      x.vercel.app -> Some("vercel.app")

bank.com.cn ~ evil.com.cn                = true
alice.github.io ~ bob.github.io          = true
x.vercel.app ~ y.vercel.app              = true
site.co.kr ~ other.co.kr                 = true
example.s3.amazonaws.com ~ other.s3.amazonaws.com = true
```

Missing from the list at minimum: `com.cn`, `co.kr`, `com.tr`, `co.il`, `com.tw`, `com.my`,
`com.ph`, `co.th`, `com.ar`, `co.id`, `org.nz`, `govt.nz`, `ac.jp`, `edu.au`, plus the private
suffixes that matter most for a credential gate (`github.io`, `vercel.app`, `herokuapp.com`,
`pages.dev`, `netlify.app`, `blogspot.com`, `s3.amazonaws.com`).

**Why it matters / how it fails:**
The vault has one row: `bank.com.cn / ada`. The agent emits
`credential.request {origin: "https://evil.com.cn/login"}`. `keep_credential_request_offer` sees a
"match", so NativeChat paints **"Use a saved login for evil.com.cn?"**. If the user clicks Use saved,
`answer_credential_request` POSTs `credential.result` with `credentialId` = the *bank* row's uuid
(`state.rs:7585`), telling the server which stored credential was selected for a site it does not
belong to. Today A.0 has no broker so no password moves, but this is the exact gate A.1 will hang
the session broker off — at that point it is a cookie/session handover to the wrong origin. Even
today it leaks "this user has a saved login for the bank" to whoever controls `evil.com.cn` content
that steers the agent.

**Suggested fix:**
Use a real public-suffix list (`publicsuffix` / `psl` crate, or embed the ICANN + PRIVATE sections
of the PSL). Failing that, require an exact host match plus an explicit registrable-domain
allowlist, and never fall through to "last two labels" for a suffix the app does not know.

---

## [SEVERITY: high] [CONFIDENCE: high] A broken vault reads as an empty vault: `site_logins_ready` is set even when the sqlite list errors

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:7303-7311`

```rust
match list {
    Ok(rows) => { state.site_logins = rows; state.site_login_error = None; }
    Err(err) => state.site_login_error = Some(err.to_string()),
}
state.site_logins_ready = true;   // <- unconditional
```

**What's wrong:**
`site_logins_ready` is the flag whose entire purpose (per `keep_credential_request_offer`'s doc) is
"do not treat an empty in-memory vec as *no logins*". It is flipped to `true` on the error branch
too, while `site_logins` keeps its previous (usually empty) contents.

**Why it matters / how it fails:**
The sqlite read fails once at startup — the data volume is not mounted yet, the db is locked by a
migration, disk full. `site_logins` stays `[]`, `site_logins_ready` becomes `true`. Every
`credential.request` from then on is auto-missed: the card is never painted
(`graft_user_forms:6613`), `credential.result {status: missing}` is POSTed behind the user's back,
and the agent is told "this user has no saved login for facebook.com" when in fact the user has one
and the app simply could not read it. The only trace is `site_login_error`, rendered in
Settings → Logins and nowhere else. This is precisely the "can a user tell a missing vault from a
broken one" case, and the answer is no.

**Suggested fix:**
Set `site_logins_ready = true` only on `Ok`. On `Err`, leave it false (so cards keep being offered
and the user can still answer), retry the list, and surface the vault error on the credential card
itself rather than only in Settings.

---

## [SEVERITY: high] [CONFIDENCE: high] Saving or deleting any login sweeps every conversation and auto-misses stale historical credential cards

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:7312-7319` (the sweep loop in `reload_site_logins`), `state.rs:6685-6713` (`sweep_empty_vault_credential_requests`), `state.rs:6744-6771` (`auto_miss_empty_vault_credential_request`)

**What's wrong:**
`reload_site_logins` — which runs after every `save_offered_login` and every `delete_site_login` —
walks **all** conversations and auto-misses any `CredentialRequest` part that no longer matches a
row. `should_auto_miss_credential_request` has no notion of whether the request is still live: it
only checks `is_unresolved()`, the auto-missed set, and the resolutions map (both in-memory only).

**Why it matters / how it fails:**
1. Last week thread A had a `credential.request` for `bank.com` that the user never answered. It
   matched a saved row, so `overlay_replay_cards` re-paints it whenever the thread is reopened.
2. Today the user opens Settings → Logins and deletes the `bank.com` login.
3. `delete_site_login` → `reload_site_logins` → sweep over every conversation → thread A's ancient
   card now has no match → `auto_miss_empty_vault_credential_request` fires:
   - POSTs `credential.result {status: missing}` for a `requestId` the server abandoned days ago,
   - inserts the *old* run into `empty_vault_continue_runs`,
   - `begin_responding(thread A, "Working")` — thread A now shows a working spinner out of nowhere,
   - spawns `follow_run(old_run_id)`, which replays the finished run and overwrites the current
     message content/parts with the replay (`state.rs:6175-6176`) and can `persist_assistant_reply`
     it a second time.

The user deleted one password and three unrelated threads started "working".

**Suggested fix:**
Restrict the sweep to requests the app believes are live (e.g. the run is in `live_turns` or
`parked_hitl_runs`, or the request arrived this session), and never start a `follow_run` for a run
the app is not currently tracking. Persist the answered/auto-missed request ids so a relaunch does
not re-decide history.

---

## [SEVERITY: high] [CONFIDENCE: high] After a relaunch, an unmatched live `credential.request` is dropped silently with no `missing` POST — the agent parks forever

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:5090` (`overlay_replay_cards` calls `graft_user_forms`, not `graft_turn_parts`)

**What's wrong:**
The retain rule that drops an unmatched Use-saved card lives in `graft_user_forms:6613`. The
compensating `missing` POST lives in `graft_turn_parts:6634`, which wraps it. `overlay_replay_cards`
— the path that repaints a thread from server replay when you reopen it — calls the bare
`graft_user_forms`.

**Why it matters / how it fails:**
The agent asks for a saved login for `facebook.com`; the vault is empty. The user quits NativeChat
before answering (or the app crashes). On relaunch they reopen the thread: `overlay_replay_cards`
rebuilds the parts, the card is dropped because there is no matching row, and **nothing is POSTed**.
The server-side run stays parked on `credential.request` indefinitely, and the user sees a thread
that just stops — no card, no pill, no Waiting chrome, no way to answer. `credential_auto_missing`
is in-memory so the app has no memory that it already decided; it simply never decides.

**Suggested fix:**
Route `overlay_replay_cards` through `graft_turn_parts` (it already has the conversation id), or
lift the drop-rule + POST into one function so the retain can never happen without the result.

---

## [SEVERITY: high] [CONFIDENCE: medium] Single-flight refresh clears the slot unconditionally, so a late waiter can wipe a *newer* in-flight refresh

**Where:** `/Volumes/goldcoders/code/nativechat/src/opengrok/client.rs:410-434`

```rust
let outcome = shared.await;
{
    let mut flight = self.refresh_flight.lock().await;
    *flight = None;          // <- clears whatever is there, not "my" flight
}
```

**What's wrong:**
Every waiter on a shared flight sets the slot to `None` after the await, without checking that the
value in the slot is still the flight it joined. There is no identity check (`Arc::ptr_eq` /
generation counter).

**Why it matters / how it fails:**
Tasks A and B both join flight F1. F1 resolves; both are woken.
- A takes the mutex, sets `*flight = None`, releases.
- Task C (a third request bursting after idle) takes the mutex, sees `None`, creates F2, stores it,
  releases, and starts POSTing `/auth/refresh`.
- B — still holding the completed outcome — takes the mutex and sets `*flight = None`, **evicting
  F2 while its POST is in flight**.
- Task D arrives, sees `None`, creates F3 and POSTs `/auth/refresh` a second time concurrently.

F2 and F3 are two simultaneous `/auth/refresh` calls. OpenGrok rotates one-time today, so the loser
gets 401, `refresh_once` checks `has_fresh_access()` — which is a race against whether F2's
`set-cookie` has landed in the jar yet — and on the losing side calls `clear_session()`. That is the
SignedOut banner this commit exists to prevent. The window is small but a burst is exactly the
scenario (`concurrent_ensure_fresh_token_joins_one_refresh` only exercises two callers with no third
arriving mid-clear, so the test does not cover it).

**Suggested fix:**
Store `(generation: u64, shared)` and only clear when the generation matches the one you awaited,
or hold the flight in an `Arc` and compare with `Arc::ptr_eq`.

---

## [SEVERITY: high] [CONFIDENCE: high] `refresh()` now short-circuits `Ok(())`, defeating the 401 retry in `send_json`

**Where:** `/Volumes/goldcoders/code/nativechat/src/opengrok/client.rs:413-415` (`else if self.has_fresh_access() { return Ok(()); }`), consumed at `client.rs:316` and `state.rs:1956`

**What's wrong:**
`has_fresh_access()` is `access_token().is_some() && !needs_refresh(token_seconds_left())` — i.e.
"there is a cookie and its `exp` is more than 30 s away". When that is true, `refresh()` returns
`Ok(())` without doing anything. Before this PR `refresh()` always POSTed `/auth/refresh`.

**Why it matters / how it fails:**
`send_json`'s recovery path is:

```rust
if response.status() == UNAUTHORIZED && !path.starts_with("/auth/") {
    if self.refresh().await.is_ok() { response = build(self.access_token()).send().await?; }
    if response.status() == UNAUTHORIZED { return Err(signed_out_error(...)); }
}
```

A 401 whose cause is *not* expiry — the session was revoked server-side, OpenGrok restarted with a
new signing key, the user's clock is skewed fast, the token was issued for a different org — leaves
the jar holding a cookie whose `exp` still looks healthy. `refresh()` returns `Ok(())` immediately,
the retry re-sends the *identical* bearer, gets 401 again, and the user is signed out. The one
self-heal the app had is now a no-op in exactly the cases it was for. `restore_session`
(`state.rs:1956`) has the same shape and is also neutered.

**Suggested fix:**
Only take the `has_fresh_access()` shortcut for the *proactive* `ensure_fresh_token` caller (pass a
`force: bool`, or expose `refresh_forced()`); the 401-retry path must always POST.

---

## [SEVERITY: medium] [CONFIDENCE: high] The card-paint gate reads sqlite only; the answer path also demands the Keychain secret, so a Keychain miss looks like "None saved"

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:6626-6630` (`has_matching_saved_login`, sqlite metadata only) vs `state.rs:7570-7579` (`vault.find` + `vault.secret_present`)

**What's wrong:**
`keep_credential_request_offer` decides whether to paint using the in-memory `site_logins` (sqlite
metadata). `result_without_broker` then requires `have_metadata && have_secret`, where `have_secret`
is a live Keychain lookup. The two gates can disagree.

**Why it matters / how it fails:**
This PR changes the bundle identifier (`Cargo.toml:63`, `Info.plist:5`: `com.example.nativechat` →
`dev.hexuria.nativechat`) and `build.sh` / `scripts/install-nativechat-app.sh` now pass
`--identifier dev.hexuria.nativechat` to `codesign`. The sqlite metadata is unaffected
(`Config::data_dir` is `ProjectDirs::from("ai","nativechat","NativeChat")`, independent of the bundle
id), but the macOS Keychain ACL for `ai.nativechat.site-login` items is bound to the creating
binary's designated requirement. After upgrading, `security_framework::passwords::generic_password`
can be denied. Result: Settings → Logins still lists `ada · facebook.com`, the Use-saved card is
still painted, the user clicks it, and gets a muted **"None saved"** pill + `missing` — for a login
they can see on screen. Same failure for a locked keychain, a keychain on a different volume, or the
Linux `site-login.vault` file being unreadable.

**Suggested fix:**
Make the paint gate agree with the answer gate: `has_matching_saved_login` should also consult
`vault.secret_present(&row.id)` (it is synchronous). And distinguish "no row" from "row without a
secret" in the pill copy — the latter needs "Saved login unavailable, re-save it", not "None saved".

---

## [SEVERITY: medium] [CONFIDENCE: high] Auto-miss starts a second reader of a run whose SSE stream is still open

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:6744-6771` (`auto_miss_empty_vault_credential_request` → `follow_run`), reached from the live stream callback at `state.rs:5564`

**What's wrong:**
`graft_turn_parts` is called from inside `send_opengrok_turn`'s streaming paint callback. When it
auto-misses, the spawned task calls `state.follow_run(run_id, …)` while the SSE stream for that same
run is still delivering. `follow_run` has no dedupe guard.

**Why it matters / how it fails:**
`live_turns[conv].message_id == reply_id` (`state.rs:5471-5477`), so both writers target the same
message: the SSE path writes `assembler.snapshot()`, and `follow_run` writes
`reply_from_replay(...)` every 300 ms for up to 400 iterations (2 minutes). The user sees the
assistant bubble flicker between the streamed text and the (usually older) replay text, and the
replay's `"finished"` branch can `persist_assistant_reply` while the stream is still running, after
which `turn_is_unsettled` goes false and the stream's own completion block silently returns at
`state.rs:5640`. Two HTTP readers for every empty-vault turn is also pure waste.

**Suggested fix:**
Do not `follow_run` when `turn_is_unsettled(conv, run_id)` is still true and a stream owns the turn;
the existing stream will carry the continuation. Alternatively keep a `HashSet<run_id>` of active
followers and refuse a second one.

---

## [SEVERITY: medium] [CONFIDENCE: high] The auto-miss `missing` POST failure is swallowed, and the deferred persist can lose the turn

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:6767-6771` (`let _ = client.post_credential_result(...)`), interacting with `state.rs:5685-5689` and `state.rs:5743-5744`

**What's wrong:**
Two coupled problems:
1. The `missing` POST's result is discarded. If it fails (wifi drop, 500, gateway down), the app
   still strips the card, still shows "Working", and still starts `follow_run`. The server never
   learns the answer and the run stays parked.
2. Because `empty_vault_continue_runs` contains the run, `send_opengrok_turn` **skips**
   `persist_assistant_reply` (`state.rs:5688`) and hands ownership of the write to `follow_run`. If
   `follow_run`'s first `replay_run` errors it `break`s (`state.rs:6254`) and falls to the tail,
   which only calls `finish_responding` — it never persists and never `release_live_turn`s.

**Why it matters / how it fails:**
Vault is empty, agent asks for a login, the network flaps for 3 seconds. The card silently
disappears; the user sees "Working" go away and nothing else; the agent is parked server-side
forever; and the assistant reply that *was* streamed is never written to sqlite while
`live_turns[conv]` stays populated, so the thread is held out of reload and the turn is lost on
relaunch. There is no error surface anywhere in this path.

**Suggested fix:**
Keep the `Result`. On failure: restore the card (or fold a "Couldn't tell the agent" pill with a
retry), do not enter `empty_vault_continue_runs`, and let the normal persist path run.

---

## [SEVERITY: medium] [CONFIDENCE: high] Bare `host:port` origins never resolve, so a saved `localhost` login is auto-missed

**Where:** `/Volumes/goldcoders/code/nativechat/src/site_login/origin.rs:42-47` (`host_of`)

**What's wrong:**
`host_of` tries `Url::parse(raw)` first and **returns early with whatever `host_str()` gives**,
including `None`. For a bare `host:port` with an alphabetic host, `Url::parse` succeeds with the host
as the *scheme* and an opaque path, so `host_str()` is `None` and the `https://{raw}` fallback is
never reached. Probe output from the real function:

```
    localhost:8765 -> None
 facebook.com:8080 -> None
    127.0.0.1:8443 -> Some("127.0.0.1")     # digits can't be a scheme, so the fallback runs
               ::1 -> None
```

**Why it matters / how it fails:**
`scripts/serve-login-demo.sh` documents the remasure flow and the demo runs on
`http://127.0.0.1:8765` (works, by luck of the leading digit). Point the same demo at
`localhost:8765` — or have the agent report the origin as `example.com:8443` — and
`registrable_origin` returns `None`, `origins_match` falls back to a raw string compare against the
stored `localhost` row, misses, and the app auto-POSTs `missing` for a site the user has definitely
saved. The user is never told; the card just never appears.

**Suggested fix:**
In `host_of`, only accept the first `Url::parse` when `host_str()` is `Some`; otherwise fall through
to the `https://{raw}` attempt. `if let Ok(url) = Url::parse(raw) && let Some(h) = url.host_str() { return Some(h.into()); }`

---

## [SEVERITY: medium] [CONFIDENCE: high] Username matching is exact, case-sensitive and untrimmed on the row side

**Where:** `/Volumes/goldcoders/code/nativechat/src/site_login/origin.rs:76-79`

```rust
match username.map(str::trim).filter(|name| !name.is_empty()) {
    Some(want) => row_username == want,
    None => true,
}
```

**What's wrong:**
The request side is trimmed; the row side is not. Neither is case-folded. The row's username comes
from whatever the person typed into the in-chat form (`site_login/extract.rs:username_from` keeps the
raw value); the request's username comes from the agent reading the page.

**Why it matters / how it fails:**
User saves `Ada@Example.com` (capitalised as they typed it). Agent later sends
`credential.request {origin: "facebook.com", username: "ada@example.com"}`. `login_matches_request`
returns false → no card → silent `missing`. Email local parts are case-insensitive in practice and
domains always are, so this will bite. A trailing space in the stored value has the same effect.

**Suggested fix:**
Compare `row_username.trim().eq_ignore_ascii_case(want)` (or normalise on save), and keep the
comparison consistent with `store.rs:find`'s SQL, which is also exact — either normalise both or use
`COLLATE NOCASE`.

---

## [SEVERITY: medium] [CONFIDENCE: high] The local fold and the REST status are computed from two different sources and can disagree

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:7541-7548` (fold from in-memory `site_logins`) vs `state.rs:7570-7585` (status from `vault.find` + `secret_present`)

**What's wrong:**
`fold_credential_answer(allow, has_matching_saved_login(...))` paints the pill synchronously from the
in-memory list; the REST status is computed asynchronously from sqlite. Only the
`Missing` disagreement is repaired afterwards (`state.rs:7601-7606`); an `Error`/`Denied`
disagreement is not.

**Why it matters / how it fails:**
`site_logins_ready == false` (first sqlite list has not landed — startup, or any read error) means
`keep_credential_request_offer` *keeps* the card. The user clicks "Use saved" within that window.
`has_matching_saved_login` returns false against the empty in-memory vec, so the pill folds
**"None saved" / "No saved login for this site."** — while `vault.find` finds the row, finds the
secret, and POSTs `status: error` with the real `credentialId`. The user is told there is no saved
login for a site they did save, and the transcript records that falsehood permanently.

**Suggested fix:**
Compute the fold from the same async vault answer that produces the status (fold optimistically to a
neutral "Checking…" and settle once), or refuse to paint the card at all until
`site_logins_ready` — and repaint on *every* status, not just `Missing`.

---

## [SEVERITY: medium] [CONFIDENCE: medium] "Not now" no longer follows the run, so everything the agent does after `denied` is invisible

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:7609-7612`

```rust
if allow && !run_id.is_empty() {
    state.begin_responding(conversation_id.as_deref(), "Working");
    state.follow_run(run_id, conversation_id, cx);
}
```

**What's wrong:**
Before this PR (`git show origin/gol/user-form-transcript-chrome-4df8:src/state.rs`) the same block
was `if !spec.run_id.is_empty()` — deny followed the run too. The `allow &&` guard is new.

**Why it matters / how it fails:**
The user clicks "Not now". NativeChat POSTs `credential.result {status: denied}` and then stops
watching. `park_waiting_for_you` already released `live_turns`, and `sync_waiting_chrome`
(`state.rs:7553-7555`) clears the Waiting label. If OpenGrok resumes the run after a `denied` result
— which is the normal contract for a HITL answer, the agent has to be told so it can try another
route — every subsequent frame of that run is dropped on the floor: no text, no status line, nothing
persisted. The thread looks idle while the bot is working. The user's only recovery is to switch
threads and back to force a reconcile.

**Suggested fix:**
Follow the run for both answers (the comment's worry — "Not now must not restart Working" — is about
the *label*, which `follow_run` sets from `activity_from_replay` anyway), or at minimum keep the
turn registered so the reconcile path picks it up.

---

## [SEVERITY: medium] [CONFIDENCE: high] "Route traffic through this computer" now defaults ON, resets to ON on logout, lost its disabled state, and a failed POST is neither reverted nor shown

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:1893` (`egress_tunnel_enabled: true`), `state.rs:2663` (logout resets to `true`), `state.rs:2352-2380` (`set_egress_tunnel_enabled`), `src/components/app_settings.rs:~697` (`Switch` lost `.disabled(!can_toggle)`)

**What's wrong:**
Three related changes:
1. The default flipped from `false` to `true`, and `host_egress_tunnel_flag` only overwrites it when
   the host actually sends `egressTunnelEnabled`. A host that omits the key leaves the UI asserting
   ON.
2. `logout()` resets it to `true`, discarding a deliberate opt-out.
3. `set_egress_tunnel_enabled` mutates local state optimistically, POSTs, and on `Err(_)` does
   nothing — no revert, no error. It also sets `host_egress_tunnel_available = true` purely because
   the POST returned OK.

**Why it matters / how it fails:**
This switch means "the bot's web traffic exits through my desktop" — a privacy/network-exposure
control. A user on a shared/provisioned box opens the Computer pane and sees the route-traffic icon
lit in `theme.primary` (`components/computer.rs:route_traffic_icon`) without ever having opted in;
if the host never sent the key, the UI is simply lying about the server's state. A user who turns it
off, then signs out and back in, silently gets it back on. And a user who turns it off while the
gateway is down sees "off" forever while the server keeps it on.

**Suggested fix:**
Default to `false` (or to `None` = unknown, rendered as indeterminate) until the host answers; do not
reset on logout (or reset to unknown); revert the optimistic flip and surface
`computer_action_error` when the POST fails.

---

## [SEVERITY: medium] [CONFIDENCE: high] `build.sh` / install script boot out the remasure LaunchAgents and never restart them

**Where:** `/Volumes/goldcoders/code/nativechat/scripts/stop-nativechat-for-codesign.sh:9-34` and `:36-43`

**What's wrong:**
`stop_nativechat_for_codesign` `launchctl bootout`s three labels and their plists. The counterpart
`maybe_start_nativechat_remasure` runs `${root}/scripts/start-nativechat-remasure.sh` **only if it is
executable** — and that file does not exist in the repo (`ls scripts/` →
`install-nativechat-app.sh  serve-login-demo.sh  stop-nativechat-for-codesign.sh`).

**Why it matters / how it fails:**
Anyone who runs `./build.sh` (or `scripts/install-nativechat-app.sh`) has their remasure/e2e
KeepAlive agents unloaded permanently and silently. The next remasure run does nothing and the
failure mode is "the harness stopped launching the app", with no message explaining why. The
bootout/codesign ordering fix therefore traded one silent failure for another.

**Suggested fix:**
Either commit `scripts/start-nativechat-remasure.sh` (bootstrap the same labels from the plists it
booted out), or have `maybe_start_nativechat_remasure` re-`bootstrap` each plist it actually unloaded
and print a warning when it cannot.

---

## [SEVERITY: medium] [CONFIDENCE: high] `scripts/install-nativechat-app.sh` cannot run from the `~/bin` location its own header prescribes

**Where:** `/Volumes/goldcoders/code/nativechat/scripts/install-nativechat-app.sh:14-22`

```bash
# Mac remasure: sync this file over ~/bin/install-nativechat-app.sh (that path is not in the repo).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/stop-nativechat-for-codesign.sh"
ENTITLEMENTS="$ROOT/nativechat.entitlements"
```

**What's wrong:**
`ROOT` is derived from the script's own location. Copied to `~/bin/install-nativechat-app.sh`,
`ROOT` becomes `$HOME`, so the `source` targets `$HOME/scripts/stop-nativechat-for-codesign.sh`
(absent) and, under `set -euo pipefail`, the script dies on line 18 with a bare
`No such file or directory`. `ENTITLEMENTS` would also point at `$HOME/nativechat.entitlements`.

**Why it matters / how it fails:**
The documented remasure procedure ("sync this file over `~/bin/...`") produces a script that exits 1
immediately. And because the header explicitly blesses a second copy outside the repo, the two will
drift — which is the exact class of problem the review brief asks about.

**Suggested fix:**
Resolve the repo root from an env var / `git rev-parse --show-toplevel` with a fallback, inline the
helper functions rather than sourcing a sibling, and make `~/bin/install-nativechat-app.sh` a symlink
into the repo instead of a copy.

---

## [SEVERITY: medium] [CONFIDENCE: medium] `pkill` does not wait for the process to exit, so the codesign race is only narrowed, not closed

**Where:** `/Volumes/goldcoders/code/nativechat/scripts/stop-nativechat-for-codesign.sh:34-35`

```bash
pkill -x NativeChat 2>/dev/null || true
pkill -x nativechat 2>/dev/null || true
```

**What's wrong:**
`pkill` sends SIGTERM and returns immediately. `build.sh` then proceeds straight to
`cargo bundle` / `codesign --force` on a bundle whose Mach-O pages may still be mapped by a
terminating process.

**Why it matters / how it fails:**
A GPUI app with an open window, a TTS service and detached tokio tasks can take hundreds of
milliseconds to unwind. `codesign --force` rewrites the code signature of a binary that is still
mapped; the kernel invalidates the pages under the dying process and the *next* launch of the
freshly-signed app can still hit `CODESIGNING Invalid Page` → SIGKILL. That is the bug 12871dc
claims to fix.

Two smaller notes on the same file: `pkill -x nativechat` will also kill a developer's
`cargo run` dev binary in another terminal, and both `pkill`s match by name only — any unrelated
process called `nativechat` goes with it.

**Suggested fix:**
Poll until gone, then escalate:
```bash
for _ in $(seq 1 40); do pgrep -x NativeChat >/dev/null || break; sleep 0.25; done
pgrep -x NativeChat >/dev/null && pkill -9 -x NativeChat
```

---

## [SEVERITY: medium] [CONFIDENCE: high] The empty-vault path leaves no user-visible trace at all, which is inconsistent with the "None saved" backstop

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:6714-6742` (`strip_auto_missed_credential_requests`) vs `src/opengrok/credential.rs:44` / `src/components/save_login.rs:158-229` (the "None saved" pill)

**What's wrong:**
When the vault has no match, the card is removed from the transcript entirely — no pill, no muted
line, nothing. When the same condition is discovered one click later (the backstop), the app folds a
"None saved" remnant. The same fact produces two completely different transcripts depending on
timing.

**Why it matters / how it fails:**
From the user's chair: the agent says "I need to sign in to facebook.com", then the thread shows
"Working" and moves on. There is no indication that NativeChat was asked for a credential, declined
on their behalf, and told the server `missing`. The user has no way to learn "add a login in
Settings → Logins and this will work" — the only place that string exists is the Settings → Logins
subtitle, which they have no reason to visit. If the vault is *broken* rather than empty
(see the `site_logins_ready` finding), this is indistinguishable from normal operation.

Also: if a `credential.request` arrives before the first sqlite list lands (`site_logins_ready ==
false`), `keep_credential_request_offer` returns `true`, the card **is** painted, and then
`reload_site_logins`'s sweep strips it — a visible flash of a card the user may already be reaching
for.

**Suggested fix:**
Always leave the folded `Missing` remnant (with a "Add a login" affordance that opens
Settings → Logins) rather than deleting the part, and separate "no saved login" from "vault
unavailable" in the copy. Do not paint at all until `site_logins_ready` to kill the flash.

---

## [SEVERITY: medium] [CONFIDENCE: medium] Round-tripping the refresh error through `RefreshOutcome` flattens `Failure` kinds

**Where:** `/Volumes/goldcoders/code/nativechat/src/opengrok/client.rs:53-58` (`RefreshOutcome::Failed { status, message }`), `client.rs:431-433` (`OpenGrokError::from_server(status, message)`)

**What's wrong:**
`OpenGrokError` carries a private `failure: Failure` that is deliberately *not* derivable from
status + message (`error.rs:55-64`: "Private so the invariant holds"). `RefreshOutcome` keeps only
`status` and `message`, so the reconstruction at `client.rs:432` re-derives the kind with
`from_server`:
- a transport failure (`OpenGrokError::transport`, `Failure::OutOfReach(Server)`, status `None`)
  comes back as `Failure::Verdict`;
- a 401 that `read_error`/`signed_out_error` would classify as `Failure::SignedOut` comes back as
  `Failure::Verdict` with status 401, so `is_signed_out()` is now `false`.

**Why it matters / how it fails:**
`state.rs:2771-2775`, `state.rs:5761`, `state.rs:7035` and `state.rs:7847` all branch on
`failure()` / `is_signed_out()` to decide between the "server out of reach" indicator, the SignedOut
banner, and a red transcript line. A wifi blip during `/auth/refresh` that previously lit the
reachability indicator now reads as a verdict. Today the only two `refresh()` callers ignore or
`.is_ok()` the error, so the blast radius is small — but the contract is broken for the next caller.

**Suggested fix:**
Carry the `Failure` (make it `Copy` + public-by-getter into the outcome) or store the whole error in
an `Arc<OpenGrokError>` inside `RefreshOutcome` so waiters get the original classification.

---

## [SEVERITY: low] [CONFIDENCE: high] Vault state and an in-memory password survive `logout()`

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:2643-2672`

**What's wrong:**
`logout()` clears coworkers, computers, approvals and egress state, but not `site_logins`,
`site_logins_ready`, `site_login_vault`, `credential_request_resolutions`, `credential_auto_missing`,
or `pending_save` (which holds a cleartext password in memory awaiting Save/Not now).

**Why it matters / how it fails:**
Account A signs out on a shared Mac; account B signs in. Settings → Logins still lists A's saved
sites; B's agent asking for `credential.request {origin: facebook.com}` now gets a **"Use a saved
login for facebook.com?"** card backed by A's Keychain row, and (in A.1) B's box would receive A's
session. Separately, a `PendingSave` whose Save/Not now was never answered keeps A's password in the
process after sign-out.

**Suggested fix:**
Clear `pending_save` on logout at minimum; decide explicitly whether the vault is per-machine (then
say so in the Logins subtitle) or per-account (then scope the rows and clear them).

---

## [SEVERITY: low] [CONFIDENCE: high] The folded credential remnant is not persisted — it vanishes on relaunch

**Where:** `/Volumes/goldcoders/code/nativechat/src/state.rs:136-139` (`saved_parts` maps `ChatPart::CredentialRequest(_)` to a paragraph break)

**What's wrong:**
`credential_request_resolutions` is in-memory and `saved_parts` drops the part, so "Used saved
login" / "Dismissed" / "None saved" exist only for the current session.

**Why it matters / how it fails:**
The stated goal of 20a4dd2 is a remnant "the way user-form Continue/Dismiss leave a remnant".
Restart the app and the transcript shows no record that a saved credential was confirmed for a site
— which is the one event in this feature a user would later want to audit. (User-form pills have the
same gap, so this is consistent, not a regression — but it undercuts the feature.)

**Suggested fix:**
Persist a minimal `MessagePart::CredentialRemnant { origin, username, resolution }` (never the
credentialId) so the fold survives reload.

---

## [SEVERITY: low] [CONFIDENCE: high] Theme persistence does blocking file I/O on the UI thread and swallows write errors

**Where:** `/Volumes/goldcoders/code/nativechat/src/theme.rs:111-125` (`load_saved_mode_from` / `save_mode_to`), called from `state.rs:set_theme_mode` and `state.rs:restore_saved_theme`

**What's wrong:**
`set_theme_mode` (the sidebar toggle, ⌘T, and the Settings chips) synchronously writes
`theme.json` on the render thread, with `let _ = std::fs::write(path, json);`. `load_saved_mode` does
a synchronous read from `theme::init`, `main.rs:249` and `RootView::new` (three reads at startup).

**Why it matters / how it fails:**
On a network/slow volume — which this project runs on (`/Volumes/goldcoders`, noted in the user's own
memory as a single point of failure) — the toggle stalls the frame. If the write fails (volume
dropped, read-only, disk full) the mode silently fails to persist and the user finds their theme
reverted next launch with no explanation.

**Suggested fix:**
Write via `cx.background_spawn`, and set a visible error (or at least `log::warn!`) when the write
fails.

---

## [SEVERITY: low] [CONFIDENCE: medium] `FileVault` writes plaintext passwords at default permissions before chmod, and swallows the chmod failure

**Where:** `/Volumes/goldcoders/code/nativechat/src/site_login/secrets.rs:91-102` (pre-existing, but newly advertised by this PR)

**What's wrong:**
`src/site_login/mod.rs:9-10` now documents "When Keychain is missing (Linux/dev), `VAULT_FILE` in
that same data dir, **mode 0600**". The implementation does `fs::write(&tmp, bytes)` — creating the
file with the process umask (typically 0644) containing cleartext passwords — and only then
`let _ = fs::set_permissions(&tmp, 0o600)`, whose failure is discarded, before `fs::rename`.

**Why it matters / how it fails:**
On a multi-user Linux dev box there is a window where every password in the vault is world-readable,
and if `set_permissions` fails the file is renamed into place at 0644 permanently. The new doc
comment states a guarantee the code does not make.

**Suggested fix:**
Create the temp file with `OpenOptions::new().mode(0o600).create_new(true)`, and propagate a
`set_permissions` error instead of discarding it.

---

## [SEVERITY: low] [CONFIDENCE: high] `let _ =` / side-effect-in-predicate / unbounded maps in the new credential machinery

**Where:**
- `/Volumes/goldcoders/code/nativechat/src/state.rs:5356-5360` — `persist_assistant_reply` mutates
  `self.empty_vault_continue_runs` inside the closure passed to `is_some_and`, so the removal happens
  as a side effect of evaluating a boolean and fires even when `settles` turns out false.
- `state.rs:1870-1875` — `credential_request_resolutions`, `credential_auto_missing` and
  `user_form_resolutions` are never pruned; they grow for the process lifetime.
- `state.rs:6714-6742` — `strip_auto_missed_credential_requests` re-implements the exact retain
  predicate already in `graft_user_forms:6613`, inlining `login_matches_request` by hand because of a
  borrow conflict.
- `src/agent/host.rs:~2100` — `parse_invoke_allow(args).ok()` throws away the specific
  `unknown allow \`{word}\`` message, so a typo'd invoke reports the generic
  "requires arg allow".

**Why it matters / how it fails:**
Each is small, but together they make the credential state machine hard to reason about — and the
1214-line `state.rs` delta is the reason. `AppState` now owns four credential-specific collections,
seven credential-specific methods and the auto-miss lifecycle, interleaved with turn plumbing.

**Suggested fix:**
Extract a `CredentialRequests` struct (resolutions + auto-missed + continue-runs + the ready flag +
the match/sweep/fold methods) into `src/site_login/` or `src/opengrok/credential.rs` with `AppState`
holding one field, and give it the single retain predicate both call sites use.

---

## [SEVERITY: low] [CONFIDENCE: medium] `attention_cta` is a bare `div` + `on_mouse_down`: no keyboard activation, no focus ring, disabled is opacity-only

**Where:** `/Volumes/goldcoders/code/nativechat/src/components/alert_chrome.rs:144-181`, used by `components/computer.rs` (Skip this step / I'm done, continue) and `components/user_form.rs:360-419` (Take over / I'm done / Skip)

**What's wrong:**
The CTAs moved off `Button` onto a hand-rolled pill. They fire on `on_mouse_down` (not click, so a
press-and-drag-away still commits), have no `tab` focus, no `Enter`/`Space` handling, and the
disabled state is `.opacity(0.45)` with no cursor or tooltip explaining why.

**Why it matters / how it fails:**
"I'm done, continue" is the CTA that hands a live box back to the agent; it is now mouse-only. A
user who tabs to it and presses Enter gets nothing. And a disabled "I'm done" (because
`can_resolve` is false while the sibling `handoffEntryId` has not arrived) just looks faded, with no
explanation — the user reads it as the app being broken.

**Suggested fix:**
Use `on_click` instead of `on_mouse_down`, add a focus handle + key binding, and give the disabled
state a tooltip ("waiting for the computer to hand back").

---

## [SEVERITY: low] [CONFIDENCE: medium] `screen_tile` sets a fixed width *and* height *and* `aspect_ratio`

**Where:** `/Volumes/goldcoders/code/nativechat/src/components/computer.rs:951-956`

**What's wrong:**
```rust
let width = computer_pane_screen_width();
let height = box_screen_height_for_width(width);
div().w(px(width)).h(px(height)).aspect_ratio(BOX_SCREEN_ASPECT)
```
With both axes already fixed, `aspect_ratio` is either ignored or overrides one of them depending on
the taffy version. The values agree today only because `box_screen_height_for_width` uses the same
ratio.

**Why it matters / how it fails:**
Change `INFO_PANE_WIDTH` or the gutter constant and the two sources of truth can diverge, producing
either a letterboxed thumb or a clipped taskbar — the exact defect 18e2343 set out to fix. The
in-chat well (`components/user_form.rs:317-319`) correctly uses `w_full()` + `aspect_ratio` only.

**Suggested fix:**
Drop the explicit `.h(...)` and keep `w` + `aspect_ratio`, matching the in-chat well.

---

## [SEVERITY: low] [CONFIDENCE: medium] `computer-update` / `computer-reset` stay clickable in the gpui-agent tree when the UI has them disabled

**Where:** `/Volumes/goldcoders/code/nativechat/src/agent/host.rs:~1345` (nodes added unconditionally) vs `src/components/computer.rs:1255-1275` (`icon_btn_enabled` attaches no handler when disabled)

**What's wrong:**
The host snapshot always emits `computer-update` and `computer-reset` buttons, and
`click_command` maps them to `OpenComputerConfirm(Update/Reset)` unconditionally. The rendered UI
disables them (`!present || updating || current`).

**Why it matters / how it fails:**
A remasure/e2e driver can click `computer-reset` while an update is in flight — a destructive action
("Everything on it is lost") that a human could not reach from the same screen. The credential card
got exactly this treatment (`host.rs:1727-1729` skips folded requests); the computer controls did
not.

**Suggested fix:**
Mirror `update_disabled()` / `can_reset` into the snapshot and refuse the click, the way
`credential_request_command` refuses a folded card.

---

## [SEVERITY: low] [CONFIDENCE: medium] `keep_credential_request_offer` drops a *settled* card when `auto_missed` is set

**Where:** `/Volumes/goldcoders/code/nativechat/src/opengrok/credential.rs:318-334`

**What's wrong:**
`auto_missed` is checked before `settled`, so `keep_credential_request_offer(true, _, true, _)` is
`false` — a card the user actually answered would be deleted if its id ever landed in
`credential_auto_missing`. The test at `credential.rs:523` asserts this ordering deliberately, but
nothing enforces that the two sets stay disjoint: `auto_miss_empty_vault_credential_request` inserts
into `credential_auto_missing` without checking `credential_request_resolutions`, relying entirely on
`should_auto_miss_credential_request` having filtered first.

**Why it matters / how it fails:**
Any future caller of `auto_miss_empty_vault_credential_request` that skips the
`should_auto_miss_credential_request` guard silently erases an answered card (and its audit
remnant) from the transcript. It is a one-line footgun in a state machine with four flags.

**Suggested fix:**
Check `settled` first, or have `auto_miss_empty_vault_credential_request` refuse when
`credential_request_resolutions.contains_key(request_id)`.

---

## [SEVERITY: low] [CONFIDENCE: low] Stale completed flight can be replayed to a fresh caller; dropped waiters leak the client

**Where:** `/Volumes/goldcoders/code/nativechat/src/opengrok/client.rs:410-434`

**What's wrong:**
The shared future is only cleared by a waiter that survives to the post-await block. If every waiter
is cancelled (GPUI task dropped on window close / entity release), the slot keeps `Some(shared)`.
If the future had already completed, the next `refresh()` joins it and receives the **cached**
outcome rather than performing a real refresh; if it had not, the slot pins a `BoxFuture` holding an
`OpenGrokClient` clone, which holds the same `Arc<AsyncMutex<Option<Shared>>>` — a reference cycle
that outlives the request.

**Why it matters / how it fails:**
Worst case is one caller getting a stale `Ok`/`Failed` — it will then 401 and retry, so it
self-heals; and a small leak of one in-flight reqwest per cancelled burst. Low impact but worth
closing while the identity check from the high-severity finding above is being added.

**Suggested fix:**
Clear the slot from the flight itself (a guard / `Drop` on the future) rather than from the waiters.

---

## Categories with nothing to report

- **Secret leakage into AG-UI `content`, `ChatPart`, transcripts, the Box, or a REST body.** I traced
  every new path: `credential_result_body` (`credential.rs:208-223`) carries only status / requestId /
  agentId / credentialId; `CredentialRequestSpec` has no password field and its `from_event` never
  reads one; `PendingSave` has a redacting `Debug`; `saved_parts` refuses credential parts; the new
  `eprintln!` (`state.rs:5461`) prints only a run id. `SecretStore::get` is called from nowhere
  outside tests — `answer_credential_request` uses `secret_present` only. `Settings → Logins` renders
  `origin`/`username`/`label` and never a secret. The only value that leaves the machine is the
  sqlite row uuid as `credentialId`, which is not a secret (though see the eTLD+1 finding for why it
  can be the *wrong* uuid).
- **Panics on user-reachable paths.** Every new `unwrap`/`expect`/`panic!` in the diff is inside
  `#[cfg(test)]`. The two new `debug_assert_ne!`s (`state.rs:7543`, `state.rs:7580`) are
  release-compiled out and guard an invariant the code already enforces.
- **Deadlocks in the single-flight refresh.** `refresh_once` is awaited outside the mutex and makes
  no re-entrant call to `refresh()`, so there is no lock-ordering hazard. The bugs there are the
  unconditional clear and the `has_fresh_access` shortcut, both reported above.
