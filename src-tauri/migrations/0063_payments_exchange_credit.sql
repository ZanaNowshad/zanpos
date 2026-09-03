-- Let the till store the tender it already knows how to build.
--
-- `payments.payment_method` has carried CHECK(payment_method IN
-- ('cash','card','wallet','other')) since the initial migration and no later one
-- widened it. Meanwhile the application grew a fifth tender: `types.ts` declares
-- `exchange_credit` on `PaymentInput`, `posExchange.ts::buildExchangePayments`
-- produces it whenever a customer puts a returned item's credit towards a
-- replacement, and both `shift_repo` and `report_commands` read it back by name
-- when working out how much cash a refund actually took out of the drawer.
--
-- So the write was impossible and the reads always found nothing. An exchange
-- died with "CHECK constraint failed" at the moment the cashier pressed Charge —
-- and worse, it died *after* `create_refund` had already committed the credit,
-- leaving the customer's return recorded and their replacement unsold. The
-- drawer reconciliation, meanwhile, silently assumed no exchange credit had ever
-- been applied to any refund, so it expected cash out of the till for money that
-- never left it.
--
-- Widening the CHECK is what makes the two halves agree. SQLite cannot alter a
-- constraint in place, so the table is rebuilt; the column list is copied
-- verbatim from 0001_initial.sql plus nothing.
CREATE TABLE payments_new (
    payment_id          TEXT PRIMARY KEY,
    sale_id             TEXT NOT NULL REFERENCES sales(sale_id),
    origin_device_id    TEXT NOT NULL DEFAULT '',
    payment_method      TEXT NOT NULL CHECK(payment_method IN
                            ('cash','card','wallet','other','exchange_credit')),
    amount_minor        INTEGER NOT NULL,
    currency            TEXT NOT NULL DEFAULT 'BHD',
    status              TEXT NOT NULL DEFAULT 'approved',
    external_reference  TEXT,
    tendered_minor      INTEGER,
    change_minor        INTEGER,
    recorded_by_user_id TEXT NOT NULL,
    recorded_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    sync_status         TEXT NOT NULL DEFAULT 'pending',
    sync_attempts       INTEGER NOT NULL DEFAULT 0
);

INSERT INTO payments_new
SELECT payment_id, sale_id, origin_device_id, payment_method, amount_minor,
       currency, status, external_reference, tendered_minor, change_minor,
       recorded_by_user_id, recorded_at, created_at, updated_at,
       sync_status, sync_attempts
FROM payments;

DROP TABLE payments;
ALTER TABLE payments_new RENAME TO payments;

CREATE INDEX idx_payments_sale ON payments(sale_id);
CREATE INDEX idx_payments_sync_status ON payments(sync_status);
