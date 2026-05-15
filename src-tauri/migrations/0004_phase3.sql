-- Phase 3: Central Sync via Supabase
-- Adds Supabase config keys to app_config.
-- sync_queue and sync_state were already created in 0001_initial.sql.

INSERT OR IGNORE INTO app_config (key, value) VALUES ('supabase_url', '');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('supabase_service_key', '');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('sync_last_online_at', '');
