# VOD Scheduling — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Schedule a single uploaded VOD to auto-go-live to chosen destinations at a set time (one-off or cron, in an owner-set system timezone), play it out with a standby card around it, and auto-stop when it ends.

**Architecture:** A background **scheduler supervisor** (same pattern as `failover.rs`) computes each schedule's next run in the system timezone and, at fire time, starts a **playout controller** unless already live (skip+notify). The playout controller reuses `media_player` (VOD → relay), `program_intent` (routing), `egress` (fan-out), and the failover-fallback pattern (standby card). Everything persists in SQLite. Built so Phases 2 (playlists) and 3 (any-content) are additive.

**Tech Stack:** Rust (Axum, Tokio, sqlx runtime queries, thiserror, tracing), `cron` + `chrono` + `chrono-tz` for DST-safe scheduling, SvelteKit (Svelte 5 runes, Tailwind), SQLite.

---

## File structure

| File | Responsibility |
|------|----------------|
| `migrations/009_schedules.sql` | `schedules`, `schedule_items`, `schedule_runs` tables |
| `crates/common/src/types.rs` (modify) | `Schedule`, `ScheduleItem`, `ScheduleRun`, `TriggerKind`, `EndBehavior`, `ScheduleItemKind` enums/structs |
| `crates/common/src/events.rs` (modify) | `ScheduleStarted` / `ScheduleEnded` / `ScheduleSkipped` `WsEvent` variants |
| `crates/api/src/schedule_time.rs` (create) | Pure next-run computation from a trigger + system tz (DST-safe). Unit-tested in isolation. |
| `crates/api/src/playout.rs` (create) | `start_broadcast` / `stop_broadcast`: egress + standby + VOD + duration-based end |
| `crates/api/src/scheduler.rs` (create) | Background supervisor: compute next run, fire, skip+notify |
| `crates/api/src/routes/schedules.rs` (create) | CRUD + items + enable/disable + run-now + timezone get/set |
| `crates/api/src/state.rs` (modify) | `active_schedule: watch::Sender<Option<Uuid>>`, `schedule_nudge: watch::Sender<u64>` |
| `crates/api/src/main.rs` (modify) | create channels, spawn scheduler |
| `crates/api/src/routes/mod.rs` (modify) | register `/schedules*` and `/settings/timezone` routes |
| `crates/api/src/lib.rs` (modify) | `pub mod playout; pub mod scheduler; pub mod schedule_time;` |
| `crates/api/Cargo.toml` (modify) | add `cron`, `chrono-tz` |
| `web/src/lib/types.ts` (modify) | TS types mirroring the common types |
| `web/src/lib/api.ts` (modify) | schedule + timezone API client methods |
| `web/src/lib/stores/pipeline.ts` (modify) | `activeSchedule` store |
| `web/src/lib/ws.ts` (modify) | handle `schedule_*` events |
| `web/src/routes/(app)/schedules/+page.svelte` (create) | Schedules list + editor |
| `web/src/routes/(app)/+layout.svelte` (modify) | add "Schedules" nav item |
| `web/src/routes/(app)/settings/+page.svelte` (modify) | system timezone picker |

Convention notes (from `system/CLAUDE.md`): every new `.sql`/`.rs` file starts with the AGPL header `// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.` (SQL uses `--`). Use `sqlx::query`/`query_as` runtime queries (no compile-time macros). No `unwrap()` in library code.

---

## Task 1: Dependencies + migration

**Files:**
- Modify: `crates/api/Cargo.toml`
- Create: `migrations/009_schedules.sql`

- [ ] **Step 1: Add crates**

In `crates/api/Cargo.toml` under `[dependencies]`, after the existing `chrono` line add:

```toml
cron = "0.12"
chrono-tz = "0.9"
```

- [ ] **Step 2: Create the migration**

Create `migrations/009_schedules.sql`:

```sql
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
```

- [ ] **Step 3: Verify it builds and migrates**

Run: `cargo build -p muxshed-api`
Expected: `Finished` (migrations run at API startup via `sqlx::migrate!`; a compile is enough to confirm the new deps resolve).

- [ ] **Step 4: Commit**

```bash
git add crates/api/Cargo.toml Cargo.lock migrations/009_schedules.sql
git commit -m "feat(schedules): add cron/chrono-tz deps and schedules migration"
```

---

## Task 2: Common types

**Files:**
- Modify: `crates/common/src/types.rs` (add types + `#[cfg(test)]` cases)

- [ ] **Step 1: Write the failing serialization test**

Append inside the existing `#[cfg(test)] mod tests` block in `crates/common/src/types.rs`:

```rust
#[test]
fn schedule_types_roundtrip() {
    let s = Schedule {
        id: Uuid::nil(),
        name: "Premiere".into(),
        enabled: true,
        trigger: TriggerKind::Cron { expr: "0 20 * * *".into() },
        destination_ids: vec![Uuid::nil()],
        standby_asset_id: Some(Uuid::nil()),
        end_behavior: EndBehavior::Stop,
        until_at: None,
        items: vec![ScheduleItem { id: Uuid::nil(), position: 0, kind: ScheduleItemKind::Vod, ref_id: Uuid::nil() }],
        next_run_at: None,
    };
    let back: Schedule = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.name, "Premiere");
    assert!(matches!(back.trigger, TriggerKind::Cron { .. }));
    assert!(matches!(back.end_behavior, EndBehavior::Stop));
    // enum wire format is snake_case + tagged
    let v = serde_json::to_value(&EndBehavior::Loop).unwrap();
    assert_eq!(v, serde_json::json!("loop"));
    let t = serde_json::to_value(&TriggerKind::Once { at: "2026-07-02T20:00:00".into() }).unwrap();
    assert_eq!(t["kind"], "once");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p muxshed-common schedule_types_roundtrip`
Expected: FAIL — `cannot find type Schedule`.

- [ ] **Step 3: Add the types**

