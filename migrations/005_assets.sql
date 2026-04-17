-- Licensed under the Business Source License 1.1 — see LICENSE.

CREATE TABLE IF NOT EXISTS assets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    asset_type TEXT NOT NULL DEFAULT 'video',
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    mime_type TEXT NOT NULL DEFAULT '',
    folder_id TEXT,
    loop_mode TEXT NOT NULL DEFAULT 'one_shot',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    start_ms INTEGER NOT NULL DEFAULT 0,
    opaque_ms INTEGER NOT NULL DEFAULT 0,
    clear_ms INTEGER NOT NULL DEFAULT 0,
    end_ms INTEGER NOT NULL DEFAULT 0,
    audio_behaviour TEXT NOT NULL DEFAULT '"silent"',
    thumbnail_path TEXT NOT NULL DEFAULT '',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    color TEXT NOT NULL DEFAULT '#6366f1',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Migrate existing stingers to assets
INSERT OR IGNORE INTO assets (id, name, asset_type, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path)
SELECT id, name, 'stinger', file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path
FROM stingers;
