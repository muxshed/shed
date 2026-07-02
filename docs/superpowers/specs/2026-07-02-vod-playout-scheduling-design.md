# VOD Playout & Scheduling — Design

**Date:** 2026-07-02
**Status:** Approved for planning
**Repo:** `system/`

## 1. Summary

Let operators broadcast **uploaded VODs as live streams** and **schedule** those broadcasts.
A scheduled broadcast auto-goes-live at a set time (one-off or recurring via cron), plays its
content out to chosen destinations, and stops or loops when the content ends.

The three use cases the owner asked for are presets of one underlying system:

| Use case | = |
|----------|---|
| Scheduled premiere | one VOD + scheduled start + `end_behavior: stop` |
| 24/7 VOD channel | a playlist + `end_behavior: loop` |
| General scheduler | any content (VOD / scene / source) + cron recurrence |

Most of the *video plumbing already exists* — `media_player` plays video files to the program/egress
path today. The genuinely new work is a **scheduler** and a **playout controller**, plus a
**Schedules** UI. Transitions between content reuse the egress-restart splice handling built and
verified for program failover.

## 2. Confirmed decisions

- **Destinations:** per-broadcast — each schedule selects its own destinations.
- **Navigation:** a dedicated top-level **Schedules** menu item (`/schedules`).
- **Conflict policy:** if a schedule fires while already live (manual or another schedule),
  **skip + notify** — never interrupt what's on air. Record the skip.
- **Standby:** a configurable **standby card** (image/video) fills any gap while live
  (before the first content frame, between playlist items, after end when held) so the stream is
  never black. Reuses the failover-fallback mechanism.
- **Recurrence:** **full cron** expressions, plus one-off date/time.
- **Timezone:** a single **owner-configured system timezone** (IANA zone). Every schedule is
  evaluated against it; DST-accurate. Not per-schedule.

## 3. Architecture

Everything airs through the existing **source → program → egress** path. A scheduled broadcast is:
pick content → go live to its destinations → play out → stop/loop. Two new components drive it.

```
                 ┌─────────────────────────────────────────────┐
 schedules (DB)  │  Scheduler supervisor (background task)      │
 cron / one-off  │  - computes next_run_at per enabled schedule │
                 │  - at fire time: live? -> skip+notify        │
                 │                  else  -> start playout      │
                 └───────────────────────┬─────────────────────┘
                                         │ start(schedule)
                 ┌───────────────────────▼─────────────────────┐
                 │  Playout controller (per running broadcast)  │
                 │  - VOD playlist -> ffmpeg concat source      │
                 │  - mixed content -> switch program_intent    │
                 │  - gaps/pre/post -> standby card             │
                 │  - end_behavior: stop | loop | standby       │
                 └───────┬───────────────────────┬─────────────┘
                         │ program_intent         │ egress.start(destinations)
                 ┌───────▼────────┐       ┌───────▼──────────┐
                 │ program router │──────▶│ egress (+restart)│──▶ destinations
                 └────────────────┘       └──────────────────┘
```

**Why not a single ffmpeg concat process for the whole broadcast?** Considered and rejected as the
primary architecture: seamless but rigid — requires uniform codecs, can't mix a live source/scene
into a playlist, and makes standby insertion awkward. Concat is used *only* as an internal
optimization for pure-VOD playlists (see §5.2).

## 4. Data model (SQLite, new migration)

```sql
CREATE TABLE schedules (
  id              TEXT PRIMARY KEY NOT NULL,
  name            TEXT NOT NULL,
  enabled         INTEGER NOT NULL DEFAULT 1,
  trigger_kind    TEXT NOT NULL,              -- 'once' | 'cron'
  trigger_at      TEXT,                       -- one-off datetime (system tz), for 'once'
  trigger_cron    TEXT,                       -- cron expr, for 'cron'
  destination_ids TEXT NOT NULL DEFAULT '[]', -- JSON array of destination ids
  standby_asset_id TEXT,                       -- library asset for the standby card (nullable)
  end_behavior    TEXT NOT NULL DEFAULT 'stop', -- 'stop' | 'loop' | 'standby'
  until_at        TEXT,                        -- optional hard end time (system tz)
  next_run_at     TEXT,                        -- computed; UTC instant
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE schedule_items (
  id          TEXT PRIMARY KEY NOT NULL,
  schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,               -- ordering
  item_kind   TEXT NOT NULL,                  -- 'vod' | 'scene' | 'source'  (Phase 1: 'vod')
  ref_id      TEXT NOT NULL                   -- asset_id / scene_id / source_id
);

CREATE TABLE schedule_runs (
  id          TEXT PRIMARY KEY NOT NULL,
  schedule_id TEXT NOT NULL,
  started_at  TEXT,
  ended_at    TEXT,
  status      TEXT NOT NULL                   -- 'ran' | 'skipped' | 'error' | 'missed'
);
```

System timezone lives in the existing `settings` table (`key = 'system_timezone'`, e.g.
`Europe/London`), consistent with `output_config` / `failover_config`.

## 5. Components

### 5.1 Scheduler supervisor (`crates/api/src/scheduler.rs`)

A background task spawned in `main.rs`, same pattern as the failover supervisor. Loop:

1. Load enabled schedules; for each, compute `next_run_at` in the **system timezone** using a cron
   crate over the IANA tz database (chrono-tz), so DST is handled correctly.
