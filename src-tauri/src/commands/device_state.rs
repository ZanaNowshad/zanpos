//! Whether a terminal is actually there, worked out rather than stored.
//!
//! `devices.status` was a column. It was written when the device row was
//! created and then, in practice, never again — so the Command Center's
//! online/offline tile reported what somebody had once typed. POS-7757Z sat
//! there reading "online" with `last_seen_at` and IP both empty since
//! registration, and POS2 did the same. A tile that says online when nothing has
//! ever contacted the hub is worse than no tile: it sends an operator looking
//! for the problem somewhere else.
//!
//! So state is not stored. It is derived, every time it is read, from evidence
//! the terminal had to produce: a heartbeat with a timestamp, the address the
//! hub observed it coming from, and a sequence number. Nothing here can be set
//! by hand, and nothing goes stale, because there is nothing to go stale.
//!
//! The distinction that matters operationally is between a terminal that has
//! *never* been bound and one that was working and stopped. Both were "offline"
//! before; they need completely different fixes.

use serde::{Deserialize, Serialize};

/// Beyond this, a terminal has missed enough beats that it is not serving.
pub const ONLINE_WITHIN_SECS: i64 = 120;

/// Beyond *this*, it is not coming back on its own. Between the two is the
/// interesting band: a till mid-reboot, or on a flaky link, looks the same as
/// one that just died, and calling both "offline" hides which.
pub const STALE_WITHIN_SECS: i64 = 15 * 60;

/// Clock skew worth warning about. Matches the join-time warning threshold: a
/// slow device's rows land behind the sync watermark and are never offered
/// again, so skew is a data-loss vector, not a cosmetic figure.
pub const CLOCK_SKEW_WARN_SECS: i64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// No installation is bound to this device record. The row exists because
    /// somebody registered it; nothing has ever claimed it.
    Unpaired,
    /// Bound, but has never sent a heartbeat. Almost always a configuration
    /// problem — wrong hub endpoint, or the binding never completed.
    NeverSeen,
    Online,
    /// Beat recently but not just now. Rebooting, or on a poor link.
    Stale,
    Offline,
}

impl DeviceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unpaired => "unpaired",
            Self::NeverSeen => "never_seen",
            Self::Online => "online",
            Self::Stale => "stale",
            Self::Offline => "offline",
        }
    }

    /// What an operator should do about it, which is the only reason the
    /// distinction between these states is worth drawing.
    pub fn advice(self) -> &'static str {
        match self {
            Self::Unpaired => {
                "No installation is bound to this device record. Bind the terminal in \
                 Settings → Hub & Devices."
            }
            Self::NeverSeen => {
                "Bound but has never checked in. Verify the hub endpoint on that terminal \
                 and restart it — this is a configuration problem, not a network one."
            }
            Self::Online => "Serving normally.",
            Self::Stale => {
                "Checked in recently but not just now. Usually a reboot or a weak link; \
                 look again in a few minutes before treating it as down."
            }
            Self::Offline => {
                "Has not checked in for some time. Confirm the terminal is powered on and \
                 can reach the hub."
            }
        }
    }
}

/// Derive state from evidence.
///
/// `paired` means an installation has claimed this device record. Written as a
/// pure function of its inputs so the rule can be tested at its boundaries
/// without a database, a clock, or a terminal.
pub fn device_state(paired: bool, seconds_since_seen: Option<i64>) -> DeviceState {
    match (paired, seconds_since_seen) {
        (false, _) => DeviceState::Unpaired,
        (true, None) => DeviceState::NeverSeen,
        // A beat from the future is a clock disagreement, not freshness. Treat
        // the terminal as present rather than inventing a negative age.
        (true, Some(age)) if age <= ONLINE_WITHIN_SECS => DeviceState::Online,
        (true, Some(age)) if age <= STALE_WITHIN_SECS => DeviceState::Stale,
        _ => DeviceState::Offline,
    }
}

