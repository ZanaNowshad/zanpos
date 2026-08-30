-- The terminal's own wall clock when it sent a heartbeat.
--
-- `last_heartbeat_at` is hub time; the difference between the two is the
-- terminal's clock skew. A slow device's rows land behind the sync watermark
-- and are never offered again, so skew is not a cosmetic number — it is a
-- data-loss vector that is otherwise invisible until a table diverges.
-- The hub records it on every accepted beat; the column stays NULL for older
-- terminals that predate the field.
ALTER TABLE devices ADD COLUMN heartbeat_sent_at TEXT;
