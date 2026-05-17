-- Patch 7: Add structured return reason code to refunds
-- Valid codes: customer_return | defective | wrong_item | exchange | other
ALTER TABLE refunds ADD COLUMN return_reason_code TEXT NOT NULL DEFAULT 'other';
