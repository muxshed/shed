// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Startup housekeeping run once before the router is built.

use sqlx::SqlitePool;

/// Remove orphan guest sources left over from a previous run and reset guest
/// records. Guest sources are ephemeral (their peers live in memory) and are
/// linked to a `guests` record; persistent WHIP ingest sources use the same
/// `web_rtc` kind but are NOT guest-linked, so they must survive a restart and
/// are left untouched here.
pub async fn cleanup_orphan_guest_sources(db: &SqlitePool) {
    let _ = sqlx::query(
        "DELETE FROM sources WHERE id IN (SELECT source_id FROM guests WHERE source_id IS NOT NULL)",
    )
    .execute(db)
    .await;
    let _ = sqlx::query(
        "UPDATE guests SET status = 'invited', source_id = NULL WHERE source_id IS NOT NULL",
    )
    .execute(db)
    .await;
}
