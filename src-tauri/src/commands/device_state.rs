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
pub fn seconds_since(last_heartbeat_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
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

#[cfg(test)]
mod tests;
