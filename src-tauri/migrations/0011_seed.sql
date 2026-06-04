-- Seed roles
INSERT OR IGNORE INTO roles (role_id, name, created_at) VALUES
    ('01JROLES000000000000000001', 'owner',    '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000002', 'manager',  '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000003', 'cashier',  '2025-01-01T00:00:00Z');

-- Seed branch (placeholder, updated during setup)
INSERT OR IGNORE INTO branches (branch_id, branch_code, name, created_at, updated_at)
VALUES ('01JBRANCH000000000000000001', 'ZAN', 'My Store', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed admin user (deactivated, updated during setup wizard)
INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at)
VALUES ('01JUSERS000000000000000001', '01JBRANCH000000000000000001', 'Admin', 'admin',
        'PLAIN:0000', '01JROLES000000000000000001', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed device (placeholder, made active during setup)
INSERT OR IGNORE INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
VALUES ('01JDEVICE0000000000000001', '01JBRANCH000000000000000001', 'POS01', 'Main Terminal', 'offline', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed business flags
INSERT OR IGNORE INTO app_config (key, value, updated_at) VALUES
    ('flag_allow_negative_stock', '1', '2025-01-01T00:00:00Z'),
    ('flag_require_discount_reason', '1', '2025-01-01T00:00:00Z'),
    ('flag_cashier_can_discount', '1', '2025-01-01T00:00:00Z'),
    ('flag_auto_print_receipt', '0', '2025-01-01T00:00:00Z'),
    ('setup_complete', '0', '2025-01-01T00:00:00Z'),
    ('retention_days_sales', '90', '2025-01-01T00:00:00Z'),
    ('retention_days_logs', '30', '2025-01-01T00:00:00Z');
