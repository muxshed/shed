-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

CREATE TABLE IF NOT EXISTS channel (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    token TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT 'Muxshed Stream',
    logo_path TEXT,
    accent TEXT,
    password_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
