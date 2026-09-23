-- When a coworker's reply finished, and (when the harness sent one) the
-- phase breakdown of that turn.
--
-- `created_at` is when the run began: the thread is ordered by that, so a
-- long turn that started before a queued message still sits before it after
-- a reload. The peek stamp needs the other end of the wait, or a six-minute
-- `profile.list` still wears 4:34 PM as if it answered instantly.
ALTER TABLE chat_messages ADD COLUMN finished_at TEXT;
ALTER TABLE chat_messages ADD COLUMN run_timing TEXT;
