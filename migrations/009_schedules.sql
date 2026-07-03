-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

CREATE TABLE IF NOT EXISTS schedules (
    id               TEXT PRIMARY KEY NOT NULL,
    name             TEXT NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 1,
    trigger_kind     TEXT NOT NULL DEFAULT 'once',   -- 'once' | 'cron'
    trigger_at       TEXT,                           -- ISO-8601 local (system tz), for 'once'
    trigger_cron     TEXT,                           -- 5-field cron, for 'cron'
    destination_ids  TEXT NOT NULL DEFAULT '[]',     -- JSON array of destination ids
    standby_asset_id TEXT,
    end_behavior     TEXT NOT NULL DEFAULT 'stop',   -- 'stop' | 'loop' | 'standby'
    until_at         TEXT,                           -- optional hard stop (ISO local)
    next_run_at      TEXT,                           -- computed UTC instant (RFC3339)
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schedule_items (
    id          TEXT PRIMARY KEY NOT NULL,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    item_kind   TEXT NOT NULL DEFAULT 'vod',         -- 'vod' | 'scene' | 'source'
    ref_id      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schedule_runs (
    id          TEXT PRIMARY KEY NOT NULL,
    schedule_id TEXT NOT NULL,
    started_at  TEXT,
    ended_at    TEXT,
    status      TEXT NOT NULL                        -- 'ran' | 'skipped' | 'error' | 'missed'
);

CREATE INDEX IF NOT EXISTS idx_schedule_items_schedule ON schedule_items(schedule_id);
