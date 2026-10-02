//! Opt-in gpui-agent control plane (feature `agent`).
//!
//! Start NativeChat with `GPUI_AGENT=1` (and a token, or
//! `GPUI_AGENT_INSECURE_NO_TOKEN=1`). The TCP thread posts onto a mailbox;
//! [`RootView`](crate::root::RootView) drains it on the UI thread.
//!
//! Stable ids: `app-window`, `sidebar`, `sidebar-chat-list`, `nav-new-chat`,
//! `nav-toggle-sidebar`, `session-{id}`, `footer-theme`, `footer-account`,
//! `composer`, `composer-panel`, `composer-panel-search`, `composer-recipe-bar`,
//! `composer-skill` (the skill the next message is sent with, value = the id the turn names;
//! in the tree only while one is on the draft),
//! `image-thumb-{n}`, `lightbox`, `user-form-{key}`, `user-form-field-{key}-{id}`,
//! `user-form-continue-{key}`, `user-form-dismiss-{key}`, `user-form-screen-{key}`,
//! `user-form-pill-{key}`, `computer-handoff-{key}`,
//! `computer-handoff-takeover-{key}`, `computer-handoff-done-{key}`,
//! `computer-handoff-skip-{key}`,
//! `save-login-{entry}`, `save-login-save-{entry}`, `save-login-skip-{entry}`,
//! `user-form-use-saved-{key}-{login}` (one per saved account for the card's site),
//! `user-form-saved-note-{key}`, `user-form-saved-clear-{key}`,
//! `settings-tab-logins`, `settings-logins-search` (value = the query), `settings-login-add`,
//! `settings-login-import`, `settings-logins-notice`, `settings-logins-error`,
//! `settings-logins-empty`, `settings-logins-group-passwords|passkeys|codes|security` (a
//! section of the list, value = its count; the three kinds are always there until a search
//! leaves one empty, Security only while a row has a `Security:` note) with its rows
//! `settings-login-row-{id}` under it (a row with a `Security:` note is under its kind and
//! under Security, one id twice; the picked one has state `selected`),
//! `settings-login-code-{id}` (a row's live code and the seconds left, on the pane and in
//! the list), `settings-login-add-error` while the Add sheet shows one,
//! `settings-login-notes-save-{id}` (the button that files an edited note),
//! `settings-login-detail-{id}` (the picked row's pane: `settings-login-username-{id}`,
//! `settings-login-website-{id}`, `settings-login-where-{id}`, `settings-login-notes-{id}`
//! (value = the notes), `settings-login-last-used-{id}`, `settings-login-delete-{id}`),
//! `settings-login-add-sheet` while the Add sheet is up (`settings-login-add-title`,
//! `settings-login-add-username`, `settings-login-add-password`, `settings-login-add-website`,
//! `settings-login-add-notes`, `settings-login-add-save`, `settings-login-add-cancel`),
//! `coworker-{id}` (a sidebar row: value = the last thing said in the bot's thread, state
//! `listed` when that thread came from the server's list and not this Mac),
//! `approval-{call}` (title = the card's own, states = reason, thread, tool),
//! `reply-steps` (the newest coworker reply's steps, a list with value = how many; in the tree
//! only while it has any) with `step-{call_id}` under it (label = the step row's own words,
//! value `running` / `ok` / `failed`, state `expanded` while its row is open; a click opens or
//! shuts it, and an open step holds its "N steps" line open), `reply-reasoning` (value = how
//! many Thought rows that reply has, in the tree only while it has any; state `expanded` while
//! all are open; a click opens them all, or shuts them once they all are),
//! `recipe-run` (on the open recipe: value = the bot it plays on, disabled while it cannot run
//! or a run is going; invoke `recipe.run {bot?}`), `recipe-run-result` (value `running` / `ok`
//! / `failed` / `interrupted`), `recipe-error` (what the page says went wrong, e.g. a
//! refused Run), `recipe-history-runs` (value = count) with
//! `recipe-history-run-{runId}` (value = `running` / `finished` / `interrupted`, state `ok`),
//! `settings-computer-{machine}-exec` (a connected computer's local-exec mode, value `ask` /
//! `bypass` / `never`, state `this-mac`) with `settings-computer-{machine}-exec-ask|bypass|never`,
//! `settings-tab-computer`, where this Mac's standing rules sit under its mode:
//! `settings-local-rules-allow|deny` (a list, value = its count; in the tree only while it has a
//! rule on it) with its rows `settings-local-rule-allow|deny-{n}` (counted from 0 in the
//! server's order, value = the command exactly; state `inert` on an allow the server says can
//! never match, with `settings-local-rule-inert-allow-{n}` saying so and the server's reason as
//! its value; state `removing` while its Remove is with the server), and under each row
//! `settings-local-rule-remove-allow|deny-{n}` (dead while removing) and
//! `settings-local-rule-error-allow|deny-{n}` (why its last Remove did not go through);
//! `settings-local-rules-empty` while there are none, `settings-local-rules-error` while they
//! could not be read. Nothing for a machine that is not this Mac.
//! `routine-new`, `routine-{id}`, `routine-{id}-trigger-schedule`,
//! `routine-{id}-trigger-webhook`, `routine-{id}-webhook-url`,
//! `routine-{id}-webhook-key`, `routine-{id}-rotate`, `routine-{id}-test` (Test run, on a
//! routine the server has), `routine-{id}-run-{runId}` (one Run history line: label `Test run` /
//! `Webhook` / `Schedule`, value `running` / `waiting` / `ok` / `error`; a click opens the
//! routine's thread), `routine-{id}-delete`.
//! `routine-{id}-thread` (Open thread, on a routine the server has); on a routine's thread the
//! chat carries `chat-routine-thread` (label the routine's name, value `schedule` / `webhook`)
//! and `chat-routine-back` (back to the bot's own chat), `chat-routine-instructions` (value = how many
//! bubbles are labelled as the routine's instruction). Invoke `routine.thread {id}` opens it.
//! Invoke `routine.run {id}` is Test run; `routine.edit {id, name?, prompt?}` saves an edit the
//! way the editor does (a `PATCH` of what changed).
//!
//! A routine's `{id}` is the server's schedule id. The two trigger ids are in the tree only
//! while the routine has no trigger, and the webhook's three only while it has one, so
//! `assert --exists false` answers "this one already fires" and "this one is not a webhook".
//!
//! Files (#90): `composer-file-{i}` under `composer` (label = the file's name, value `uploading`
//! / `ready` / `failed`); `message-file-{artId}` on the chat page for each file a message in the
//! open thread carried (label = filename, value = the message id). Invoke `composer.attach --arg
//! path=/absolute/path` attaches a file, as picking it with the + would; it uploads at once, and a
//! relative path or a kind the server does not take is refused. `composer.detach --arg index=N`
//! takes `composer-file-N` off the draft, as its ✕ would.
//!
//! The draft's chips: `composer-chip-{i}` under `composer`, in order (label = what the chip reads
//! as, value `tool` / `recipe` / `workflow` / `skill`). A chip is one object in the field (#40):
//! one Backspace after it removes it whole.
//!
//! Choice cards (the server's `form` tool) in the open thread: `choice-{messageId}` (value
//! `open` / `answered` / `not-answered` / `dismissed`; state `keyboard` on the one card a letter
//! answers). While open: `choice-{messageId}-{field}-{option}` (value = its keycap letter on a
//! one-question card, where a click sends the answer; state `selected` when picked),
//! `choice-{messageId}-dismiss`, and `choice-{messageId}-submit` on a card of several questions.
//! Answered: `choice-{messageId}-answer` (label = what was sent, title aside). `key
//! choice-{messageId} <letter>` takes the caret out of the composer and presses the letter at
//! the window, the way a person does after clicking the card. Only the card in state `keyboard`
//! takes it; a letter aimed at any other card is refused. A card the bot followed with another
//! card is `not-answered`: only the newest card asks.
//!
//! In the bot's settings: `agent-usage` (value = the Usage card's line: what the server says the
//! bot used this month, or why it cannot say), `agent-usage-toggle` (Show / Hide, only while the
//! server reported models), and `agent-usage-model-{i}` per model, visible while open (#138).
//!
//! In the bot's settings: `agent-tools` (value = the Tools card's first line, what the server's
//! `GET /coworkers/{id}/tools` says the bot is offered on its next turn: `2 built in · 1 from
//! plugins`, `Asking the server…`, or why there is no list), `agent-ceiling` (value = `3 of 8
//! allowed`, or why there are no switches; in the tree once the server has answered), and
//! `agent-tools-toggle` (Show / Hide, only while there is something to show). Visible while the
//! card is open: the card's own lines, `agent-ceiling-read-only` (the server's words for a 403:
//! every switch is dead), `agent-ceiling-wait` (why every switch is dead for now: another Bot's
//! switch, or a read of this Bot's ceiling, is with the server) and `agent-ceiling-note` (the
//! server's words about the last switch when no row is the one they are about, as a 409's "the
//! tools changed since you looked"); and one `agent-ceiling-switch-{name}` per row of the bot's
//! tool ceiling (opengrok-server#268): a switch named by the row's heading, value `builtin` /
//! `plugin`, `checked` where it stands (where it was asked to go while that is with the server),
//! enabled only while a click would send it, states `switching` and `unavailable`. Under it:
//! `agent-ceiling-why-{name}` (why the server cannot offer it now),
//! `agent-ceiling-connector-{name}` (the connection a plugin uses) and
//! `agent-ceiling-error-{name}` (the server's words for a switch it did not take, or that nobody
//! knows whether it did). Every id under a row has
//! its fixed word (`switch`, `why`, `connector`, `error`) before the row's name and the card's
//! own ids have none, so no plugin's name makes one id another's. A click moves a live switch the
//! other way and sends the whole ceiling at once, with the version it was read at; one at a time,
//! so every switch is refused while any is with the server or the ceiling is being read, and a
//! plugin the server no longer loads is refused going back on. Where the ceiling could not be
//! read (a server without the route, a Bot this person does not own), the card lists what the
//! next turn is offered instead, read-only: `agent-tool-{name}` (value `builtin` / `plugin`).
//!
//! In the bot's settings, below Tools: `agent-skills` (value = the Skills card's line: `2
//! attached · 1 switched off`, `None attached`, `Asking the server…`, or why there are no
//! switches, as "Only this Bot's owner can change its skills."), and `agent-skills-toggle` (Show
//! / Hide, only while there are skills to show). Visible while the card is open: the card's own
//! lines, `agent-skills-read-only` (the server's words for a 403: every switch is dead),
//! `agent-skills-wait` (another Bot's skill switch, or a read of this Bot's skills, is with the
//! server) and `agent-skills-note` (the server's words about the last switch when no skill is the
//! one they are about: a 409's "the skills changed since you looked", or the 422 over the cap on
//! attached skills); one `agent-skills-switch-{id}` per skill of the account's library the owner
//! may attach (opengrok-server#270), by the skill's id and never its name, since one of the
//! owner's skills and a colleague's can share a name: a switch named by the skill's name, value
//! `mine` / `org`, `checked` while attached (where it was asked to go while that is with the
//! server), enabled only while a click would send it, states `switching` and `switched-off`.
//! Under it: `agent-skills-off-{id}` (switched off in Settings → Skills) and
//! `agent-skills-error-{id}` (the server's words for a switch it did not take, or that nobody
//! knows whether it did). And `agent-skills-shared` ("People who use this Bot can read its
//! attached skills."), only on a Bot the roster says is shared with the owner's organization.
//! Every id under a skill has its fixed word (`switch`, `off`, `error`) before the skill's id and
//! the card's own ids have none, so no skill's id makes one id another's. A click attaches or
//! detaches at once, sending every attached skill's id with the version the skills were read at;
//! one at a time, so every switch is refused while any skill switch is with the server or the
//! skills are being read, and a skill switched off in Settings → Skills is refused being
//! attached, though it can always be detached. The Tools card's switches and these are apart:
//! neither waits on the other.
//!
//! The Bot's model picker, on the Model card in the Bot's settings, which is the one place a
//! Bot's model is picked: the composer has no chip for it, and no `model-*` id outside the card
//! is the picker's. `agent-model-card`, in the tree while a Bot is open, is a button named as the
//! picker reads in a line (`GPT-6 Luna · Medium ⚡`) and valued by the model the Bot's next turn
//! runs on, with its door's wire word as a state (`gateway` / `local_proxy`), `fast` while ⚡ is
//! on, and `expanded` while its popover is open; a click opens or shuts the popover, only while
//! the settings are open. The popover, `agent-model-pop`, visible while open, holds
//! `agent-model-fast` (a switch, checked while on; dead, with why as its value, where the list
//! holds no fast version of the model or the account's plan model answers for every Bot),
//! `agent-model-effort` (a slider named as the effort reads, `Light` … `Ultra` or `Default`, and
//! valued by the server's word; `set_value` takes `low`, `medium`, `high`, `xhigh` or `max`; dead
//! from a server that keeps no effort, one from before opengrok-server#271),
//! `agent-model-open-list` (named by the model; it opens the list) and `agent-model-reset` (↺:
//! Default effort and ⚡ off, the model left alone; live while there is something to put back).
//! While the list shows, those give way to `agent-model-open-list` as the heading back (state
//! `expanded`), `agent-model-search` (the search box at the top of the list, a textbox valued by
//! what is typed: `set_value` writes it, `type` adds to it, `key` takes Backspace and Enter; it
//! filters both groups at once, whatever the case, by a model's name and by its raw id, and the
//! list opens with it empty) and `agent-model-list` (always in the tree, valued by how many
//! models the search leaves, all of them while nothing is typed). The list holds what its window
//! draws: at most five models at a time, an `agent-model-row-{source}-{id}` each, by its door's
//! wire word and the id a pick pins with ⚡ off (valued by its door's word, state `selected` on
//! the one that answers and `fast` where the list holds its fast twin; a click puts the Bot on
//! it, fast where ⚡ is on and it has a twin, and goes back to the controls), with an
//! `agent-model-group-{source}` heading over each group's first model in view, which is not one
//! of the five: `agent-model-group-local_proxy` is "Subscription", the person's own plan, and
//! `agent-model-group-gateway` is "Gateway", the server's paid keys. The window opens with the
//! model that answers in view, and the wheel scrolls it; a model out of view, or one the search
//! leaves out, is refused, and the search box brings it into view. Then `agent-model-plan` (on a
//! server whose rows carry no `source`, while the account is on the person's plan: the account's
//! plan model, which answers for every Bot there; a line, not a row, which the search leaves or
//! takes away as it would a row), `agent-model-no-match` ("No model matches", while the search
//! leaves nothing of a list that has some), `agent-model-routines` (on a Bot whose own door is
//! the person's plan, whatever it is pinned to: the line saying its routines won't run, since
//! they run on the server's paid keys and the server refuses every routine of such a Bot
//! (opengrok-server #304), and to pick a Gateway model to run it on a schedule; a line, not a
//! row) and `agent-model-note` (the server's word on why the list is not fuller).
//! `agent-model-error` is the server's words for the last change it refused. Every change is
//! saved on the Bot at once, `source`, `model` and `effort` on `PATCH /coworkers/{id}`
//! (opengrok-server main d6f640e (#307, after #304), pin bf99845). Invoke `model.picker`,
//! `model.picker.open` and `model.picker.close` work the popover, and a click on
//! `agent-model-dismiss` shuts it.
//!
//! In the bot's settings: `agent-settings-error` is the pane's red line over Save: a refused Save
//! or pick, in the server's words.
//!
//! Connections (#2), a connection named by the server's id and a service by the name
//! `GET /connectors` lists it under. Only the person's own connections are on either surface: a
//! Bot's own sign-in, the whole server's, or a scope this app does not know is not theirs to
//! lend or disconnect. `settings-tab-connections` (refused while Settings is shut); while
//! Settings is open on it, `settings-connections-refresh`, `settings-connections` (value = how
//! many are connected) with a `settings-connection-{id}` per connection (label = its label,
//! value = the line under it: the service and who it is lent to, by bot name, a lend or a revoke
//! with the server shown as asked; state `changing` while a change to it is with the server),
//! holding `settings-connection-disconnect-{id}` (dead while changing; pressing it only asks,
//! and while the row asks it is replaced by `settings-connection-ask-{id}`, value = "Disconnect
//! Gmail? Ada and Bo will lose it.", with `settings-connection-confirm-{id}` to disconnect and
//! `settings-connection-keep-{id}` to leave it) and
//! `settings-connection-error-{id}` (why its last Disconnect did not go through);
//! `settings-connections-empty` / `settings-connections-error` in its place when there is
//! nothing to list or it could not be read. Then `settings-connectors` (value = how many
//! services are on offer and not connected) with `settings-connect-{connector}` (label `Connect
//! Gmail`, or `Opening…` with state `opening` while its sign-in page is asked for, when every
//! Connect is dead; state `waiting` once the browser has the page, until the service is listed
//! or ten minutes have gone) and `settings-connect-error-{connector}` (why Connect did not open
//! the browser); `settings-connectors-empty` (the server offers none, or all are connected) /
//! `settings-connectors-error` in its place. A click on a Connect opens the person's browser, as
//! a person's does, once the server answers and only if the page is still on screen.
//!
//! In the bot's settings: `agent-connections` (value = the card's second line: `1 of 2 lent to
//! this Bot`, `Asking the server…`, or why there is nothing to count), and per connection
//! `agent-connection-lend-{id}` (a switch, label = its label, value = its service, checked while
//! it shows as lent to this bot; a click asks for the other way; dead and `changing` while a
//! change is with the server) and `agent-connection-error-{id}` (why a lend or a revoke of it to
//! this bot did not go through; another bot's is on that bot's card); `agent-connections-note`,
//! the card's sentence that a lent connection reaches a plugin only once the bot is allowed it in
//! its Tools (opengrok-server#268).
//!
//! Reply source, where a Bot's replies are paid from: the server's paid keys or the person's own
//! subscription through opencodex, running on the same machine as the server. The page sets up
//! the subscription's connection and switches no door: a Bot's door is picked with its model on
//! `agent-model-card`, and the account's kind stays as the server keeps it for the Bots that have
//! picked none. No radio is drawn, and none of the radio's ids, nor the plan model picker's from
//! before (`settings-reply-source-kind*`, `settings-reply-source-via-mac`,
//! `settings-reply-source-model*`, `settings-reply-source-no-model`), is on the tree or answers.
//! `settings-tab-reply-source` (refused while Settings is shut); while Settings is open on it,
//! `settings-reply-source` (value = the door the server keeps, `gateway` / `local_proxy`; states
//! `unsaved` while a change waits for Save, `saving` and `reading` while one is with the server,
//! and `via-mac` while the account's way to the plan is the person's Mac). Until the setting has
//! been read, on a server without reply sources, or when it could not be read, the section holds
//! only `settings-reply-source-unavailable`, the line the page draws in place of the form
//! (`Asking the server…` while the section has state `asking`), and Default for new Bots. Once
//! read it holds `settings-reply-source-elsewhere` where the app's server is not on this Mac (the
//! page's line saying the plan is set up only from the server's own Mac);
//! `settings-reply-source-url` (value = the proxy URL shown; `set_value` and `type` write it as
//! typing would, and an empty one clears the address with the next Save; `key` is refused);
//! `settings-reply-source-key` (never valued: states `set` while the server holds a key, `typed`
//! while one waits for Save, which the window draws as masked dots whoever typed it; `set_value`
//! writes the whole key, `type` and `key` are refused; disabled, as the window draws it, while
//! Remove key is picked); `settings-reply-source-remove-key` (while the server holds a key: label
//! `Remove key`, or `Keep key` with state `picked` while its removal waits for Save);
//! `settings-reply-source-health` (label the line, value `running` / `not-running` /
//! `no-address`); `settings-reply-source-providers` (why the Subscription group offers no Claude
//! or Gemini model); `settings-reply-source-error` (the server's words for a refused Save, why
//! nobody knows what became of one, or a read that failed, and after a Save or a page left with a
//! key typed, the line asking for it again; state `trouble` while drawn in the danger colour, a
//! refusal or a failed read); `settings-reply-source-hint` (what Save waits for: an address while
//! the account's replies are on the plan, or a model for the Mac while its way is the Mac); and
//! `settings-reply-source-save` (enabled only while a click would send something). A Save sends
//! the kind the server keeps, as it is, with what changed, and never a plan model or a way. Every
//! control is refused off the page, before the setting is read, and while a Save is out; Save is
//! refused while a read is out too, and says what it waits for; and the plan's controls are
//! refused where the server is not on this Mac. An id the page does not draw is refused as not on
//! the page.
//!
//! Last on the page, whatever the setting, `settings-new-bots`: Default for new Bots, where a
//! newly hired Bot starts (state `unavailable` while the server keeps no default for new Bots,
//! which it does not yet: the contract is being agreed with opengrok-server, and nothing of it is
//! sent or read). It holds `settings-new-bots-unavailable` ("Coming soon: the server can't keep a
//! default for new Bots yet.") and `settings-new-bots-card`, the picker's card (a button named as
//! it reads, `No model · Default`, disabled; a click is refused with why). Both are refused off
//! the page.
//!
//! The Mac relay (hexuria/nativechat #156, opengrok-server #292), from a server that knows it
//! (its setting carries `relay`); a server before it draws none of this. The page no longer moves
//! the account to the Mac or back: the section has state `via-mac` while the server keeps the Mac
//! as the account's way. `settings-relay`, Answer with this Mac: `settings-relay-switch`
//! (a switch, checked while on; it acts at once and waits on no Save, off always, on once this
//! Mac is enrolled; after another Mac took the relay, turning it off and on takes it back),
//! `settings-relay-unavailable` (why the card takes no change: this Mac not enrolled),
//! `settings-relay-status` (label the line, value `answering` / `another-mac` / `connecting` /
//! `not-connected`), `settings-relay-detail` (under it: why this Mac is not connected, state
//! `trouble`, or how to take the relay back), `settings-relay-addr` (value = opencodex's address
//! on this Mac; `set_value` and `type` write it; an address not on this Mac is refused by Save
//! with a hint, an emptied one goes back to the default), `settings-relay-model` (a menu, value
//! = the relay's model shown, state `empty` while no Mac lists one) with
//! `settings-relay-no-model` and a `settings-relay-model-{id}` per model a Mac lists,
//! `settings-relay-models-note`, `settings-relay-key` (never valued: states `set` while this
//! Mac's Keychain holds a key, `typed` while one waits for Save, `retype` when one typed was
//! dropped with the page; `set_value` writes the whole key) and `settings-relay-key-remove`
//! (while a key is kept: `Remove key`, or `Keep key` with state `picked`). The address and key are
//! kept on this Mac by Save, the key in the Keychain, and never sent to the server.
//!
//! `reply-source-{messageId}` is the badge of each reply in the open thread that wears one, as the
//! feed draws it (label `paid key` / `your plan` / `your plan · Mac`, with ` ⚡` after it where the
//! model that answered is a fast twin; value = the door, state `via-mac` on one the person's Mac
//! answered), holding `reply-source-model-{messageId}` (label = the model
//! the server named, which the badge shows on hover) when it named one. A reply whose run
//! ended because the person's plan could not answer (a `RUN_ERROR` code: the relay's, where the
//! Mac could not, or `plan_unavailable`, where the person's own setting left the plan nothing to
//! answer with) keeps its line, and while it is the open thread's last turn the page holds
//! `run-error-send-on-server` (`Send this reply on Server instead`): a click sends the same turn
//! again on the server's paid keys, this once, as `retry-turn` does for a turn that never left. Under `composer-queued`, a
//! `queued-waiting-{messageId}` (`Waiting for your Mac`) for each held message the server holds
//! for the person's Mac (`heldFor: "relay_offline"`). In the bot's settings, while the Bot's
//! replies go through the person's plan, its own door or the account's that it follows,
//! `agent-usage-plan` (the Usage card does not count those replies).
//!
//! Named invokes (parity / gpui-agent): `UserFormContinue`, `UserFormDismiss`,
//! `UserFormOpenScreen`, `UserFormUseSaved`, `UserFormClearSaved` (also kebab
//! `user-form.continue` / `user-form.dismiss` / `user-form.screen` /
//! `user-form.use-saved --arg login_id=…` / `user-form.clear-saved`), `AddSiteLogin`
//! (`logins.add --arg origin= --arg username= --arg password= [--arg label= --arg notes=]`)
//! and `ImportSiteLogins` (`logins.import --arg path=`). Click ids above still work.
//!
//! Settings → Logins: `logins.list` (answers the rows — `id, kind, label, origin, username,
//! on_this_mac, last_used_at_ms`; never a password), `logins.search --arg q=…` (no `q`
//! clears; `set_value` / `type` / `key` on `settings-logins-search` do the same),
//! `logins.select --arg id=…` (no `id` clears the pick; a click on a row does the same),
//! `logins.notes --arg id=… --arg notes=…` (or `set_value` on `settings-login-notes-{id}`).
//! The Add sheet's fields are the window's own: `logins.add` carries the values instead.
//!
//! Routines: `routine.list` (answers with the open bot's rows —
//! `id, name, kind, cron, active, webhook_url, webhook_key`), `routine.create --arg
//! kind=cron|webhook --arg prompt=... [--arg cron=...]`, `routine.rotate --arg id=...`,
//! `routine.delete --arg id=...`.
//!
//! Typing goes in as GPUI keystrokes. `type`, `key` and `set_value` on the composer are
//! planned here ([`ComposePlan`]) and pressed by [`RootView`](crate::root::RootView), because
//! `/` and `@` are keys the composer takes before the text field ever sees them.
//!
//! `/` has no verb of its own: a row is taken the way a person takes it, by typing into
//! `composer-panel-search` and pressing Enter, which is the only path that puts the chip in the
//! message and the thing on the draft together. Two skills may share a name, and Enter takes
//! the first selectable row the search leaves: tell them apart by their description, which the
//! search reads too, or by counting rows under `composer-panel` and arrowing down to the one
//! wanted. `composer-skill`'s value says which of them was actually taken.

