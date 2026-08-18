-- Delivery riders.
--
-- Deliberately separate from `users`. A rider is not a system account: they
-- never log in, never touch a till, and hold no role or PIN. What the shop
-- needs of them is a name and a WhatsApp number to send the drop details to.
-- Putting them in `users` would mean either inventing credentials nobody uses
-- or weakening what a user row guarantees, and it would put riders on the
-- cashier-selection screen at login, where they do not belong.
--
-- `delivery_orders.delivery_staff_name` already records who took a drop, as
-- free text. That column stays as it is — it is history, and a rider later
-- removed from the roster must not blank out past deliveries. `rider_id` is
-- added alongside it so live deliveries can point at the roster row.
CREATE TABLE riders (
    rider_id         TEXT PRIMARY KEY,
    branch_id        TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    name             TEXT NOT NULL,
    -- E.164, e.g. +97333050666. Required: a rider with no number cannot be
    -- sent a drop, which is the whole point of the record.
    phone            TEXT NOT NULL,
    notes            TEXT,
    is_active        INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,
    version          INTEGER NOT NULL DEFAULT 1,
    sync_status      TEXT NOT NULL DEFAULT 'pending',
    sync_attempts    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_riders_branch ON riders(branch_id);
CREATE INDEX idx_riders_sync_status ON riders(sync_status);
-- One roster entry per number, among the living. A deleted rider's number is
-- free to reuse when someone rejoins on a new handset.
CREATE UNIQUE INDEX idx_riders_phone_unique ON riders(phone) WHERE deleted_at IS NULL;

-- Which roster entry took this drop, when one was chosen at checkout.
ALTER TABLE delivery_orders ADD COLUMN rider_id TEXT;
CREATE INDEX idx_delivery_orders_rider ON delivery_orders(rider_id);
