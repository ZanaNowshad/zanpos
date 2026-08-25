-- Making idempotency explicit, and stopping one bad row wedging a terminal.
--
-- Two problems, one migration, because they are the receiving half of the same
-- pipeline.
--
-- 1. Replay protection worked by attempting the insert and reading the unique
--    constraint violation back. It was correct but invisible: nothing recorded
--    that an event had been seen, so "did this sale ever reach us" had no answer
--    and a deliberate replay was impossible.
--
-- 2. A single row that failed to apply failed the whole pull cycle, and a failed
--    cycle blocks watermark advancement. A row that can never apply — an FK to
--    something hard-deleted upstream — therefore stopped *every* table syncing
--    on that terminal, permanently, with no way out but manual surgery.

CREATE TABLE IF NOT EXISTS sync_inbox (
    -- {table}:{pk}:{payload_hash[..16]}. Deterministic rather than a ULID,
    -- because rows arrive over the wire without an id of their own — so an
    -- identical redelivery has to produce an identical key or it would not
    -- deduplicate at all.
    event_id      TEXT PRIMARY KEY,
    table_name    TEXT NOT NULL,
    entity_id     TEXT NOT NULL,
    payload_hash  TEXT NOT NULL,
    source_device TEXT,
    received_at   TEXT NOT NULL,
    processed_at  TEXT,
    status        TEXT NOT NULL DEFAULT 'received',  -- received|applied|duplicate|failed
    attempts      INTEGER NOT NULL DEFAULT 0,
    last_error    TEXT
);

-- Drives the retry sweep and the failed-event view.
CREATE INDEX IF NOT EXISTS idx_sync_inbox_status ON sync_inbox(status, received_at);
-- Answers "what happened to this row", which is the question actually asked
-- when a record is missing and somebody is looking for it.
CREATE INDEX IF NOT EXISTS idx_sync_inbox_entity ON sync_inbox(table_name, entity_id);

CREATE TABLE IF NOT EXISTS sync_dead_letter (
    dead_letter_id  TEXT PRIMARY KEY,
    table_name      TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    -- The row verbatim. Quarantining is only acceptable because the payload is
    -- kept and can be pushed back through apply_row once the cause is fixed;
    -- without this column it would be data loss with extra steps.
    payload_json    TEXT NOT NULL,
    reason          TEXT NOT NULL,
    attempts        INTEGER NOT NULL,
    first_failed_at TEXT NOT NULL,
    last_failed_at  TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'quarantined'  -- quarantined|replayed|discarded
);

CREATE INDEX IF NOT EXISTS idx_dlq_status ON sync_dead_letter(status, last_failed_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_dlq_entity
    ON sync_dead_letter(table_name, entity_id, status);
