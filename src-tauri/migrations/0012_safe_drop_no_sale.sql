-- Migration 0012: Extend cash_events to support 'safe_drop';
-- add no_sale_events table for cash-drawer-open audit trail.

-- SQLite cannot ALTER a CHECK constraint, so recreate cash_events
-- with the extended allowed set: ('paid_in', 'paid_out', 'safe_drop').
CREATE TABLE IF NOT EXISTS cash_events_v2 (
  cash_event_id       TEXT PRIMARY KEY,
  shift_id            TEXT NOT NULL REFERENCES shifts(shift_id),
  branch_id           TEXT NOT NULL,
  device_id           TEXT NOT NULL,
  event_type          TEXT NOT NULL CHECK(event_type IN ('paid_in','paid_out','safe_drop')),
  amount_minor        INTEGER NOT NULL CHECK(amount_minor > 0),
  note                TEXT,
  created_by_user_id  TEXT NOT NULL,
  created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO cash_events_v2
  SELECT cash_event_id, shift_id, branch_id, device_id, event_type,
         amount_minor, note, created_by_user_id, created_at
  FROM cash_events;

DROP TABLE cash_events;
ALTER TABLE cash_events_v2 RENAME TO cash_events;

CREATE INDEX IF NOT EXISTS idx_cash_events_shift ON cash_events(shift_id);

-- Lightweight no-sale log: records every drawer-open-without-sale action.
-- Monetary reconciliation is unaffected; this is purely for audit.
CREATE TABLE IF NOT EXISTS no_sale_events (
  no_sale_id          TEXT PRIMARY KEY,
  shift_id            TEXT NOT NULL REFERENCES shifts(shift_id),
  branch_id           TEXT NOT NULL,
  device_id           TEXT NOT NULL,
  actor_user_id       TEXT NOT NULL,
  note                TEXT,
  created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_no_sale_shift ON no_sale_events(shift_id);
