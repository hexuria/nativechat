CREATE TABLE models (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    model_type TEXT NOT NULL,
    input_token_limit INTEGER,
    output_token_limit INTEGER,
    capabilities TEXT NOT NULL, -- JSON blob
    is_thinking BOOLEAN DEFAULT FALSE,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
