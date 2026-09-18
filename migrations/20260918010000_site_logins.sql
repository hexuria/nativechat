-- Site-login metadata only. The password is never a column here: it lives in
-- the OS keychain (service ai.nativechat.site-login) or, when Keychain is
-- unavailable, data_dir/site-login.vault. Do not add a password / secret /
-- blob column. The existing `credentials` table is LLM API keys — do not
-- reuse it for site logins.

CREATE TABLE IF NOT EXISTS site_logins (
    id TEXT PRIMARY KEY NOT NULL,
    origin TEXT NOT NULL,
    username TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS site_logins_origin_username
    ON site_logins (origin, username);
