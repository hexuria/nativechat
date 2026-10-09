-- A notification can be a fault: a read that failed at one place in the app (9 Oct 2026). The ⚠
-- badge on that place is the newest unread fault there, so marking the notification read hides
-- the badge and marking it unread brings it back; the next read that works resolves it.
--
-- `fault_place` is the place's word (`usage`, `tools`, `models`, …), null for a notice that is
-- not a fault. `endpoint` is the request it answered, method and path, never a body or a key.
-- The same failure again bumps `count` and `last_ms` rather than adding a row.
ALTER TABLE notifications ADD COLUMN fault_place TEXT;
ALTER TABLE notifications ADD COLUMN endpoint TEXT;
ALTER TABLE notifications ADD COLUMN status INTEGER;
ALTER TABLE notifications ADD COLUMN count INTEGER NOT NULL DEFAULT 1;
ALTER TABLE notifications ADD COLUMN last_ms INTEGER;
ALTER TABLE notifications ADD COLUMN resolved INTEGER NOT NULL DEFAULT 0;
