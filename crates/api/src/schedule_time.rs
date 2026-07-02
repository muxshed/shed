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
        let after = utc("2026-03-28T19:30:00Z"); // 19:30 GMT, before spring-forward (last Sun of March = Mar 29)
        let next = next_run_after(&TriggerKind::Cron { expr: "0 20 * * *".into() }, tz, after).unwrap();
        assert_eq!(next, utc("2026-03-28T20:00:00Z")); // 20:00 London still GMT == 20:00Z

        let after2 = utc("2026-03-30T10:00:00Z");
        let next2 = next_run_after(&TriggerKind::Cron { expr: "0 20 * * *".into() }, tz, after2).unwrap();
        assert_eq!(next2, utc("2026-03-30T19:00:00Z")); // 20:00 London now BST == 19:00Z
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
