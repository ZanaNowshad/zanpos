-- ── Migration 0023: UNIQUE index on customers.phone ─────────────────────────
-- Required for INSERT OR IGNORE deduplication during WhatsApp contact import.
-- Safe: only deduplicates data and adds an index; no table recreation.

-- 1. Remove duplicate phone entries — keep the oldest row per phone number
--    (MIN(customer_id) picks the oldest ULID, which sorts lexicographically by time)
DELETE FROM customers
WHERE customer_id NOT IN (
    SELECT MIN(customer_id)
    FROM customers
    WHERE phone IS NOT NULL
    GROUP BY phone
)
AND phone IS NOT NULL;

-- 2. Create partial UNIQUE index (NULL phones are excluded — a customer may have
--    no phone, and multiple such rows are allowed)
CREATE UNIQUE INDEX IF NOT EXISTS idx_customers_phone_unique
    ON customers(phone) WHERE phone IS NOT NULL;
