-- Per-device pairing for the LAN hub on :8923.
--
-- Before this, one shared store token authorised any device, and the
-- x-zanpos-device header was recorded but never verified — a device could
-- claim any identity, and a leaked token could not be withdrawn from one
-- terminal without re-keying every terminal in the shop.
--
-- Only the SHA-256 digest is stored. The raw token is shown once at pairing
-- and never persisted, so a copy of this database does not let anyone join
-- the hub. revoked_at is a tombstone rather than a delete, so a revoked
-- device stays auditable.
CREATE TABLE hub_paired_devices (
    device_id     TEXT PRIMARY KEY NOT NULL,
    device_name   TEXT NOT NULL,
    token_digest  BLOB NOT NULL,
    paired_at     TEXT NOT NULL,
    last_seen_at  TEXT,
    revoked_at    TEXT
);

-- The auth path loads every live device on hub start and after each
-- pair/revoke; this keeps that load cheap as terminals accumulate.
CREATE INDEX idx_hub_paired_devices_live ON hub_paired_devices(device_id) WHERE revoked_at IS NULL;
