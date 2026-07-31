-- Failed receipts, queued for reprint.
--
-- A silent no-print is this product's historical failure mode: the sale is
-- committed, the drawer opens, and the customer waits for paper that never
-- comes. The sale must never be blocked by the printer, so a failure is
-- recorded here instead and surfaced — at the till immediately, and again in
-- the EOD summary so nothing is quietly lost across a shift.
--
-- `lines` holds the already-rendered receipt text rather than a sale id, so a
-- reprint reproduces exactly what should have printed at the time, even if
-- prices or settings changed since.
CREATE TABLE reprint_queue (
    id           TEXT PRIMARY KEY NOT NULL,
    sale_id      TEXT,
    receipt_number TEXT,
    store_name   TEXT NOT NULL,
    lines        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    error        TEXT,
    business_date TEXT NOT NULL,
    printed_at   TEXT
);

-- The till asks "what is still unprinted?" on every EOD and after every
-- successful print, so the pending case is the one worth indexing.
CREATE INDEX idx_reprint_queue_pending ON reprint_queue(business_date) WHERE printed_at IS NULL;
