-- Suppliers and Purchase Orders domain (AI-managed inventory sourcing)
CREATE TABLE suppliers (
    supplier_id   TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    phone         TEXT,
    email         TEXT,
    contact_name  TEXT,
    address       TEXT,
    notes         TEXT,
    is_active     INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE purchase_orders (
    po_id           TEXT PRIMARY KEY,
    supplier_id     TEXT REFERENCES suppliers(supplier_id),
    status          TEXT NOT NULL DEFAULT 'draft',  -- draft|ordered|partial|received|cancelled
    expected_date   TEXT,
    received_date   TEXT,
    notes           TEXT,
    created_by      TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE purchase_order_lines (
    po_line_id      TEXT PRIMARY KEY,
    po_id           TEXT NOT NULL REFERENCES purchase_orders(po_id),
    product_id      TEXT,
    product_name    TEXT NOT NULL,
    ordered_qty     REAL NOT NULL DEFAULT 0,
    received_qty    REAL NOT NULL DEFAULT 0,
    unit_cost_minor INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_po_supplier ON purchase_orders(supplier_id);
CREATE INDEX idx_po_status   ON purchase_orders(status);
CREATE INDEX idx_pol_po      ON purchase_order_lines(po_id);
