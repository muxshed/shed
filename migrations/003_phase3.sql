-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

CREATE TABLE IF NOT EXISTS guests (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