/// Age of a heartbeat in seconds, or None if there has never been one.
pub fn seconds_since(
    last_heartbeat_at: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<i64> {
    let raw = last_heartbeat_at?.trim();
    if raw.is_empty() {
        return None;
    }
    let then = chrono::DateTime::parse_from_rfc3339(raw)
        .ok()?
        .with_timezone(&chrono::Utc);
    // Clamped at zero: a terminal whose clock runs fast would otherwise produce
    // a negative age and fall through every threshold to Offline, which is the
    // opposite of what its beat proves.
    Some((now - then).num_seconds().max(0))
}

/// How far the terminal's clock is ahead of the hub's, in seconds.
///
/// Both timestamps were captured when the beat was received — the hub's `now`
/// against the terminal's own `sent_at` — so their difference is the skew plus
/// LAN latency, which is milliseconds and irrelevant at warning scale. None
/// when the terminal predates the `sent_at` field.
pub fn clock_skew_secs(
    last_heartbeat_at: Option<&str>,
    heartbeat_sent_at: Option<&str>,
) -> Option<i64> {
    let hub_at = chrono::DateTime::parse_from_rfc3339(last_heartbeat_at?.trim()).ok()?;
    let sent_at = chrono::DateTime::parse_from_rfc3339(heartbeat_sent_at?.trim()).ok()?;
    Some((hub_at - sent_at).num_seconds())
}

#[cfg(test)]
mod tests;

/// One terminal, as an operator needs to see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRow {
    pub device_id: String,
    pub branch_id: String,
    pub device_code: String,
    pub name: String,
    /// Derived every time it is read. There is deliberately no stored status
    /// column behind this — `devices.status` was one, nobody updated it, and
    /// terminals that had never once contacted the hub displayed as "online".
    pub state: String,
    /// What to do about it. Carried with the row so the UI cannot invent its own
    /// wording for a state whose meaning is defined here.
    pub advice: String,
    pub seconds_since_seen: Option<i64>,
    pub last_heartbeat_at: Option<String>,
    pub observed_ip: Option<String>,
    pub app_version: Option<String>,
    pub heartbeat_hub_id: Option<String>,
    /// Seconds the terminal's clock is ahead of the hub's, or None when the
    /// terminal predates the `sent_at` heartbeat field.
    pub clock_skew_secs: Option<i64>,
    pub is_paired: bool,
    pub is_active: bool,
}

/// Every registered terminal with its state worked out from evidence.
///
/// Shared by the Command Center and the ZanAI roster tool so the two cannot
/// disagree about whether a till is online — which, given the reason this
/// derivation exists, would be its own small version of the original bug.
pub async fn roster(pool: &sqlx::SqlitePool) -> crate::errors::AppResult<Vec<TerminalRow>> {
    use sqlx::Row;

    // Which record this installation has claimed. Binding is a separate fact
    // from heartbeats: a terminal can be correctly bound and never have reached
    // the hub, and conflating the two sends somebody to re-register a device
    // that is already registered.
    let own_device: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='device_id'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let rows = sqlx::query(
        "SELECT d.device_id, d.branch_id, d.device_code, d.name, d.is_active,
                d.last_heartbeat_at, d.heartbeat_sent_at, d.observed_ip, d.app_version,
                d.heartbeat_seq, d.heartbeat_hub_id,
                EXISTS(SELECT 1 FROM hub_paired_devices p
                        WHERE p.device_id = d.device_id AND p.revoked_at IS NULL)
                    AS has_live_pairing
           FROM devices d WHERE d.deleted_at IS NULL
          ORDER BY is_active DESC, datetime(COALESCE(last_heartbeat_at,'')) DESC",
    )
    .fetch_all(pool)
    .await?;

    let now = chrono::Utc::now();
    Ok(rows
        .iter()
        .map(|row| {
            let beat: Option<String> = row.get("last_heartbeat_at");
            let age = seconds_since(beat.as_deref(), now);
            let device_id: String = row.get("device_id");
            let paired = own_device.as_deref() == Some(device_id.as_str())
                || row.get::<i64, _>("has_live_pairing") == 1
                || row.get::<i64, _>("heartbeat_seq") > 0
                || beat.is_some();
            let state = device_state(paired, age);
            let sent_at: Option<String> = row.get("heartbeat_sent_at");
            let skew = clock_skew_secs(beat.as_deref(), sent_at.as_deref());
            let advice = if let Some(skew) = skew.filter(|s| s.abs() > CLOCK_SKEW_WARN_SECS) {
                format!(
                    "{} Its clock is {} seconds {} the hub's — its rows can land behind the \
                     sync watermark and go missing. Fix the time on that terminal.",
                    state.advice(),
                    skew.abs(),
                    if skew > 0 { "ahead of" } else { "behind" }
                )
            } else {
                state.advice().to_string()
            };
            TerminalRow {
                device_id,
                branch_id: row.get("branch_id"),
                device_code: row.get("device_code"),
                name: row.get("name"),
                state: state.as_str().to_string(),
                advice,
                seconds_since_seen: age,
                last_heartbeat_at: beat,
                observed_ip: row.get("observed_ip"),
                app_version: row.get("app_version"),
                heartbeat_hub_id: row.get("heartbeat_hub_id"),
                clock_skew_secs: skew,
                is_paired: paired,
                is_active: row.get::<i64, _>("is_active") == 1,
            }
        })
        .collect())
}
