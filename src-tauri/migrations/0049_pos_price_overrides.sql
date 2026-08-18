-- Server-side proof that a manager-approved cart price is intentional.
-- The cart itself comes from IPC and is therefore not trusted as authorization.
CREATE TABLE pos_price_overrides (
    cart_line_id          TEXT PRIMARY KEY,
    cart_id               TEXT NOT NULL,
    product_id            TEXT,
    price_minor           INTEGER NOT NULL CHECK (price_minor > 0),
    authorized_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at            TEXT NOT NULL
);

CREATE INDEX idx_pos_price_overrides_cart
    ON pos_price_overrides(cart_id, cart_line_id);