2. Sleep until the nearest `next_run_at` (or a config-change nudge, like the failover supervisor).
3. At fire time:
   - **Already live?** (pipeline live or another schedule's playout active) → write a `schedule_runs`
     row `status = skipped`, emit a `schedule_skipped` WS event, move on.
   - **Otherwise** → start the playout controller for this schedule; write `status = ran`.
4. One-off (`once`) schedules whose time has passed while the server was down are marked
   `missed` — never fired retroactively. Recurring schedules simply compute their next future
   occurrence.

Changing a schedule or the system timezone nudges the supervisor to recompute (re-send on a watch
channel), mirroring the failover config-nudge.

### 5.2 Playout controller (`crates/api/src/playout.rs`)

Given a running schedule, airs its content and manages `end_behavior`:

- **Pure-VOD playlist:** write an ffmpeg **concat** playlist of the items and run one media-player
  process (seamless between items; `-stream_loop -1` when `end_behavior = loop`). This becomes a
  single source routed to `program_intent`. Reuses `media_player` patterns.
- **Mixed content (Phase 3):** a state machine that switches `program_intent` between items — a VOD
  source, a scene (compositor), or a live source — restarting the egress on each splice (the
  verified failover mechanism).
- **Standby:** whenever the broadcast is live but has no content frame — before the first frame,
  during a gap, or after the end when `end_behavior = standby` — route program to the standby card
  (a media_file source from `standby_asset_id`, produced exactly like the failover fallback).
- **End:** `stop` → stop egress, go offline; `loop` → restart the playlist; `standby` → hold on the
  standby card. `until_at`, if set, forces stop at that time regardless.

Going live sets the schedule's `destination_ids` and calls the existing egress start; ending calls
egress stop.

### 5.3 Schedules API (`crates/api/src/routes/schedules.rs`)

```
GET    /api/v1/schedules            list (with next_run_at, on-air state)
POST   /api/v1/schedules            create (+ items)
GET    /api/v1/schedules/:id
PUT    /api/v1/schedules/:id        update (+ items); cron validated here
DELETE /api/v1/schedules/:id
POST   /api/v1/schedules/:id/enable
POST   /api/v1/schedules/:id/disable
POST   /api/v1/schedules/:id/run    manual "air now" (respects conflict policy)
GET    /api/v1/schedules/runs       recent run history
GET    /api/v1/settings/timezone    get/set the system timezone (PUT)
```

Cron and timezone are validated on write (reject an unparseable expr or unknown IANA zone).

### 5.4 WS events

`schedule_started { id }`, `schedule_ended { id }`, `schedule_skipped { id, reason }`,
`schedule_item_changed { id, position }` — so the Studio and Schedules UI reflect live state.

## 6. Frontend

New nav item **Schedules** (`/schedules`):

- **List:** name, next-run (in system tz with zone label), on-air badge, enable toggle, quick edit.
- **Editor:** name; **content** (add VOD items from the Library; reorder); **destinations**
  (multi-select of existing destinations); **trigger** (one-off datetime **or** cron — a friendly
  builder with a raw-cron escape hatch); **standby card** (pick a library asset); **end behavior**
  (stop / loop / standby); optional hard end time.
- **Timezone:** shown throughout; set in **Settings** (a system-timezone picker).
- **Studio:** a banner when a scheduled broadcast is driving the program (like the failover banner).

## 7. Reuse (risk reduction)

- `media_player` → VOD playout (+ ffmpeg concat for playlists).
- program router + `program_intent` → routing content to program.
- **egress + egress-restart** (failover work) → clean splices between items / content types.
- failover-fallback pattern → the standby card.
- background-supervisor pattern (failover) → the scheduler loop and its config-nudge.
- SQLite + `settings` → persistence.

## 8. Phasing (build order)

- **Phase 1 — Scheduled VOD premiere (foundation).** `schedules`/`schedule_items`/`schedule_runs`
  tables; single-VOD content; cron + one-off scheduler in the system timezone; auto-go-live →
  auto-stop; standby card; skip-on-conflict; Schedules UI (list + create/edit for one VOD);
  system-timezone setting. Delivers the headline use case end to end.
- **Phase 2 — Playlists & 24/7 channel.** Multiple VOD items via concat; `loop`; playout controller
  advances items; playlist editor.
- **Phase 3 — Any content & polish.** Scene / live-source items with source-switching; run-history
  UI; richer cron builder; optional pre-roll lead time.

The Phase 1 data model already carries `items[]`, `end_behavior`, and cron, so Phases 2–3 are
additive.

## 9. Error handling & edge cases

- **Missing/corrupt VOD file** → skip that item, go to standby, notify; `error` run status.
- **All destinations fail** → egress already handles per-destination; the broadcast still produces
  program + the watch page (consistent with current go-live behavior).
- **Server down at fire time** → one-offs marked `missed`; recurring compute next future occurrence.
- **Cron/timezone parse errors** → rejected at save time.
- **Overlapping schedules** → skip+notify (only one broadcast on air at a time).
- **System timezone change** → recompute all `next_run_at`.
- **Manual go-live already active** → scheduler skips (it's "live").

## 10. Testing

- **Unit:** cron next-occurrence across DST boundaries in the system tz; playout state machine
  (item advance, loop, standby transitions); conflict-skip decision.
- **Integration:** schedules CRUD (create/update/validate cron & tz/delete), run-history.
- **End-to-end (ffmpeg, same rigor as failover):** create a schedule with a short VOD firing in a
  few seconds → assert it auto-goes-live and the VOD reaches a real destination (persistent RTMP
  sink, capture frames) → assert it auto-stops at end and standby shows in a gap. Verify a second
  schedule firing while live is skipped.
- Frontend: `svelte-check` clean; drive the Schedules editor in a browser (create a schedule →
  confirm it persists) as done for the fit-mode and failover UIs.

## 11. Out of scope (for now)

- Frame-accurate broadcast automation / EPG.
- Catch-up (starting a VOD at an offset if the schedule started before "now").
- Ad insertion, SCTE markers, mid-roll.
- Per-schedule timezones (explicitly rejected — one system timezone).
