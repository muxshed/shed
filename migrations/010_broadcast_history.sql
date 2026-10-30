-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.
-- Muxshed Phase 4. Local observation history (not a remote YouTube SLA).
CREATE TABLE IF NOT EXISTS history_sessions (
 id TEXT PRIMARY KEY NOT NULL,
 started_at TEXT NOT NULL,
 last_seen_at TEXT NOT NULL,
 ended_at TEXT,
 end_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_history_sessions_started ON history_sessions(started_at);
CREATE TABLE IF NOT EXISTS history_events (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 at_utc TEXT NOT NULL,
 session_id TEXT,
 event_type TEXT NOT NULL,
 subject TEXT,
 detail TEXT,
 duration_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_history_events_time ON history_events(at_utc);
CREATE TABLE IF NOT EXISTS history_samples (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 observed_at TEXT NOT NULL,
 broadcast_live INTEGER NOT NULL,
 program_source_id TEXT,
 failover_active INTEGER NOT NULL,
 encoder_healthy INTEGER,
 encoder_destinations INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_history_samples_time ON history_samples(observed_at);
