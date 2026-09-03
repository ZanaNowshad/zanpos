//! Reporting scope: what a report counts as "this terminal's" work.
//!
//! All that remains of the original sync module. The event-sourcing worker it
//! used to hold was replaced by [`crate::sync_v2`], and `sync::worker` lived on
//! as a one-line re-export "so all existing `use crate::sync::SyncWorker`
//! imports continue to work without changes" — a compatibility layer that
//! outlived the compatibility problem, since exactly one import remained and it
//! now names `sync_v2` directly.
//!
//! `scope` is genuinely shared and stays here: `report_commands`,
//! `report_repo`, `phase10a_commands` and `digest` all need the same answer to
//! "whose rows does this report cover", and a second copy of that rule would
//! make two reports of the same day disagree.

pub mod scope;
