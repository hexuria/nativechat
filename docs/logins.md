# Logins: what the app keeps, shows, imports and fills

_21 Sep 2026._

## Where a login lives

A row lives on the server (`site_login`, secrets sealed apart; see opengrok-server `docs/site-logins.md`) and is mirrored in this Mac's sqlite (`site_logins`: id, origin, username, label, kind, notes, last_used_at_ms). The password is in this Mac's keychain under the row's id (service `ai.nativechat.site-login`), the authenticator-code seed under `<id>:otp`. A passkey's key never comes to the Mac.

Sync (`reload_site_logins` → `sync_site_logins`) runs on load and after sign-in: rows the server has and this Mac does not are remembered without their secret; rows saved here before the server knew them are filed there with whatever secret this Mac holds, a code seed included, and take the server's id (the keychain item moves with it); words (title, notes) follow the newer stamp, in either direction, so a note the server refused at the time goes up on the next sync. A secret missing here is fetched once from the server after Touch ID (`reveal_site_login_secrets`) and cached; a revealed seed can mint a code straight away.

A save never changes what a row is: an import that carries only a code for a login already saved with a password leaves it a password row.

## Settings → Logins

One pane on the left: a search field and a "+" (the Add sheet: Title, User Name, Password, Website, Notes), then Passwords, Passkeys and Codes as sections with a count badge and their rows (icon, title, account), Security only when a row carries a `Security:` note, and Import… at the bottom. The detail pane shows User Name, Password as dots (never read), Website, a live code with its seconds left when the row has a seed, Notes (edited in place), where the password is, when a bot last used it, when it was added, and Delete.

Knowing which rows the keychain holds — a password, a code seed — is asked by name, which
needs no unlocking; the secret itself is read only when the person opens that row or after
Touch ID on a card. That is why the list and the page paint without a keychain sheet.

The keychain trusts a program by its signature, not its path, so an unsigned build is a
stranger to it after every compile and the first read puts the OS password sheet up
whatever Touch ID just said. `scripts/sign-dev.sh` signs the dev binary with the team's
Developer ID — the same identity the shipped app uses — so the permission given once
carries from build to build and on to the shipped app. `just run` does it as part of the
loop; a build launched some other way should be signed the same way.

Site icons come from the server's `GET /site-logins/icon/{origin}`, one request per site, misses remembered; a site with none shows its first letter.

## Import

`Import…` takes a file or a directory (`src/site_login/importers/`):

| source | what | codes |
|---|---|---|
| Passwords app (macOS) | CSV `Title,URL,Username,Password,Notes,OTPAuth` | `OTPAuth` |
| Chrome | CSV `name,url,username,password,note` | — |
| 1Password | 1PUX (ZIP, `export.data`) or CSV | `OTPAuth` / `One-time password` |
| LastPass | CSV `url,username,password,totp,extra,name,…` | `totp` (bare seed) |
| Bitwarden | unencrypted JSON `items[].login` | `login.totp` (URI or seed) |
| `pass` | a directory: one GPG file per entry, read with `pass show` against that directory, with the person's own key | an `otpauth://` line |

Every row becomes the same item (site, name, password, seed, title, notes); a bare seed becomes an `otpauth://` URI with the usual defaults. The file is read once and not kept. From `pass`, only a line that calls itself a note (`note:`, `notes:`, `comment:`) becomes notes: the rest of an entry was kept encrypted for a reason. An entry in a folder takes the folder it sits in as its site, and the first entry that will not decrypt ends the read rather than putting the same passphrase sheet up for every one after it. Apple's Credential Exchange (in-memory hand-off of passwords, passkeys and codes) needs the signed app with a credential-provider extension; the receiver is in `macos/CredentialExchange/` and is not built into the dev app.

## The card

The accounts are offered the way a browser's autofill does. A card arrives with nothing
showing; a click in the name field brings the list up, floating over the card so nothing
moves, and it opens upward instead when the composer is in the way. A click anywhere else
puts it away and lets the field go, so clicking the field again brings it back. The driver
tree lists the accounts whether or not the list is up, since a pick by id does not need it
painted.

A `request_user_form` card knows what it takes (`site_login::card_target`): a login (name and password fields), a code (one `otp` field), or a passkey (no fields, `challengeKind: "passkey"`). The rows of that kind for the card's site are listed under the field. A pick puts up Touch ID (`site_login::touch_id`, LocalAuthentication, the Mac password as fallback); then:

- a login: both fields lock with the picked name and dots; Log in sends the password held in memory, marked `savedLogin` with the row's id;
- a code: the card lists the rows whose seed is on this Mac (a row synced from another Mac offers its code once its password has been used here, which fetches the seed with it); the field locks; Continue mints the six digits at that moment (`site_login::totp`; a step with fewer than eight seconds left waits for the next one, and the card is busy until it does, so Continue cannot be pressed twice). A seed this Mac cannot read says so on the card instead of sending an empty field;
- a passkey: "Passkey for … ready"; Use passkey sends only the row's id, and the server does the rest in the bot's browser. A register-mode card is one row to confirm; its Touch ID sheet asks to let the site create a passkey, and names no account, because the site makes the key for whichever account is signed in there.

Change frees a held pick. A pick does not outlive its card: a settled, dismissed or superseded card, a fill that did not end in Submitted, and a sign-out all drop it. The secret is never painted, never put in an input, and the Bot never sees it. A 403 from the server (a shared computer) brings the fields back with the reason.

Touch ID here is the system sheet, not a keychain access control: the ad-hoc dev build cannot carry the entitlement the item-level gate needs. That is the step after the app ships signed.

## Driver ids

See `src/agent/mod.rs`: `settings-logins-search`, `settings-login-add`, `settings-login-import`, `settings-logins-group-*`, `settings-login-row-{id}`, the detail ids, the Add sheet ids; `user-form-use-saved-{key}-{login}`, `user-form-saved-clear-{key}`, `user-form-passkey-register-{key}`; invokes `logins.list/search/select/add/notes/import`, `user-form.use-saved`, `user-form.clear-saved`, `user-form.register-passkey`.
