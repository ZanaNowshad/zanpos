-- Keep hub-mode mark-synced maintenance from scanning large tables.
-- These indexes are intentionally narrow: the sync worker filters by sync_status
-- on every synced table when this terminal is the hub.
CREATE INDEX IF NOT EXISTS idx_branches_sync_status ON branches(sync_status);
CREATE INDEX IF NOT EXISTS idx_categories_sync_status ON categories(sync_status);
CREATE INDEX IF NOT EXISTS idx_tax_rules_sync_status ON tax_rules(sync_status);
CREATE INDEX IF NOT EXISTS idx_products_sync_status ON products(sync_status);
CREATE INDEX IF NOT EXISTS idx_devices_sync_status ON devices(sync_status);
CREATE INDEX IF NOT EXISTS idx_users_sync_status ON users(sync_status);
CREATE INDEX IF NOT EXISTS idx_customers_sync_status ON customers(sync_status);
CREATE INDEX IF NOT EXISTS idx_shifts_sync_status ON shifts(sync_status);
CREATE INDEX IF NOT EXISTS idx_sales_sync_status ON sales(sync_status);
CREATE INDEX IF NOT EXISTS idx_sale_items_sync_status ON sale_items(sync_status);
CREATE INDEX IF NOT EXISTS idx_payments_sync_status ON payments(sync_status);
CREATE INDEX IF NOT EXISTS idx_refunds_sync_status ON refunds(sync_status);
CREATE INDEX IF NOT EXISTS idx_refund_items_sync_status ON refund_items(sync_status);
CREATE INDEX IF NOT EXISTS idx_stock_movements_sync_status ON stock_movements(sync_status);
CREATE INDEX IF NOT EXISTS idx_stock_levels_sync_status ON stock_levels(sync_status);
CREATE INDEX IF NOT EXISTS idx_audit_logs_sync_status ON audit_logs(sync_status);
CREATE INDEX IF NOT EXISTS idx_delivery_orders_sync_status ON delivery_orders(sync_status);
CREATE INDEX IF NOT EXISTS idx_product_prices_sync_status ON product_prices(sync_status);
CREATE INDEX IF NOT EXISTS idx_product_cost_history_sync_status ON product_cost_history(sync_status);
CREATE INDEX IF NOT EXISTS idx_cash_events_sync_status ON cash_events(sync_status);
