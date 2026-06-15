-- Fix undo_records: the Rust ai_admin_repo expects a richer schema than the
-- original 4-column table. Add all missing columns so mutations can complete.
ALTER TABLE undo_records ADD COLUMN entity_type          TEXT NOT NULL DEFAULT '';
ALTER TABLE undo_records ADD COLUMN entity_id            TEXT NOT NULL DEFAULT '';
ALTER TABLE undo_records ADD COLUMN snapshot_json        TEXT NOT NULL DEFAULT '';
ALTER TABLE undo_records ADD COLUMN rollback_tool        TEXT NOT NULL DEFAULT '';
ALTER TABLE undo_records ADD COLUMN rollback_input_json  TEXT NOT NULL DEFAULT '';
ALTER TABLE undo_records ADD COLUMN status               TEXT NOT NULL DEFAULT 'available';
ALTER TABLE undo_records ADD COLUMN undone_at            TEXT;
ALTER TABLE undo_records ADD COLUMN undone_by_user_id    TEXT;
