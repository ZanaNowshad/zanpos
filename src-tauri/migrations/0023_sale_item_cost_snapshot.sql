-- Preserve historical COGS at the moment each sale line is finalized.
-- Future supplier/catalog cost updates must not rewrite old margin reports.
ALTER TABLE sale_items ADD COLUMN cost_minor_snapshot INTEGER;

CREATE INDEX idx_sale_items_cost_snapshot ON sale_items(cost_minor_snapshot);
