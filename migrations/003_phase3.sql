-- Licensed under the Business Source License 1.1 — see LICENSE.

CREATE TABLE IF NOT EXISTS guests (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
