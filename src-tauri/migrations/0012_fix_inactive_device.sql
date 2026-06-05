-- Fix seed device that may have been created with is_active=0
-- This ensures app_config_load succeeds on existing databases
UPDATE devices SET is_active = 1, updated_at = '2025-02-01T00:00:00Z'
WHERE device_code = 'POS01' AND is_active = 0;
