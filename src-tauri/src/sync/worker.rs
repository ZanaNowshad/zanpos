// The old event-sourcing sync worker has been replaced by sync_v2.
// This file re-exports the new implementation so all existing `use crate::sync::SyncWorker`
// imports continue to work without changes.
pub use crate::sync_v2::SyncWorker;
