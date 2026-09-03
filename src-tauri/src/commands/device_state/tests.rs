use super::*;

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// The reported defect, as a test.
///
/// POS-7757Z and POS2 both showed "online" with `last_seen` never populated,
/// because status was a stored string nobody updated. A terminal that has never
/// checked in must never read as online, whatever its device record says.
#[test]
fn a_terminal_that_has_never_checked_in_is_never_online() {
    assert_eq!(device_state(true, None), DeviceState::NeverSeen);
    assert_eq!(device_state(false, None), DeviceState::Unpaired);

    // And there is no input at all that produces Online without a beat.
    for paired in [true, false] {
        assert_ne!(device_state(paired, None), DeviceState::Online);
    }
}

/// Unpaired and NeverSeen were both "offline" before, and they need completely
/// different fixes: one is a binding that never happened, the other a binding
/// that happened and a terminal that never phoned home.
#[test]
fn unpaired_outranks_every_other_signal() {
    for age in [None, Some(0), Some(60), Some(100_000)] {
        assert_eq!(
            device_state(false, age),
            DeviceState::Unpaired,
            "age {age:?} overrode an unbound record"
        );
    }
    assert_ne!(
        DeviceState::Unpaired.advice(),
        DeviceState::NeverSeen.advice()
    );
}

#[test]
fn the_thresholds_sit_where_they_are_documented() {
    assert_eq!(device_state(true, Some(0)), DeviceState::Online);
    assert_eq!(
        device_state(true, Some(ONLINE_WITHIN_SECS)),
        DeviceState::Online
    );
    assert_eq!(
        device_state(true, Some(ONLINE_WITHIN_SECS + 1)),
        DeviceState::Stale
    );
    assert_eq!(
        device_state(true, Some(STALE_WITHIN_SECS)),
        DeviceState::Stale
    );
    assert_eq!(
        device_state(true, Some(STALE_WITHIN_SECS + 1)),
        DeviceState::Offline
    );
}

/// A till mid-reboot and a till that died look identical for the first few
/// minutes. Collapsing them into "offline" sends someone to the shop floor for
/// a terminal that is already coming back.
#[test]
fn a_recent_gap_is_stale_rather_than_offline() {
    assert_eq!(device_state(true, Some(5 * 60)), DeviceState::Stale);
    assert_eq!(device_state(true, Some(60 * 60)), DeviceState::Offline);
    assert!(DeviceState::Stale.advice().contains("few minutes"));
}

#[test]
fn a_missing_or_unparseable_timestamp_reads_as_never_seen() {
    let now = now();
    assert_eq!(seconds_since(None, now), None);
    assert_eq!(seconds_since(Some(""), now), None);
    assert_eq!(seconds_since(Some("   "), now), None);
    assert_eq!(seconds_since(Some("not a date"), now), None);
    // Which is the state the reported terminals were actually in.
    assert_eq!(
        device_state(true, seconds_since(Some(""), now)),
        DeviceState::NeverSeen
    );
}

/// A terminal whose clock runs fast would otherwise produce a negative age,
/// fall past every threshold, and report Offline — the opposite of what its
/// beat proves.
#[test]
fn a_clock_running_fast_does_not_flip_a_live_terminal_to_offline() {
    let now = now();
    let future = (now + chrono::Duration::minutes(10)).to_rfc3339();

    assert_eq!(seconds_since(Some(&future), now), Some(0));
    assert_eq!(
        device_state(true, seconds_since(Some(&future), now)),
        DeviceState::Online
    );
}

#[test]
fn a_real_recent_beat_reads_as_online() {
    let now = now();
    let recent = (now - chrono::Duration::seconds(30)).to_rfc3339();
    let age = seconds_since(Some(&recent), now).unwrap();

    assert!((29..=31).contains(&age), "{age}");
    assert_eq!(device_state(true, Some(age)), DeviceState::Online);
}

// ── Clock skew ───────────────────────────────────────────────────────────────
//
// The sign is the entire content of this figure, and it was inverted: a till
// running fast was reported as running slow. Nothing covered it, so the roster
// told operators to move the clock in the wrong direction for however long that
// stood.

/// Positive means the terminal is ahead of the hub. A beat stamped by the
/// terminal at 12:05 that the hub receives at 12:00 is a terminal five minutes
/// fast, not five minutes slow.
#[test]
fn a_terminal_ahead_of_the_hub_reports_a_positive_skew() {
    let skew = clock_skew_secs(Some("2026-08-30T12:00:00Z"), Some("2026-08-30T12:05:00Z"));
    assert_eq!(skew, Some(300), "a fast terminal must read as ahead");
}

#[test]
fn a_terminal_behind_the_hub_reports_a_negative_skew() {
    let skew = clock_skew_secs(Some("2026-08-30T12:00:00Z"), Some("2026-08-30T11:55:00Z"));
    assert_eq!(skew, Some(-300), "a slow terminal must read as behind");
}

/// LAN latency is milliseconds, so an aligned pair reads as zero rather than as
/// a skew worth warning about.
#[test]
fn an_aligned_clock_reports_no_skew_worth_warning_about() {
    let skew =
        clock_skew_secs(Some("2026-08-30T12:00:00Z"), Some("2026-08-30T11:59:59.7Z")).unwrap();
    assert!(skew.abs() <= 1, "{skew}");
    assert!(skew.abs() <= CLOCK_SKEW_WARN_SECS);
}

#[test]
fn a_terminal_without_a_sent_at_has_no_measurable_skew() {
    assert_eq!(clock_skew_secs(Some("2026-08-30T12:00:00Z"), None), None);
    assert_eq!(clock_skew_secs(None, Some("2026-08-30T12:00:00Z")), None);
    assert_eq!(
        clock_skew_secs(Some("2026-08-30T12:00:00Z"), Some("not a date")),
        None
    );
}

/// Every state has to tell an operator something different to do, or the extra
/// states are decoration.
#[test]
fn each_state_carries_distinct_advice() {
    let states = [
        DeviceState::Unpaired,
        DeviceState::NeverSeen,
        DeviceState::Online,
        DeviceState::Stale,
        DeviceState::Offline,
    ];
    let mut advice: Vec<&str> = states.iter().map(|s| s.advice()).collect();
    advice.sort_unstable();
    advice.dedup();
    assert_eq!(
        advice.len(),
        states.len(),
        "two states give the same advice"
    );

    let mut names: Vec<&str> = states.iter().map(|s| s.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), states.len());
}
