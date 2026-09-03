-- Server-side proof that a discount on a cart was actually authorised.
--
-- `pos_apply_bill_discount` and `pos_apply_line_discount` check the manager PIN,
-- enforce the upper bound, demand a reason and write an audit entry — and then
-- return the mutated cart to the frontend. `pos_finalize_sale` accepts a whole
-- cart back over IPC and used the `bill_discount_minor` and
-- `line_discount_minor` fields on it verbatim. So the authorisation was a
-- client-side round trip: a crafted finalize call could carry any discount at
-- all, without a manager, without a reason, and without an audit row.
--
-- This is the same hole `pos_price_overrides` was created to close for prices,
-- and it is closed the same way: the authorising command records the amount it
-- approved, and finalize refuses a discount it cannot find here. The comment on
-- that table applies word for word — the cart comes from IPC and is therefore
-- not authorisation.
--
-- `cart_line_id` is '' for a whole-bill discount rather than NULL, so the
-- primary key actually constrains: SQLite treats NULLs as distinct, and a
-- nullable key column would let one cart hold any number of bill discounts.
CREATE TABLE pos_discount_authorizations (
    cart_id               TEXT NOT NULL,
    cart_line_id          TEXT NOT NULL,
    discount_minor        INTEGER NOT NULL CHECK (discount_minor > 0),
    reason                TEXT,
    authorized_by_user_id TEXT NOT NULL REFERENCES users(user_id),
    created_at            TEXT NOT NULL,
    PRIMARY KEY (cart_id, cart_line_id)
);

CREATE INDEX idx_pos_discount_auth_cart
    ON pos_discount_authorizations(cart_id);
