-- Seed roles
INSERT OR IGNORE INTO roles (role_id, name, created_at) VALUES
    ('01JROLES000000000000000001', 'owner',    '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000002', 'manager',  '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000003', 'cashier',  '2025-01-01T00:00:00Z');


-- Seed tax rules (needed by products and sales)
INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at) VALUES
    ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z'),
    ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed branch (placeholder, updated during setup)
INSERT OR IGNORE INTO branches (branch_id, branch_code, name, created_at, updated_at)
VALUES ('01JBRANCH0000000000000001', 'MAIN', 'My Store', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed admin user (deactivated, updated during setup wizard)
INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at)
VALUES ('01JUSERS000000000000000001', '01JBRANCH0000000000000001', 'Admin', 'admin',
        'PLAIN:0000', '01JROLES000000000000000001', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Legacy user IDs for test compatibility (old schema IDs)
-- Note: admin1 is seeded INACTIVE to match old migration behaviour where
-- PLain: users are deactivated for security. Test helpers re-activate as needed.
INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at) VALUES
    ('01JUSER000000000000ADMIN1', '01JBRANCH0000000000000001', 'Admin', 'admin1',
     'PLAIN:0000', '01JROLES000000000000000001', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z'),
    ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Cashier 1', 'cashier1',
     'PLAIN:0000', '01JROLES000000000000000003', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed device (placeholder, updated during setup)
INSERT OR IGNORE INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
VALUES ('01JDEVICE0000000000000001', '01JBRANCH0000000000000001', 'POS01', 'Main Terminal', 'offline', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');

-- Seed business flags
INSERT OR IGNORE INTO app_config (key, value, updated_at) VALUES
    ('flag_allow_negative_stock', '1', '2025-01-01T00:00:00Z'),
    ('flag_require_discount_reason', '1', '2025-01-01T00:00:00Z'),
    ('flag_cashier_can_discount', '1', '2025-01-01T00:00:00Z'),
    ('flag_auto_print_receipt', '0', '2025-01-01T00:00:00Z'),
    ('setup_complete', '0', '2025-01-01T00:00:00Z'),
    ('retention_days_sales', '90', '2025-01-01T00:00:00Z'),
    ('retention_days_logs', '30', '2025-01-01T00:00:00Z');
