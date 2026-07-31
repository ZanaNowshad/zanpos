-- Phase 1: AI payment verification (WhatsApp screenshot → local OCR → AI confirm).
-- A pending row is recorded when a delivery receipt is sent over WhatsApp; when the
-- customer replies with a payment screenshot it is resolved to confirmed/failed.
CREATE TABLE payment_confirmations (
    id                    TEXT PRIMARY KEY,
    customer_jid          TEXT NOT NULL,           -- e.g. 97333050666@s.whatsapp.net
    receipt_number        TEXT NOT NULL,
    delivery_id           TEXT,                    -- resolved from receipt_number when paid
    expected_amount_minor INTEGER NOT NULL,
    currency_exponent     INTEGER NOT NULL DEFAULT 3,
    business_name         TEXT NOT NULL DEFAULT '',
    branch_id             TEXT,
    status                TEXT NOT NULL DEFAULT 'pending'
                          CHECK (status IN ('pending','confirmed','failed')),
    ocr_text              TEXT,                    -- raw deterministic OCR output
    amount_found          TEXT,                    -- amount the AI read from the screenshot
    name_matched          INTEGER NOT NULL DEFAULT 0,
    customer_name         TEXT,
    reason                TEXT,                    -- AI explanation (esp. on failure)
    seen                  INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL,
    resolved_at           TEXT
);
CREATE INDEX idx_payconf_jid_status   ON payment_confirmations(customer_jid, status);
CREATE INDEX idx_payconf_status_seen  ON payment_confirmations(status, seen);