Add near the other config types in `crates/common/src/types.rs` (e.g. after `FailoverConfig`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EndBehavior {
    #[default]
    Stop,
    Loop,
    Standby,
}

impl EndBehavior {
    pub fn as_str(&self) -> &'static str {
        match self { EndBehavior::Stop => "stop", EndBehavior::Loop => "loop", EndBehavior::Standby => "standby" }
    }
    pub fn from_db(s: &str) -> Self {
        match s { "loop" => EndBehavior::Loop, "standby" => EndBehavior::Standby, _ => EndBehavior::Stop }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleItemKind {
    Vod,
    Scene,
    Source,
}

impl ScheduleItemKind {
    pub fn as_str(&self) -> &'static str {
        match self { ScheduleItemKind::Vod => "vod", ScheduleItemKind::Scene => "scene", ScheduleItemKind::Source => "source" }
    }
    pub fn from_db(s: &str) -> Self {
        match s { "scene" => ScheduleItemKind::Scene, "source" => ScheduleItemKind::Source, _ => ScheduleItemKind::Vod }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerKind {
    /// One-off local datetime in the system timezone, e.g. "2026-07-02T20:00:00".
    Once { at: String },
    /// 5-field cron (min hour dom mon dow), evaluated in the system timezone.
    Cron { expr: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleItem {
    pub id: Uuid,
    pub position: u32,
    pub kind: ScheduleItemKind,
    pub ref_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub trigger: TriggerKind,
    pub destination_ids: Vec<Uuid>,
    pub standby_asset_id: Option<Uuid>,
    pub end_behavior: EndBehavior,
    pub until_at: Option<String>,
    pub items: Vec<ScheduleItem>,
    /// Computed next fire time as an RFC3339 UTC instant. None = not scheduled.
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRun {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub status: String,
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p muxshed-common schedule_types_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/types.rs
git commit -m "feat(schedules): common Schedule/ScheduleItem/TriggerKind types"
```

---

## Task 3: DST-safe next-run computation (pure, unit-tested)

**Files:**
- Create: `crates/api/src/schedule_time.rs`
- Modify: `crates/api/src/lib.rs` (add `pub mod schedule_time;`)

This is the novel/risky part — isolate it so it can be tested without a server.

- [ ] **Step 1: Add the module declaration**

In `crates/api/src/lib.rs`, add near the other `pub mod` lines:

```rust
pub mod schedule_time;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/api/src/schedule_time.rs`:

```rust
// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Pure next-run computation for schedule triggers, evaluated in a system
//! timezone. DST-correct: "20:00 daily" stays 20:00 local across clock changes.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use muxshed_common::TriggerKind;
use std::str::FromStr;

/// Parse an IANA timezone name; falls back to UTC on anything unknown.
pub fn parse_tz(name: &str) -> Tz {
    name.parse::<Tz>().unwrap_or(chrono_tz::UTC)
}

/// The `cron` crate expects a 6-field expression (with seconds). We accept the
/// standard 5-field form from users and prepend a "0" seconds field.
fn to_six_field(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 { format!("0 {}", expr.trim()) } else { expr.trim().to_string() }
}

/// Next run strictly after `after` (a UTC instant), for the given trigger in `tz`.
/// Returns None for one-offs already in the past, or an unparseable cron.
pub fn next_run_after(trigger: &TriggerKind, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match trigger {
        TriggerKind::Once { at } => {
            // `at` is a naive local datetime in `tz`; convert to UTC.
            let naive = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M"))
                .ok()?;
            let local = tz.from_local_datetime(&naive).single()?;
            let utc = local.with_timezone(&Utc);
            if utc > after { Some(utc) } else { None }
        }
        TriggerKind::Cron { expr } => {
            let schedule = cron::Schedule::from_str(&to_six_field(expr)).ok()?;
            let after_local = after.with_timezone(&tz);
            schedule.after(&after_local).next().map(|dt| dt.with_timezone(&Utc))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn daily_cron_holds_local_time_across_dst() {
        let tz = parse_tz("Europe/London");
        // Just before the spring-forward night (clocks go +1 at 01:00 on 2026-03-29).
        let after = utc("2026-03-28T19:30:00Z"); // 19:30 GMT
        let next = next_run_after(&TriggerKind::Cron { expr: "0 20 * * *".into() }, tz, after).unwrap();
        // 20:00 London on 2026-03-28 (still GMT) == 20:00Z
        assert_eq!(next, utc("2026-03-28T20:00:00Z"));

        // After the DST switch, 20:00 London is BST == 19:00Z.
        let after2 = utc("2026-03-30T10:00:00Z");
        let next2 = next_run_after(&TriggerKind::Cron { expr: "0 20 * * *".into() }, tz, after2).unwrap();
        assert_eq!(next2, utc("2026-03-30T19:00:00Z"));
    }

    #[test]
    fn once_in_past_is_none() {
        let tz = parse_tz("UTC");
        let after = utc("2026-07-02T21:00:00Z");
        assert!(next_run_after(&TriggerKind::Once { at: "2026-07-02T20:00:00".into() }, tz, after).is_none());
    }

    #[test]
    fn once_future_converts_local_to_utc() {
        let tz = parse_tz("America/New_York"); // UTC-4 in July (EDT)
        let after = utc("2026-07-02T10:00:00Z");
        let next = next_run_after(&TriggerKind::Once { at: "2026-07-02T20:00:00".into() }, tz, after).unwrap();
        assert_eq!(next, utc("2026-07-03T00:00:00Z")); // 20:00 EDT == 00:00Z next day
    }

    #[test]
    fn bad_cron_is_none() {
        let tz = parse_tz("UTC");
        assert!(next_run_after(&TriggerKind::Cron { expr: "not a cron".into() }, tz, Utc::now()).is_none());
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p muxshed-api schedule_time`
Expected: FAIL (module/deps not yet compiled) — then after deps compile, the tests exist.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p muxshed-api schedule_time`
Expected: PASS (4 tests). If `once_future_converts_local_to_utc` fails, confirm the machine's `chrono-tz` DST table matches (EDT = UTC-4).

- [ ] **Step 5: Commit**

```bash
git add crates/api/src/schedule_time.rs crates/api/src/lib.rs
git commit -m "feat(schedules): DST-safe next-run computation with tests"
```

---

## Task 4: State channels + WS events

**Files:**
- Modify: `crates/common/src/events.rs`
- Modify: `crates/api/src/state.rs`
- Modify: `crates/api/src/main.rs`

- [ ] **Step 1: Add WS event variants**

In `crates/common/src/events.rs`, add before the `Error` variant in `enum WsEvent`:

```rust
    ScheduleStarted { id: Uuid },
    ScheduleEnded { id: Uuid },
    ScheduleSkipped { id: Uuid, reason: String },
```

- [ ] **Step 2: Add state channels**

In `crates/api/src/state.rs`, in `struct AppState` after `failover_active`:

```rust
    /// The schedule whose broadcast is currently on air (None = manual/idle).
    pub active_schedule: watch::Sender<Option<Uuid>>,
    /// Bumped whenever schedules or the system timezone change, to wake the scheduler.
    pub schedule_nudge: watch::Sender<u64>,
```

- [ ] **Step 3: Wire channels in main.rs**

In `crates/api/src/main.rs`, next to the other `watch::channel` inits (near `failover_active_tx`):

```rust
    let (active_schedule_tx, _active_schedule_rx) = watch::channel::<Option<uuid::Uuid>>(None);
    let (schedule_nudge_tx, _schedule_nudge_rx) = watch::channel::<u64>(0);
```

And in the `AppState { .. }` literal after `failover_active: failover_active_tx,`:

```rust
        active_schedule: active_schedule_tx,
        schedule_nudge: schedule_nudge_tx,
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build -p muxshed-api`
Expected: `Finished` (note: `crates/api/tests/api_tests.rs` constructs `AppState` directly — add the two channels there too, mirroring how `program_intent`/`failover_active` were added, or the test target won't compile). Add in that test file next to `failover_active_tx`:

```rust
    let (active_schedule_tx, _) = tokio::sync::watch::channel::<uuid::Uuid>(None).0; // see note
```

Correct form (matches production):

```rust
    let (active_schedule_tx, _) = tokio::sync::watch::channel::<Option<uuid::Uuid>>(None);
    let (schedule_nudge_tx, _) = tokio::sync::watch::channel::<u64>(0);
```

and in the test's `AppState { .. }`: `active_schedule: active_schedule_tx, schedule_nudge: schedule_nudge_tx,`.

Run: `cargo build -p muxshed-api --tests`
Expected: `Finished`.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/events.rs crates/api/src/state.rs crates/api/src/main.rs crates/api/tests/api_tests.rs
git commit -m "feat(schedules): schedule state channels + WS events"
```

---

## Task 5: Playout controller

**Files:**
- Create: `crates/api/src/playout.rs`
- Modify: `crates/api/src/lib.rs` (add `pub mod playout;`)

Reuses `media_player::start_media_playback`, `program_intent`, `egress`, and (for standby) the failover-fallback approach. Phase-1 end detection is **duration-based** using the asset's `duration_ms`.

- [ ] **Step 1: Add module declaration**

In `crates/api/src/lib.rs`: `pub mod playout;`

- [ ] **Step 2: Implement the controller**

Create `crates/api/src/playout.rs`:

```rust
// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Playout controller: airs a schedule's content to program + destinations.
//! Phase 1 = a single VOD with a standby card around it and duration-based end.

use crate::state::AppState;
use muxshed_common::{DestinationKind, EndBehavior, WsEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

struct VodItem {
    asset_id: Uuid,
    file_path: PathBuf,
    duration_ms: u64,
}

/// Air the schedule now. Spawns a task that runs the broadcast and returns.
pub async fn start_broadcast(state: Arc<AppState>, schedule_id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = run_broadcast(&state, schedule_id).await {
            tracing::warn!("playout: broadcast {} error: {}", schedule_id, e);
            record_run(&state, schedule_id, "error").await;
            let _ = state.active_schedule.send(None);
        }
    });
}

async fn run_broadcast(state: &Arc<AppState>, schedule_id: Uuid) -> Result<(), String> {
    // 1. Load the schedule's first VOD item + its asset (Phase 1 = single VOD).
    let vod = load_first_vod(state, schedule_id).await?;
    let end_behavior = load_end_behavior(state, schedule_id).await;
    let standby_asset = load_standby_asset(state, schedule_id).await;
    let destinations = load_destinations(state, schedule_id).await;

    // 2. Mark on air + notify.
    let _ = state.active_schedule.send(Some(schedule_id));
    record_run(state, schedule_id, "ran").await;
    let _ = state.ws_tx.send(WsEvent::ScheduleStarted { id: schedule_id });
    tracing::info!("playout: schedule {} on air (vod {})", schedule_id, vod.asset_id);

    // 3. Bring up the standby card first so the stream is never black, then egress.
    let standby_id = ensure_standby(state, &standby_asset).await;
    let cfg = load_output_config(state).await;
    let seq = if let Some(sid) = standby_id { state.sequence_headers.read().await.get(&sid).cloned() } else { None };
    if let Err(e) = state.egress.start(schedule_id, destinations, state.program_tx.clone(), Some(cfg), seq).await {
        tracing::warn!("playout: egress start error (continuing): {}", e);
    }
    if let Some(sid) = standby_id { let _ = state.program_intent.send(Some(sid)); }
    state.pipeline.start(Vec::new()).await.ok();

    // 4. Start the VOD (one-shot for stop/standby, looping for loop) and cut to it.
    let vod_source = Uuid::new_v4();
    let loop_mode = if matches!(end_behavior, EndBehavior::Loop) { "loop" } else { "one_shot" };
    crate::media_player::start_media_playback(state.clone(), vod_source, &vod.file_path, loop_mode)
        .await
        .map_err(|e| format!("vod playback: {}", e))?;
    // Give the encoder a moment to produce its first frame, then cut.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = state.program_intent.send(Some(vod_source));
    crate::egress_restart_for(state, vod_source).await;

    // 5. Wait for the VOD to finish (duration-based), unless looping (runs until stopped).
    if !matches!(end_behavior, EndBehavior::Loop) {
        let dur = Duration::from_millis(vod.duration_ms.max(1000));
        tokio::select! {
            _ = tokio::time::sleep(dur) => {}
            _ = wait_until_deactivated(state, schedule_id) => {
                crate::media_player::stop_media_playback(state, &vod_source).await;
                return Ok(()); // externally stopped
            }
        }
        // 6. End behavior.
        crate::media_player::stop_media_playback(state, &vod_source).await;
        match end_behavior {
            EndBehavior::Standby => {
                if let Some(sid) = standby_id { let _ = state.program_intent.send(Some(sid)); crate::egress_restart_for(state, sid).await; }
            }
            _ => {
                state.egress.stop().await;
                state.pipeline.stop().await.ok();
                let _ = state.program_intent.send(None);
                if let Some(sid) = standby_id { crate::media_player::stop_media_playback(state, &sid).await; }
                let _ = state.active_schedule.send(None);
                let _ = state.ws_tx.send(WsEvent::ScheduleEnded { id: schedule_id });
                mark_run_ended(state, schedule_id).await;
            }
        }
    }
    Ok(())
}

/// Stop the currently-airing broadcast for this schedule (manual stop).
pub async fn stop_broadcast(state: &AppState) {
    let id = *state.active_schedule.borrow();
    state.egress.stop().await;
    state.pipeline.stop().await.ok();
    let _ = state.program_intent.send(None);
    let _ = state.active_schedule.send(None);
    if let Some(id) = id {
        let _ = state.ws_tx.send(WsEvent::ScheduleEnded { id });
        mark_run_ended(state, id).await;
    }
}

async fn wait_until_deactivated(state: &Arc<AppState>, schedule_id: Uuid) {
    let mut rx = state.active_schedule.subscribe();
    loop {
        if *rx.borrow_and_update() != Some(schedule_id) { return; }
        if rx.changed().await.is_err() { return; }
    }
}

async fn ensure_standby(state: &Arc<AppState>, standby: &Option<VodItem>) -> Option<Uuid> {
    let s = standby.as_ref()?;
    let id = Uuid::new_v4();
    let loop_mode = "loop";
    let _ = crate::media_player::start_media_playback(state.clone(), id, &s.file_path, loop_mode).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    Some(id)
}

async fn load_first_vod(state: &AppState, schedule_id: Uuid) -> Result<VodItem, String> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT ref_id FROM schedule_items WHERE schedule_id = ? AND item_kind = 'vod' ORDER BY position ASC LIMIT 1",
    )
    .bind(schedule_id.to_string())
    .fetch_optional(&state.db).await.map_err(|e| e.to_string())?
    .ok_or("schedule has no VOD item")?;
    let asset_id: Uuid = row.0.parse().map_err(|_| "bad asset id")?;
    let a = sqlx::query_as::<_, (String, i64)>("SELECT file_path, duration_ms FROM assets WHERE id = ?")
        .bind(asset_id.to_string())
        .fetch_optional(&state.db).await.map_err(|e| e.to_string())?
        .ok_or("VOD asset not found")?;
    Ok(VodItem { asset_id, file_path: PathBuf::from(a.0), duration_ms: a.1.max(0) as u64 })
}

async fn load_standby_asset(state: &AppState, schedule_id: Uuid) -> Option<VodItem> {
    let sid: Option<(Option<String>,)> =
        sqlx::query_as("SELECT standby_asset_id FROM schedules WHERE id = ?")
            .bind(schedule_id.to_string()).fetch_optional(&state.db).await.ok().flatten();
    let asset_id = sid.and_then(|r| r.0)?;
    let a = sqlx::query_as::<_, (String, i64)>("SELECT file_path, duration_ms FROM assets WHERE id = ?")
        .bind(&asset_id).fetch_optional(&state.db).await.ok().flatten()?;
    Some(VodItem { asset_id: asset_id.parse().ok()?, file_path: PathBuf::from(a.0), duration_ms: a.1.max(0) as u64 })
}

async fn load_end_behavior(state: &AppState, schedule_id: Uuid) -> EndBehavior {
    sqlx::query_as::<_, (String,)>("SELECT end_behavior FROM schedules WHERE id = ?")
        .bind(schedule_id.to_string()).fetch_optional(&state.db).await.ok().flatten()
        .map(|r| EndBehavior::from_db(&r.0)).unwrap_or_default()
}

async fn load_destinations(state: &AppState, schedule_id: Uuid) -> Vec<muxshed_common::Destination> {
    let row: Option<(String,)> = sqlx::query_as("SELECT destination_ids FROM schedules WHERE id = ?")
        .bind(schedule_id.to_string()).fetch_optional(&state.db).await.ok().flatten();
    let ids: Vec<String> = row.and_then(|r| serde_json::from_str(&r.0).ok()).unwrap_or_default();
    let mut out = Vec::new();
    for id in ids {
        if let Ok(Some(d)) = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT id, name, kind, enabled FROM destinations WHERE id = ?")
            .bind(&id).fetch_optional(&state.db).await
        {
            if let Ok(kind) = serde_json::from_str::<DestinationKind>(&d.2) {
                out.push(muxshed_common::Destination { id: d.0.parse().unwrap_or_default(), name: d.1, kind, enabled: d.3 != 0 });
            }
        }
    }
    out
}

async fn load_output_config(state: &AppState) -> crate::routes::output::OutputConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
        .fetch_optional(&state.db).await.ok().flatten()
        .and_then(|(j,)| serde_json::from_str(&j).ok()).unwrap_or_default()
}

async fn record_run(state: &AppState, schedule_id: Uuid, status: &str) {
    let _ = sqlx::query("INSERT INTO schedule_runs (id, schedule_id, started_at, status) VALUES (?, ?, ?, ?)")
        .bind(Uuid::new_v4().to_string()).bind(schedule_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339()).bind(status)
        .execute(&state.db).await;
}

async fn mark_run_ended(state: &AppState, schedule_id: Uuid) {
    let _ = sqlx::query(
        "UPDATE schedule_runs SET ended_at = ? WHERE schedule_id = ? AND ended_at IS NULL")
        .bind(chrono::Utc::now().to_rfc3339()).bind(schedule_id.to_string())
        .execute(&state.db).await;
}
```

- [ ] **Step 3: Add the `egress_restart_for` helper**

The playout controller and failover both need "restart egress for a source if live". Add to `crates/api/src/lib.rs` (top-level, after the `pub mod` lines):

```rust
use std::sync::Arc as _StdArc; // ensure Arc in scope for the helper below

/// Restart the egress with a fresh encoder primed for `source`, when live.
/// Shared by the failover supervisor and the playout controller.
pub async fn egress_restart_for(state: &std::sync::Arc<state::AppState>, source: uuid::Uuid) {
    if state.egress.is_running().await {
        let seq = state.sequence_headers.read().await.get(&source).cloned();
        state.egress.restart(seq).await;
    }
}
```

Then in `crates/api/src/failover.rs`, replace the body of the private `restart_egress` with a call to the shared helper (or delete `restart_egress` and call `crate::egress_restart_for(state, source).await` at its two call sites). This is a DRY cleanup, not new behavior.

- [ ] **Step 4: Verify it builds**

Run: `cargo build -p muxshed-api`
Expected: `Finished`. Fix any signature mismatches against `egress.start` (compare to `routes/stream.rs`) and `media_player::start_media_playback`.

- [ ] **Step 5: Commit**

```bash
git add crates/api/src/playout.rs crates/api/src/lib.rs crates/api/src/failover.rs
git commit -m "feat(schedules): playout controller (single VOD + standby + duration end)"
```

---

## Task 6: Scheduler supervisor

**Files:**
- Create: `crates/api/src/scheduler.rs`
- Modify: `crates/api/src/lib.rs` (`pub mod scheduler;`)
- Modify: `crates/api/src/main.rs` (spawn it)

- [ ] **Step 1: Add module + spawn**

In `crates/api/src/lib.rs`: `pub mod scheduler;`

In `crates/api/src/main.rs`, after the failover supervisor spawn:

```rust
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        muxshed_api::scheduler::run_scheduler(scheduler_state).await;
    });
```

- [ ] **Step 2: Implement the scheduler**

Create `crates/api/src/scheduler.rs`:

```rust
// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Scheduler supervisor. Computes each enabled schedule's next run in the system
//! timezone and, at fire time, starts its broadcast — unless already live, in
//! which case it records a skip and notifies.

use crate::schedule_time::{next_run_after, parse_tz};
use crate::state::AppState;
use muxshed_common::{TriggerKind, WsEvent};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

struct Pending { schedule_id: Uuid, at: chrono::DateTime<chrono::Utc> }

pub async fn run_scheduler(state: Arc<AppState>) {
    let mut nudge = state.schedule_nudge.subscribe();
    loop {
        let now = chrono::Utc::now();
        let tz = parse_tz(&load_system_tz(&state).await);
        let pending = compute_pending(&state, tz, now).await;

        // Sleep until the nearest run, a nudge, or a 60s safety re-check.
        let sleep_for = pending.as_ref()
            .map(|p| (p.at - now).to_std().unwrap_or(Duration::from_secs(0)))
            .unwrap_or(Duration::from_secs(60))
            .min(Duration::from_secs(60));

        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            r = nudge.changed() => { if r.is_err() { return; } continue; }
        }

        // Fire anything now due.
        let now = chrono::Utc::now();
        let due = compute_pending(&state, parse_tz(&load_system_tz(&state).await), now).await;
        if let Some(p) = due {
            if p.at <= now + chrono::Duration::seconds(1) {
                fire(&state, p.schedule_id).await;
            }
        }
    }
}

async fn fire(state: &Arc<AppState>, schedule_id: Uuid) {
    let live = state.active_schedule.borrow().is_some()
        || matches!(state.pipeline.state().await, muxshed_common::PipelineState::Live { .. });
    if live {
        tracing::info!("scheduler: skipping {} — already live", schedule_id);
        let _ = sqlx::query("INSERT INTO schedule_runs (id, schedule_id, status) VALUES (?, ?, 'skipped')")
            .bind(Uuid::new_v4().to_string()).bind(schedule_id.to_string()).execute(&state.db).await;
        let _ = state.ws_tx.send(WsEvent::ScheduleSkipped { id: schedule_id, reason: "already live".into() });
        // Disable one-off so it doesn't re-fire in a loop.
        disable_if_once(state, schedule_id).await;
        return;
    }
    tracing::info!("scheduler: firing schedule {}", schedule_id);
    disable_if_once(state, schedule_id).await;
    crate::playout::start_broadcast(state.clone(), schedule_id).await;
}

/// The soonest enabled schedule due at-or-after `now`.
async fn compute_pending(state: &AppState, tz: chrono_tz::Tz, now: chrono::DateTime<chrono::Utc>) -> Option<Pending> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT id, trigger_kind, trigger_at, trigger_cron FROM schedules WHERE enabled = 1",
    ).fetch_all(&state.db).await.ok()?;

    let mut best: Option<Pending> = None;
    for (id, kind, at, cron) in rows {
        let trigger = match kind.as_str() {
            "cron" => TriggerKind::Cron { expr: cron.unwrap_or_default() },
            _ => TriggerKind::Once { at: at.unwrap_or_default() },
        };
        // "after" is now minus 1s so a run exactly at now is still caught.
        if let Some(next) = next_run_after(&trigger, tz, now - chrono::Duration::seconds(1)) {
            let sched_id = id.parse().ok()?;
            if best.as_ref().map(|b| next < b.at).unwrap_or(true) {
                best = Some(Pending { schedule_id: sched_id, at: next });
            }
        }
    }
    best
}

async fn disable_if_once(state: &AppState, schedule_id: Uuid) {
    let _ = sqlx::query("UPDATE schedules SET enabled = 0 WHERE id = ? AND trigger_kind = 'once'")
        .bind(schedule_id.to_string()).execute(&state.db).await;
}

async fn load_system_tz(state: &AppState) -> String {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'system_timezone'")
        .fetch_optional(&state.db).await.ok().flatten().map(|r| r.0).unwrap_or_else(|| "UTC".into())
}
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p muxshed-api`
Expected: `Finished`. (`PipelineState` import path: confirm `muxshed_common::PipelineState` and the `Live { .. }` variant against `crates/common/src/types.rs`.)

- [ ] **Step 4: Commit**

```bash
git add crates/api/src/scheduler.rs crates/api/src/lib.rs crates/api/src/main.rs
git commit -m "feat(schedules): scheduler supervisor with skip+notify"
```

---

## Task 7: Schedules API + timezone endpoint

**Files:**
- Create: `crates/api/src/routes/schedules.rs`
- Modify: `crates/api/src/routes/mod.rs` (declare module + register routes)
- Test: `crates/api/tests/api_tests.rs` (CRUD integration test)

- [ ] **Step 1: Write the failing integration test**

Append to `crates/api/tests/api_tests.rs` (follow the existing helper that builds a test `AppState` + router; reuse the pattern already there for sources/scenes):

```rust
#[tokio::test]
async fn schedule_crud_and_timezone() {
    let app = test_app().await; // existing helper that returns the axum Router
    // set timezone
    let r = req(&app, "PUT", "/api/v1/settings/timezone", Some(json!({"timezone":"Europe/London"}))).await;
    assert_eq!(r.status(), 200);
    // create schedule
    let body = json!({
        "name":"Nightly","enabled":true,
        "trigger":{"kind":"cron","expr":"0 20 * * *"},
        "destination_ids":[],"end_behavior":"stop",
        "items":[{"kind":"vod","ref_id":"00000000-0000-0000-0000-000000000000"}]
    });
    let r = req(&app, "POST", "/api/v1/schedules", Some(body)).await;
    assert_eq!(r.status(), 201);
    // list
    let r = req(&app, "GET", "/api/v1/schedules", None).await;
    assert_eq!(r.status(), 200);
    // reject bad cron
    let bad = json!({"name":"x","trigger":{"kind":"cron","expr":"nope"},"items":[]});
    let r = req(&app, "POST", "/api/v1/schedules", Some(bad)).await;
    assert_eq!(r.status(), 400);
}
```

If `test_app`/`req`/`json` helpers don't exist under those names, use whatever the existing tests use (match their names exactly — read the top of `api_tests.rs`).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p muxshed-api schedule_crud_and_timezone`
Expected: FAIL (routes 404).

- [ ] **Step 3: Implement the routes**

Create `crates/api/src/routes/schedules.rs`:

```rust
// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::schedule_time::{next_run_after, parse_tz};
use crate::state::AppState;
use muxshed_common::{EndBehavior, MuxshedError, Schedule, ScheduleItem, ScheduleItemKind, TriggerKind};

#[derive(Deserialize)]
pub struct CreateItem { pub kind: ScheduleItemKind, pub ref_id: Uuid }

#[derive(Deserialize)]
pub struct UpsertSchedule {
    pub name: String,
    #[serde(default = "yes")] pub enabled: bool,
    pub trigger: TriggerKind,
    #[serde(default)] pub destination_ids: Vec<Uuid>,
    pub standby_asset_id: Option<Uuid>,
    #[serde(default)] pub end_behavior: EndBehavior,
    pub until_at: Option<String>,
    #[serde(default)] pub items: Vec<CreateItem>,
}
fn yes() -> bool { true }

fn validate(u: &UpsertSchedule) -> Result<(), MuxshedError> {
    match &u.trigger {
        TriggerKind::Cron { expr } => {
            // reuse the parser: a bad cron yields None for any far-future window
            if next_run_after(&u.trigger, chrono_tz::UTC, chrono::Utc::now()).is_none()
                && cron::Schedule::from_str(&six(expr)).is_err()
            { return Err(MuxshedError::BadRequest("invalid cron expression".into())); }
        }
        TriggerKind::Once { at } => {
            if chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M")).is_err()
            { return Err(MuxshedError::BadRequest("invalid datetime".into())); }
        }
    }
    Ok(())
}
use std::str::FromStr;
fn six(expr: &str) -> String {
    let p: Vec<&str> = expr.split_whitespace().collect();
    if p.len() == 5 { format!("0 {}", expr.trim()) } else { expr.trim().to_string() }
}

async fn system_tz(state: &AppState) -> chrono_tz::Tz {
    let name = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='system_timezone'")
        .fetch_optional(&state.db).await.ok().flatten().map(|r| r.0).unwrap_or_else(|| "UTC".into());
    parse_tz(&name)
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Schedule>>, ApiError> {
    let ids = sqlx::query_as::<_, (String,)>("SELECT id FROM schedules ORDER BY created_at DESC")
        .fetch_all(&state.db).await?;
    let mut out = Vec::new();
    for (id,) in ids { if let Some(s) = load_schedule(&state, &id).await? { out.push(s); } }
    Ok(Json(out))
}

pub async fn create(
    State(state): State<Arc<AppState>>, Json(body): Json<UpsertSchedule>,
) -> Result<(StatusCode, Json<Schedule>), ApiError> {
    validate(&body).map_err(ApiError::from)?;
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let (kind, at, cron) = trigger_cols(&body.trigger);
    let next = next_run_after(&body.trigger, system_tz(&state).await, chrono::Utc::now()).map(|d| d.to_rfc3339());
    sqlx::query("INSERT INTO schedules \
        (id,name,enabled,trigger_kind,trigger_at,trigger_cron,destination_ids,standby_asset_id,end_behavior,until_at,next_run_at,created_at,updated_at) \
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(id.to_string()).bind(&body.name).bind(body.enabled as i64)
        .bind(kind).bind(at).bind(cron)
        .bind(serde_json::to_string(&body.destination_ids).unwrap_or("[]".into()))
        .bind(body.standby_asset_id.map(|x| x.to_string()))
        .bind(body.end_behavior.as_str()).bind(&body.until_at).bind(next)
        .bind(&now).bind(&now)
        .execute(&state.db).await?;
    replace_items(&state, &id, &body.items).await?;
    let _ = state.schedule_nudge.send(rand_bump(&state));
    let s = load_schedule(&state, &id.to_string()).await?.ok_or(MuxshedError::Internal("load".into()))?;
    Ok((StatusCode::CREATED, Json(s)))
}

pub async fn update(
    State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<UpsertSchedule>,
) -> Result<Json<Schedule>, ApiError> {
    validate(&body).map_err(ApiError::from)?;
    let (kind, at, cron) = trigger_cols(&body.trigger);
    let next = next_run_after(&body.trigger, system_tz(&state).await, chrono::Utc::now()).map(|d| d.to_rfc3339());
    let res = sqlx::query("UPDATE schedules SET name=?,enabled=?,trigger_kind=?,trigger_at=?,trigger_cron=?,\
        destination_ids=?,standby_asset_id=?,end_behavior=?,until_at=?,next_run_at=?,updated_at=? WHERE id=?")
        .bind(&body.name).bind(body.enabled as i64).bind(kind).bind(at).bind(cron)
        .bind(serde_json::to_string(&body.destination_ids).unwrap_or("[]".into()))
        .bind(body.standby_asset_id.map(|x| x.to_string())).bind(body.end_behavior.as_str())
        .bind(&body.until_at).bind(next).bind(chrono::Utc::now().to_rfc3339()).bind(&id)
        .execute(&state.db).await?;
    if res.rows_affected() == 0 { return Err(MuxshedError::NotFound(format!("schedule {id}")).into()); }
    replace_items(&state, &id.parse().unwrap_or_default(), &body.items).await?;
    let _ = state.schedule_nudge.send(rand_bump(&state));
    Ok(Json(load_schedule(&state, &id).await?.ok_or(MuxshedError::NotFound(id))?))
}

pub async fn delete(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let r = sqlx::query("DELETE FROM schedules WHERE id=?").bind(&id).execute(&state.db).await?;
    if r.rows_affected() == 0 { return Err(MuxshedError::NotFound(format!("schedule {id}")).into()); }
    let _ = state.schedule_nudge.send(rand_bump(&state));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn run_now(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let sid: Uuid = id.parse().map_err(|_| MuxshedError::BadRequest("bad id".into()))?;
    let live = state.active_schedule.borrow().is_some();
    if live { return Err(MuxshedError::BadRequest("already live".into()).into()); }
    crate::playout::start_broadcast(state.clone(), sid).await;
    Ok(StatusCode::OK)
}

pub async fn stop(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    crate::playout::stop_broadcast(&state).await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct TzBody { pub timezone: String }

pub async fn get_timezone(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    let tz = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='system_timezone'")
        .fetch_optional(&state.db).await?.map(|r| r.0).unwrap_or_else(|| "UTC".into());
    Ok(Json(serde_json::json!({ "timezone": tz })))
}

pub async fn set_timezone(State(state): State<Arc<AppState>>, Json(b): Json<TzBody>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(MuxshedError::BadRequest("unknown timezone".into()).into());
    }
    sqlx::query("INSERT OR REPLACE INTO settings (key,value) VALUES ('system_timezone', ?)")
        .bind(&b.timezone).execute(&state.db).await?;
    // recompute all next_run_at + wake the scheduler
    recompute_all(&state).await?;
    let _ = state.schedule_nudge.send(rand_bump(&state));
    Ok(Json(serde_json::json!({ "timezone": b.timezone })))
}

// ---- helpers ----

fn trigger_cols(t: &TriggerKind) -> (&'static str, Option<String>, Option<String>) {
    match t {
        TriggerKind::Once { at } => ("once", Some(at.clone()), None),
        TriggerKind::Cron { expr } => ("cron", None, Some(expr.clone())),
    }
}

fn rand_bump(_state: &AppState) -> u64 {
    // monotonic-ish bump; value only needs to change to wake the watch receiver
    chrono::Utc::now().timestamp_millis() as u64
}

async fn replace_items(state: &AppState, schedule_id: &Uuid, items: &[CreateItem]) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM schedule_items WHERE schedule_id=?").bind(schedule_id.to_string()).execute(&state.db).await?;
    for (i, it) in items.iter().enumerate() {
        sqlx::query("INSERT INTO schedule_items (id,schedule_id,position,item_kind,ref_id) VALUES (?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(schedule_id.to_string())
            .bind(i as i64).bind(it.kind.as_str()).bind(it.ref_id.to_string())
            .execute(&state.db).await?;
    }
    Ok(())
}

async fn recompute_all(state: &AppState) -> Result<(), ApiError> {
    let tz = system_tz(state).await;
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT id,trigger_kind,trigger_at,trigger_cron FROM schedules WHERE enabled=1").fetch_all(&state.db).await?;
    for (id, kind, at, cron) in rows {
        let t = if kind == "cron" { TriggerKind::Cron { expr: cron.unwrap_or_default() } }
                else { TriggerKind::Once { at: at.unwrap_or_default() } };
        let next = next_run_after(&t, tz, chrono::Utc::now()).map(|d| d.to_rfc3339());
        sqlx::query("UPDATE schedules SET next_run_at=? WHERE id=?").bind(next).bind(id).execute(&state.db).await?;
    }
    Ok(())
}

async fn load_schedule(state: &AppState, id: &str) -> Result<Option<Schedule>, ApiError> {
    let row = sqlx::query_as::<_, (String,String,i64,String,Option<String>,Option<String>,String,Option<String>,String,Option<String>,Option<String>)>(
        "SELECT id,name,enabled,trigger_kind,trigger_at,trigger_cron,destination_ids,standby_asset_id,end_behavior,until_at,next_run_at FROM schedules WHERE id=?")
        .bind(id).fetch_optional(&state.db).await?;
    let Some(r) = row else { return Ok(None) };
    let items = sqlx::query_as::<_, (String,i64,String,String)>(
        "SELECT id,position,item_kind,ref_id FROM schedule_items WHERE schedule_id=? ORDER BY position")
        .bind(id).fetch_all(&state.db).await?
        .into_iter().map(|(iid,pos,k,ref_id)| ScheduleItem {
            id: iid.parse().unwrap_or_default(), position: pos as u32,
            kind: ScheduleItemKind::from_db(&k), ref_id: ref_id.parse().unwrap_or_default(),
        }).collect();
    let trigger = if r.3 == "cron" { TriggerKind::Cron { expr: r.5.unwrap_or_default() } }
                  else { TriggerKind::Once { at: r.4.unwrap_or_default() } };
    Ok(Some(Schedule {
        id: r.0.parse().unwrap_or_default(), name: r.1, enabled: r.2 != 0, trigger,
        destination_ids: serde_json::from_str(&r.6).unwrap_or_default(),
        standby_asset_id: r.7.and_then(|s| s.parse().ok()),
        end_behavior: EndBehavior::from_db(&r.8), until_at: r.9,
        items, next_run_at: r.10,
    }))
}
```

- [ ] **Step 4: Register routes**

In `crates/api/src/routes/mod.rs`: add `mod schedules;` near the other module decls, and inside the authenticated router builder add:

```rust
        .route("/schedules", get(schedules::list).post(schedules::create))
        .route("/schedules/{id}", put(schedules::update).delete(schedules::delete))
        .route("/schedules/{id}/run", post(schedules::run_now))
        .route("/schedules/stop", post(schedules::stop))
        .route("/settings/timezone", get(schedules::get_timezone).put(schedules::set_timezone))
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p muxshed-api schedule_crud_and_timezone`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/api/src/routes/schedules.rs crates/api/src/routes/mod.rs crates/api/tests/api_tests.rs
git commit -m "feat(schedules): schedules CRUD + timezone API"
```

---

## Task 8: Frontend — types, api, store, ws

**Files:**
- Modify: `web/src/lib/types.ts`, `web/src/lib/api.ts`, `web/src/lib/stores/pipeline.ts`, `web/src/lib/ws.ts`

- [ ] **Step 1: Add TS types**

In `web/src/lib/types.ts`:

```ts
export type EndBehavior = 'stop' | 'loop' | 'standby';
export type ScheduleItemKind = 'vod' | 'scene' | 'source';
export type Trigger = { kind: 'once'; at: string } | { kind: 'cron'; expr: string };

export interface ScheduleItem { id: string; position: number; kind: ScheduleItemKind; ref_id: string; }
export interface Schedule {
	id: string; name: string; enabled: boolean; trigger: Trigger;
	destination_ids: string[]; standby_asset_id: string | null;
	end_behavior: EndBehavior; until_at: string | null;
	items: ScheduleItem[]; next_run_at: string | null;
}
```

And extend the `WsEvent` union:

```ts
	| { type: 'schedule_started'; payload: { id: string } }
	| { type: 'schedule_ended'; payload: { id: string } }
	| { type: 'schedule_skipped'; payload: { id: string; reason: string } }
```

- [ ] **Step 2: Add api methods**

In `web/src/lib/api.ts` (import `Schedule` in the type import line), add:

```ts
	listSchedules: () => request<Schedule[]>('/schedules'),
	createSchedule: (s: Partial<Schedule> & { name: string; trigger: Schedule['trigger']; items: { kind: string; ref_id: string }[] }) =>
		request<Schedule>('/schedules', { method: 'POST', body: JSON.stringify(s) }),
	updateSchedule: (id: string, s: object) => request<Schedule>(`/schedules/${id}`, { method: 'PUT', body: JSON.stringify(s) }),
	deleteSchedule: (id: string) => request<void>(`/schedules/${id}`, { method: 'DELETE' }),
	runSchedule: (id: string) => request<void>(`/schedules/${id}/run`, { method: 'POST' }),
	stopSchedule: () => request<void>('/schedules/stop', { method: 'POST' }),
	getTimezone: () => request<{ timezone: string }>('/settings/timezone'),
	setTimezone: (timezone: string) => request<{ timezone: string }>('/settings/timezone', { method: 'PUT', body: JSON.stringify({ timezone }) }),
```

- [ ] **Step 3: Add store + ws handling**

In `web/src/lib/stores/pipeline.ts`:

```ts
export const activeSchedule = writable<{ id: string | null }>({ id: null });
```

In `web/src/lib/ws.ts` (import `activeSchedule`), add cases in the switch:

```ts
		case 'schedule_started':
			activeSchedule.set({ id: event.payload.id });
			break;
		case 'schedule_ended':
			activeSchedule.set({ id: null });
			break;
		case 'schedule_skipped':
			notify.info?.(`Schedule skipped: ${event.payload.reason}`);
			break;
```

(If `notify` isn't already imported in `ws.ts`, either import it or drop the skipped-toast line — the store update is the essential part.)

- [ ] **Step 4: Type-check**

Run: `cd web && npm run check`
Expected: `0 ERRORS`.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/types.ts web/src/lib/api.ts web/src/lib/stores/pipeline.ts web/src/lib/ws.ts
git commit -m "feat(schedules): frontend types, api client, store, ws"
```

---

## Task 9: Frontend — Schedules page, nav, timezone setting

**Files:**
- Create: `web/src/routes/(app)/schedules/+page.svelte`
- Modify: `web/src/routes/(app)/+layout.svelte` (nav item)
- Modify: `web/src/routes/(app)/settings/+page.svelte` (timezone picker)

- [ ] **Step 1: Add the nav item**

In `web/src/routes/(app)/+layout.svelte`, in the `nav` array (after `{ href: '/scenes', label: 'Scenes' }`):

```ts
		{ href: '/schedules', label: 'Schedules' },
```

- [ ] **Step 2: Create the Schedules page**

Create `web/src/routes/(app)/schedules/+page.svelte`:

```svelte
<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import { activeSchedule } from '$lib/stores/pipeline';
	import type { Schedule, Asset, Destination } from '$lib/types';

	let schedules = $state<Schedule[]>([]);
	let assets = $state<Asset[]>([]);
	let destinations = $state<Destination[]>([]);
	let tz = $state('UTC');

	// editor state
	let editing = $state<Schedule | null>(null);
	let name = $state('');
	let triggerKind = $state<'once' | 'cron'>('cron');
	let cron = $state('0 20 * * *');
	let onceAt = $state('');
	let vodId = $state('');
	let standbyId = $state('');
	let endBehavior = $state<'stop' | 'loop' | 'standby'>('stop');
	let destIds = $state<string[]>([]);

	onMount(refresh);
	async function refresh() {
		[schedules, assets, destinations, tz] = await Promise.all([
			api.listSchedules(),
			api.listAssets().then((a) => a.filter((x) => x.asset_type === 'video')),
			api.listDestinations(),
			api.getTimezone().then((t) => t.timezone),
		]);
	}

	function newSchedule() {
		editing = { id: '' } as Schedule;
		name = ''; triggerKind = 'cron'; cron = '0 20 * * *'; onceAt = '';
		vodId = assets[0]?.id ?? ''; standbyId = ''; endBehavior = 'stop'; destIds = [];
	}

	async function save() {
		if (!vodId) { notify.error('Pick a VOD'); return; }
		const trigger = triggerKind === 'cron' ? { kind: 'cron', expr: cron } : { kind: 'once', at: onceAt };
		const payload = {
			name, enabled: true, trigger,
			destination_ids: destIds, standby_asset_id: standbyId || null,
			end_behavior: endBehavior, until_at: null,
			items: [{ kind: 'vod', ref_id: vodId }],
		};
		try {
			if (editing?.id) await api.updateSchedule(editing.id, payload);
			else await api.createSchedule(payload as any);
			editing = null;
			await refresh();
		} catch (e) { notify.error(e); }
	}

	async function toggle(s: Schedule) {
		await api.updateSchedule(s.id, {
			name: s.name, enabled: !s.enabled, trigger: s.trigger,
			destination_ids: s.destination_ids, standby_asset_id: s.standby_asset_id,
			end_behavior: s.end_behavior, until_at: s.until_at,
			items: s.items.map((i) => ({ kind: i.kind, ref_id: i.ref_id })),
		});
		await refresh();
	}
	async function del(s: Schedule) { await api.deleteSchedule(s.id); await refresh(); }
	async function runNow(s: Schedule) { try { await api.runSchedule(s.id); } catch (e) { notify.error(e); } }

	function nextRun(s: Schedule) {
		return s.next_run_at ? new Date(s.next_run_at).toLocaleString() : '—';
	}
</script>

<div class="mx-auto max-w-4xl">
	<div class="row mb-4 items-center justify-between">
		<div>
			<h1 class="text-lg text-amber-bright tracking-widest">SCHEDULES</h1>
			<p class="text-xs text-amber-dim">Timezone: {tz} · set it in Settings</p>
		</div>
		<button class="btn btn--go" onclick={newSchedule}>+ New schedule</button>
	</div>

	{#if $activeSchedule.id}
		<div class="panel mb-3"><div class="panel__body text-live">● A scheduled broadcast is on air.
			<button class="btn btn--ghost ml-2" onclick={() => api.stopSchedule()}>Stop</button></div></div>
	{/if}

	{#each schedules as s (s.id)}
		<div class="panel mb-2"><div class="panel__body row items-center justify-between">
			<div>
				<span class="text-amber">{s.name}</span>
				<span class="ml-2 text-[11px] text-amber-muted">
					{s.trigger.kind === 'cron' ? `cron ${s.trigger.expr}` : `once ${s.trigger.at}`} · next {nextRun(s)} · {s.end_behavior}
				</span>
			</div>
			<div class="flex gap-1">
				<button class="btn btn--ghost" onclick={() => runNow(s)}>Air now</button>
				<button class="btn {s.enabled ? 'btn--go' : ''}" onclick={() => toggle(s)}>{s.enabled ? 'On' : 'Off'}</button>
				<button class="btn btn--danger" onclick={() => del(s)}>✕</button>
			</div>
		</div></div>
	{/each}

	{#if editing}
		<div class="panel mt-4"><div class="panel__body">
			<label class="field-label" for="s-name">Name</label>
			<input id="s-name" bind:value={name} class="input mb-3" />

			<label class="field-label" for="s-vod">VOD</label>
			<select id="s-vod" bind:value={vodId} class="select mb-3 w-full">
				{#each assets as a}<option value={a.id}>{a.name}</option>{/each}
			</select>

			<label class="field-label" for="s-trigger">Trigger</label>
			<select id="s-trigger" bind:value={triggerKind} class="select mb-2 w-full">
				<option value="cron">Recurring (cron)</option>
				<option value="once">One-off (date/time)</option>
			</select>
			{#if triggerKind === 'cron'}
				<input bind:value={cron} placeholder="0 20 * * *" class="input mb-3" />
			{:else}
				<input type="datetime-local" bind:value={onceAt} class="input mb-3" />
			{/if}

			<label class="field-label" for="s-end">When it ends</label>
			<select id="s-end" bind:value={endBehavior} class="select mb-3 w-full">
				<option value="stop">Stop (go offline)</option>
				<option value="loop">Loop</option>
				<option value="standby">Hold on standby</option>
			</select>

			<label class="field-label" for="s-standby">Standby card (optional)</label>
			<select id="s-standby" bind:value={standbyId} class="select mb-3 w-full">
				<option value="">— none —</option>
				{#each assets as a}<option value={a.id}>{a.name}</option>{/each}
			</select>

			<label class="field-label">Destinations</label>
			<div class="mb-3 flex flex-col gap-1">
				{#each destinations as d}
					<label class="flex items-center gap-2 text-xs">
						<input type="checkbox" value={d.id} checked={destIds.includes(d.id)}
							onchange={(e) => destIds = e.currentTarget.checked ? [...destIds, d.id] : destIds.filter((x) => x !== d.id)} />
						{d.name}
					</label>
				{/each}
			</div>

			<div class="flex gap-2">
				<button class="btn btn--go" onclick={save}>Save</button>
				<button class="btn btn--ghost" onclick={() => (editing = null)}>Cancel</button>
			</div>
		</div></div>
	{/if}
</div>
```

If `api.listAssets` filters on a different field than `asset_type === 'video'`, match the real `Asset` shape in `types.ts`.

- [ ] **Step 3: Add the timezone picker to Settings**

In `web/src/routes/(app)/settings/+page.svelte` script, add state + load + save (mirror the failover section pattern):

```ts
	let systemTz = $state('UTC');
	// in onMount: systemTz = (await api.getTimezone()).timezone;
	async function saveTz() { try { await api.setTimezone(systemTz); notify.success('Timezone saved'); } catch (e) { notify.error(e); } }
```

And a section in the template (before Display):

```svelte
	<section class="panel">
		<header class="panel__head">▮ System Timezone</header>
		<div class="panel__body">
			<p class="mb-2 text-xs text-amber-dim">All schedules are evaluated in this timezone.</p>
			<input bind:value={systemTz} placeholder="Europe/London" class="input mb-2" />
			<button class="btn btn--go" onclick={saveTz}>Save timezone</button>
		</div>
	</section>
```

- [ ] **Step 4: Type-check + build**

Run: `cd web && npm run check && npm run build`
Expected: `0 ERRORS`, build succeeds.

- [ ] **Step 5: Commit**

```bash
git add "web/src/routes/(app)/schedules/+page.svelte" "web/src/routes/(app)/+layout.svelte" "web/src/routes/(app)/settings/+page.svelte"
git commit -m "feat(schedules): Schedules page, nav item, timezone setting"
```

---

## Task 10: Full-suite verification + real end-to-end

**Files:** none (verification), plus a scratch e2e script.

- [ ] **Step 1: Workspace tests + clippy + svelte-check**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm run check
```
Expected: all tests pass; clippy clean; 0 svelte errors. Fix anything that fails before proceeding.

- [ ] **Step 2: Real ffmpeg end-to-end (same rig as the failover tests)**

Create a scratch script that: starts the API on isolated ports with a temp DB; uploads a short (~5s) test VOD to the library (`ffmpeg -f lavfi -i testsrc2=d=5 ... out.mp4`, then `POST /assets/upload`); creates an RTMP destination pointing at the persistent `node-media-server` sink (`scratchpad/nms/sink.js`); `PUT /settings/timezone` to a known zone; `POST /schedules` with a one-off `trigger.at` ~5 seconds in the future (in that zone) whose single VOD item is the uploaded asset and destination the sink; then waits. Assert:
  - the scheduler fires at the set time (log line `scheduler: firing schedule`),
  - the sink records a session (the VOD reaches the destination) — extract a frame and confirm it's the testsrc,
  - the broadcast auto-stops when the VOD ends (`ScheduleEnded` / egress stop), and `active_schedule` returns to null via `GET /schedules` state,
  - creating a second schedule that fires while the first is live is **skipped** (a `schedule_runs` row with `status='skipped'` + a `schedule_skipped` WS event).

Run the script; iterate on the code until all four assertions pass, capturing frames as proof (mirror `scratchpad/run_failover_nms.sh`).

- [ ] **Step 3: Browser smoke test**

Start a backend on 8080 + the web preview; create a schedule through the Schedules page UI; confirm it persists (`GET /schedules`) and the timezone setting round-trips through the Settings page (as done for the fit-mode and failover UIs).

- [ ] **Step 4: Commit any fixes, then open the PR**

```bash
git add -A && git commit -m "test(schedules): phase 1 end-to-end verification"
```
Open a PR titled "VOD scheduling — Phase 1 (scheduled premiere)" summarizing the feature + the e2e evidence, and merge per the project convention.

---

## Self-review notes (author)

- **Spec coverage:** schedules/items/runs tables (T1) · types (T2) · DST cron+once in system tz (T3, T7) · scheduler + skip-notify (T6) · playout + standby + auto-stop reusing media_player/egress/failover-fallback (T5) · per-broadcast destinations (T5/T7) · Schedules nav + UI + timezone setting (T8/T9) · real ffmpeg e2e (T10). Phases 2–3 remain (playlists/loop advance; scene/source items; run-history UI; richer cron builder) — out of scope for this plan by design.
- **Type consistency:** `EndBehavior`/`ScheduleItemKind`/`TriggerKind` names and `as_str`/`from_db` match across common types, playout, scheduler, and routes. `active_schedule`/`schedule_nudge` channel names match across state/main/scheduler/playout/routes.
- **Known follow-ups for the executor:** confirm the exact `test_app`/request helper names in `api_tests.rs`; confirm `Asset` field names in `types.ts`; confirm `egress.start`/`start_media_playback` signatures against current source before pasting; duration-based end tolerates ~½s drift (acceptable Phase 1, replaced by real exit-detection if Phase 2 needs frame-accuracy).
