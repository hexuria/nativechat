-- A turn is what the person saw: some words, a picture of the box's screen, more words. Only the
-- flattened text was kept, so a reloaded thread lost every picture and read back as one run-on
-- bubble. The pieces get their own rows, in the order they were seen.
--
-- The picture is a BLOB here rather than base64 on the message row because `content` is read for
-- every message in a thread and flattened into the history the model is sent; a megabyte of PNG
-- has no business riding along with it.
CREATE TABLE chat_message_parts (
    message_id TEXT NOT NULL,
    ord INTEGER NOT NULL,
    kind TEXT NOT NULL, -- 'text' or 'screenshot'
    text TEXT,          -- a text part's words, or a screenshot's caption
    call_id TEXT,
    image BLOB,
    width INTEGER,
    height INTEGER,
    PRIMARY KEY (message_id, ord),
    FOREIGN KEY (message_id) REFERENCES chat_messages(id) ON DELETE CASCADE
);
