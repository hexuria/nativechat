-- Initial database schema

CREATE TABLE IF NOT EXISTS credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    api_key TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    text_credential_id INTEGER,
    embedding_credential_id INTEGER,
    image_credential_id INTEGER,
    text_model_id TEXT,
    embedding_model_id TEXT,
    image_model_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (text_credential_id) REFERENCES credentials(id) ON DELETE SET NULL,
    FOREIGN KEY (embedding_credential_id) REFERENCES credentials(id) ON DELETE SET NULL,
    FOREIGN KEY (image_credential_id) REFERENCES credentials(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
