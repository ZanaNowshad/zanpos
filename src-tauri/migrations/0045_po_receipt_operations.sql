-- Purchase-order receiving: one row per user-intended receiving operation.
--
-- WHY THIS EXISTS
-- `po_receive` was transactional, rejected over-receipt and protected terminal
-- states, but it had no operation identity. A single user action sent twice —
-- a double-click that outran the UI guard, or a retry after a timeout that in
-- fact succeeded — applied twice, because each submission was independently
-- valid: receiving 3 of 10 twice leaves 6 received and no rule violated.
--
-- Over-receipt protection cannot catch this. The server has to be able to tell
-- an accidental replay apart from a legitimate second partial receipt, and the
-- only thing that distinguishes them is the identity of the operation.
--
-- This follows the mechanism the project already uses for the same problem:
-- `sales.idempotency_key` and `refunds.idempotency_key`, both NOT NULL UNIQUE.
-- The uniqueness is enforced by the database inside the receiving transaction,
-- so two concurrent replays cannot both win — one commits, the other hits the
-- constraint and rolls back its stock and cost writes with it.
--
-- The row is also a genuine receiving record: which PO, by whom, when, and how
-- much, which nothing previously captured.

CREATE TABLE po_receipts (
    receipt_id       TEXT PRIMARY KEY,
    po_id            TEXT NOT NULL REFERENCES purchase_orders(po_id),
    -- Client-generated once per user-intended submission and reused across
    -- retries of that same submission. A new receiving action gets a new key.
    idempotency_key  TEXT NOT NULL UNIQUE,
    actor_user_id    TEXT NOT NULL,
    branch_id        TEXT NOT NULL,
    lines_received   INTEGER NOT NULL DEFAULT 0,
    units_received   TEXT NOT NULL DEFAULT '0',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    sync_status      TEXT NOT NULL DEFAULT 'pending',
    sync_attempts    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_po_receipts_po ON po_receipts(po_id);
CREATE INDEX idx_po_receipts_sync ON po_receipts(sync_status);

-- Forward-safe: purely additive. Existing purchase orders, lines, stock levels
-- and cost history are untouched, and receipts recorded before this migration
-- simply have no row here.
