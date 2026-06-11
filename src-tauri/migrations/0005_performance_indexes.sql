-- 0005_performance_indexes.sql
-- Performance indexes for columns used in WHERE clauses that lacked coverage.
-- All use IF NOT EXISTS so re-running is safe.

-- sales.customer_id — loyalty update and customer history queries
CREATE INDEX IF NOT EXISTS idx_sales_customer_id ON sales(customer_id);

-- sales.status — void, status-filtered list queries
CREATE INDEX IF NOT EXISTS idx_sales_status ON sales(status);

-- sales.updated_at — sync watermark pull filter
CREATE INDEX IF NOT EXISTS idx_sales_updated_at ON sales(updated_at);

-- sale_items.product_id — stock deduction, per-product reporting
CREATE INDEX IF NOT EXISTS idx_sale_items_product_id ON sale_items(product_id);

-- sale_items.updated_at — sync watermark pull filter
CREATE INDEX IF NOT EXISTS idx_sale_items_updated_at ON sale_items(updated_at);

-- sale_items.created_at — date-range report joins
CREATE INDEX IF NOT EXISTS idx_sale_items_created_at ON sale_items(created_at);
