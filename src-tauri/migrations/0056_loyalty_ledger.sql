-- Loyalty points become derived, the way stock already is.
--
-- `customers.loyalty_points` is a counter written with `loyalty_points =
-- loyalty_points + ?` and synced as an absolute value. Two tills awarding the
-- same customer during the same shift each compute a total from what they can
-- see, and only one of those totals survives the merge.
--
-- `apply_customer` already recognised this and merges the column with
-- `MAX(local, incoming)` rather than plain last-writer-wins, which keeps the
-- larger of the two — so the loss is bounded, but it is still a loss: a till
-- that awarded 5 points loses them entirely to one that awarded 10, and the
-- customer is short.
--
-- The convergent shape is the one `stock_movements` already uses: record what
-- happened, derive the total. Two terminals that each hold both events compute
-- the same balance no matter what order the events arrived in, because addition
-- does not care. `customers.loyalty_points` stays as the cached figure the till
-- reads, and stops travelling between terminals.

CREATE TABLE IF NOT EXISTS loyalty_events (
    loyalty_event_id   TEXT PRIMARY KEY,
    customer_id        TEXT NOT NULL REFERENCES customers(customer_id),
    branch_id          TEXT,
    device_id          TEXT,
    -- Which terminal minted this, so the pull filter can skip a device's own
    -- rows coming back at it. Matches every other append-only table.
    origin_device_id   TEXT NOT NULL DEFAULT '',
    -- earn | redeem | adjust | opening
    event_type         TEXT NOT NULL,
    -- Signed. Redemptions are negative; this is what gets summed.
    points_delta       INTEGER NOT NULL,
    -- The running balance as the terminal that wrote this understood it.
    -- Used as the anchor when a terminal holds only part of the history, the
    -- same role `stock_movements.quantity_after` plays.
    points_after       INTEGER NOT NULL,
    reference_type     TEXT,
    reference_id       TEXT,
    reason             TEXT,
    created_by_user_id TEXT,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    sync_status        TEXT NOT NULL DEFAULT 'pending',
    sync_attempts      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_loyalty_events_customer
    ON loyalty_events(customer_id, created_at);
CREATE INDEX IF NOT EXISTS idx_loyalty_events_sync_status
    ON loyalty_events(sync_status);
CREATE INDEX IF NOT EXISTS idx_loyalty_events_reference
    ON loyalty_events(reference_type, reference_id);

-- Opening balances, so the ledger explains the number already on screen.
--
-- Without this every existing customer's balance would read as zero the first
-- time it is recomputed. Written as 'synced' rather than 'pending': every
-- terminal runs this migration against its own copy of `customers` and would
-- otherwise push a duplicate opening row for the same person. The balance is
-- already agreed — it is the history that is missing.
INSERT OR IGNORE INTO loyalty_events (
    loyalty_event_id, customer_id, branch_id, event_type,
    points_delta, points_after, reason, created_at, updated_at, sync_status
)
SELECT
    'LOYOPEN-' || c.customer_id,
    c.customer_id,
    c.branch_id,
    'opening',
    c.loyalty_points,
    c.loyalty_points,
    'Balance carried forward when the loyalty ledger was introduced',
    COALESCE(c.created_at, '2026-01-01T00:00:00Z'),
    COALESCE(c.updated_at, c.created_at, '2026-01-01T00:00:00Z'),
    'synced'
FROM customers c
WHERE c.loyalty_points IS NOT NULL
  AND c.loyalty_points <> 0;
