-- Settings → Logins grows three fields. Still no password column here: the
-- kind says what the row holds (password | code | passkey), the notes are the
-- person's own words about it, and last_used_at_ms is when the server last saw
-- the login used (unix milliseconds; NULL until it has been).

ALTER TABLE site_logins ADD COLUMN kind TEXT NOT NULL DEFAULT 'password';
ALTER TABLE site_logins ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE site_logins ADD COLUMN last_used_at_ms INTEGER;
