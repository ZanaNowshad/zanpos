-- Cloud-authoritative enforcement additions
-- cloud_grace_deadline: ISO timestamp when 7-day grace expires.
--   Empty string means either Supabase is configured OR this record hasn't been set yet.
INSERT OR IGNORE INTO app_config(key, value, updated_at)
  VALUES ('cloud_grace_deadline', '', datetime('now'));

-- Retention policy (days). Pruning only removes confirmed-synced rows.
INSERT OR IGNORE INTO app_config(key, value, updated_at)
  VALUES ('retention_days_sales', '90', datetime('now'));
INSERT OR IGNORE INTO app_config(key, value, updated_at)
  VALUES ('retention_days_logs', '30', datetime('now'));

-- Timestamp of last pruning pass (empty = never run)
INSERT OR IGNORE INTO app_config(key, value, updated_at)
  VALUES ('last_prune_at', '', datetime('now'));
