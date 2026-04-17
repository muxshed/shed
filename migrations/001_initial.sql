-- Licensed under the Business Source License 1.1 — see LICENSE.

CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS scenes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS scene_layers (
    id TEXT PRIMARY KEY NOT NULL,
    scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    x INTEGER NOT NULL DEFAULT 0,
    y INTEGER NOT NULL DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 1920,
    height INTEGER NOT NULL DEFAULT 1080,
    z_index INTEGER NOT NULL DEFAULT 0,
    opacity REAL NOT NULL DEFAULT 1.0
);

CREATE TABLE IF NOT EXISTS stingers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    start_ms INTEGER NOT NULL DEFAULT 0,
    opaque_ms INTEGER NOT NULL DEFAULT 0,
    clear_ms INTEGER NOT NULL DEFAULT 0,
    end_ms INTEGER NOT NULL DEFAULT 0,
    audio_behaviour TEXT NOT NULL DEFAULT 'silent',
    thumbnail_path TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS destinations (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    scopes TEXT NOT NULL DEFAULT '["read","control","admin"]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE TABLE IF NOT EXISTS delay_config (
    id INTEGER PRIMARY KEY DEFAULT 1,
    enabled INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 7000,
    whisper_enabled INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO delay_config (id) VALUES (1);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
