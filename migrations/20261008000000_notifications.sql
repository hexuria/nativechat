-- What went wrong, per Bot, kept on this Mac until the person clears it (8 Oct 2026). A failed turn
-- or a refused change used to live only where it happened: a line in the chat that could not be
-- dismissed, or a word under a control that went when the control did. Each row is one such
-- failure, with where in the app it was caught, so it can be traced back and copied.
--
-- `bot_id` is null for one that belongs to no Bot (signing in, the server going away). `code` is
-- the source file and line the app caught it at. `raw` is the server's own words when it sent
-- more than the sentence shown. Nothing secret is kept here: no request bodies, no keys.
CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL,
    at_ms INTEGER NOT NULL,
    bot_id TEXT,
    place TEXT NOT NULL,
    code TEXT NOT NULL,
    said TEXT NOT NULL,
    raw TEXT,
    run_id TEXT,
    read INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS notifications_by_bot ON notifications (bot_id, at_ms);
