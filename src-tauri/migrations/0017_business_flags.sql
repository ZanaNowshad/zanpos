-- 0017_business_flags.sql
-- Seed default values for business behavior flags.
-- INSERT OR IGNORE preserves any value the owner already set
-- (safe to re-run on existing databases).
INSERT OR IGNORE INTO app_config (key, value) VALUES
  ('flag_allow_negative_stock',   '0'),
  ('flag_require_discount_reason','1'),
  ('flag_cashier_can_discount',   '0'),
  ('flag_auto_print_receipt',     '0');
