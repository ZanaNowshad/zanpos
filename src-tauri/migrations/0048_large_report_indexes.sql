-- Composite indexes for stable query plans on million-row stores.
-- Column order follows equality filters first, then date/range and ordering.
CREATE INDEX IF NOT EXISTS idx_sales_branch_date_status
    ON sales(branch_id, business_date, status, sold_at DESC, sale_id DESC);

CREATE INDEX IF NOT EXISTS idx_sale_items_product_voided
    ON sale_items(product_id, voided, sale_id);

-- Stock is retained as decimal text for exact domain behavior. The expression
-- index allows threshold reports to avoid scanning the entire store inventory.
CREATE INDEX IF NOT EXISTS idx_stock_levels_branch_quantity_numeric
    ON stock_levels(branch_id, CAST(quantity_on_hand AS REAL), product_id);
