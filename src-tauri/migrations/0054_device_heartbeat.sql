-- Evidence that a terminal is actually there.
--
-- `devices.status` was a stored string, written when a row was created and
-- essentially never updated, and the Command Center read it as live truth. So a
-- till that had never once contacted the hub showed as "online", and POS-7757Z
-- showed online with `last_seen_at` and IP both empty since the day it was
-- registered. A status you can be told is not a status you can trust.
--
-- Nothing here is a state. State is *derived* from these observations at read
-- time (`device_state`), so it cannot be stale and cannot be set by hand.
ALTER TABLE devices ADD COLUMN last_heartbeat_at TEXT;
-- The address the hub saw the request come from, not one the terminal claims.
-- A device reporting its own IP tells you what it believes; this tells you where
-- it actually is, which is what you need when two tills disagree.
ALTER TABLE devices ADD COLUMN observed_ip TEXT;
ALTER TABLE devices ADD COLUMN app_version TEXT;
-- Monotonic per device. Lets a replayed or out-of-order beat be recognised
-- rather than treated as fresh contact.
ALTER TABLE devices ADD COLUMN heartbeat_seq INTEGER NOT NULL DEFAULT 0;
-- Which hub answered. A terminal pointed at the wrong hub is otherwise
-- indistinguishable from one that is working.
ALTER TABLE devices ADD COLUMN heartbeat_hub_id TEXT;

CREATE INDEX idx_devices_heartbeat ON devices(branch_id, last_heartbeat_at DESC);
