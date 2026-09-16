-- A reply's quote belongs to the message that answers it, so a reload keeps the quote on the
-- bubble and still tells the coworker what the person was answering.
ALTER TABLE chat_messages ADD COLUMN reply_to_id TEXT;
ALTER TABLE chat_messages ADD COLUMN reply_preview TEXT;
ALTER TABLE chat_messages ADD COLUMN reply_is_me INTEGER;