mod host;
#[cfg(target_os = "macos")]
mod macos_window;

pub use gpui_agent::mailbox::AgentMailbox;
pub use host::{Command, ComposePlan, NativeChatHost, ids};

use std::time::Duration;

use gpui_agent::security::from_env;
use gpui_agent::server::spawn_mailbox;

fn auth_banner(token_set: bool) -> &'static str {
    if token_set {
        "auth: required (GPUI_AGENT_TOKEN set; clients must send the same token)"
    } else {
        "auth: none (GPUI_AGENT_INSECURE_NO_TOKEN=1 — any local process can drive this host)"
    }
}

/// Start the localhost control plane when `GPUI_AGENT=1`.
pub fn maybe_start() -> Option<AgentMailbox> {
    match from_env() {
        Ok(None) => {
            eprintln!("agent control plane off (set GPUI_AGENT=1 to opt in)");
            None
        }
        Err(err) => {
            eprintln!("agent control plane refused: {err}");
            None
        }
        Ok(Some(config)) => {
            let mailbox = AgentMailbox::new();
            let auth = auth_banner(config.token.is_some());
            match spawn_mailbox(
                config.addr,
                config.token,
                mailbox.clone(),
                Duration::from_secs(30),
            ) {
                Ok((addr, _)) => {
                    eprintln!("gpui-agent listening on {addr} (platform=desktop, app=nativechat)");
                    eprintln!("opt-in: GPUI_AGENT=1 · bind via from_env · protocol v2");
                    eprintln!("{auth}");
                    eprintln!(
                        "screenshot: macOS writes this window via screencapture -l (Screen Recording)"
                    );
                    Some(mailbox)
                }
                Err(err) => {
                    eprintln!("gpui-agent failed to bind: {err}");
                    None
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn screenshot_this_window(
    window: &gpui_kit::Window,
    path: Option<&str>,
) -> Result<gpui_agent::DispatchResult, String> {
    let path = gpui_agent::require_screenshot_path(path)?;
    let _dest = gpui_agent::confine_screenshot_path(path)?;
    let id = macos_window::cgwindow_id(window)?;
    gpui_agent::capture_window_via_screencapture(id, Some(path))
}

#[cfg(not(target_os = "macos"))]
pub fn screenshot_this_window(
    _window: &gpui_kit::Window,
    _path: Option<&str>,
) -> Result<gpui_agent::DispatchResult, String> {
    Err(gpui_agent::screenshot_unavailable(
        "desktop PNG of the app window is macOS-only (`screencapture -l`)",
    ))
}
